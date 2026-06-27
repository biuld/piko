use serde::Deserialize;
use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Policy {
    pub version: u32,
    #[serde(default)]
    pub read: Vec<PathBuf>,
    #[serde(default)]
    pub write: Vec<PathBuf>,
    #[serde(default)]
    pub deny: Vec<PathBuf>,
    #[serde(default)]
    pub allowed_commands: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub allow_network: bool,
}

#[derive(Clone, Copy)]
pub enum Access {
    Read,
    Write,
}

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("invalid policy: {0}")]
    Invalid(String),
    #[error("access denied: {0}")]
    Denied(String),
    #[error("unsupported shell syntax: {0}")]
    Shell(String),
    #[error("command is not allowed: {0}")]
    Command(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Policy {
    pub fn load(path: &Path) -> Result<Self, PolicyError> {
        if !path.exists() {
            return Err(PolicyError::Invalid(format!(
                "policy file not found at '{}'",
                path.display()
            )));
        }
        let policy: Self = serde_json::from_slice(&fs::read(path)?)?;
        if policy.version != 1 {
            return Err(PolicyError::Invalid("version must be 1".into()));
        }
        if policy.read.is_empty() {
            return Err(PolicyError::Invalid("read must not be empty".into()));
        }
        Ok(policy)
    }

    fn root(root: &Path, cwd: &Path) -> Result<PathBuf, PolicyError> {
        let root = if root.is_absolute() {
            root.to_path_buf()
        } else {
            cwd.join(root)
        };
        if root.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(PolicyError::Invalid(format!(
                "policy path contains '..': {}",
                root.display()
            )));
        }
        Ok(root.canonicalize().unwrap_or(root))
    }

    fn resolve_missing(candidate: &Path) -> Result<PathBuf, PolicyError> {
        let mut missing = Vec::new();
        let mut ancestor = candidate;
        while !ancestor.exists() {
            missing.push(
                ancestor
                    .file_name()
                    .ok_or_else(|| PolicyError::Denied(candidate.display().to_string()))?,
            );
            ancestor = ancestor
                .parent()
                .ok_or_else(|| PolicyError::Denied(candidate.display().to_string()))?;
        }
        let mut resolved = ancestor.canonicalize()?;
        for part in missing.into_iter().rev() {
            resolved.push(part);
        }
        Ok(resolved)
    }

    pub fn authorize(
        &self,
        cwd: &Path,
        input: &Path,
        access: Access,
        must_exist: bool,
    ) -> Result<PathBuf, PolicyError> {
        let cwd = cwd.canonicalize()?;
        let candidate = if input.is_absolute() {
            input.to_path_buf()
        } else {
            cwd.join(input)
        };
        let resolved = if must_exist || candidate.exists() {
            candidate.canonicalize()?
        } else {
            Self::resolve_missing(&candidate)?
        };
        if resolved
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            return Err(PolicyError::Denied(resolved.display().to_string()));
        }
        for root in &self.deny {
            if resolved.starts_with(Self::root(root, &cwd)?) {
                return Err(PolicyError::Denied(resolved.display().to_string()));
            }
        }
        let roots = match access {
            Access::Read => &self.read,
            Access::Write => &self.write,
        };
        for root in roots {
            if resolved.starts_with(Self::root(root, &cwd)?) {
                return Ok(resolved);
            }
        }
        Err(PolicyError::Denied(resolved.display().to_string()))
    }

    pub fn validate_command(
        &self,
        command: &str,
        cwd: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Expansion makes static command identity ambiguous. The OS sandbox remains the
        // filesystem boundary, but fail closed here instead of pretending to understand it.
        for syntax in ["`", "$(", "${", "<(", ">(", "\n", "\r"] {
            if command.contains(syntax) {
                return Err(
                    PolicyError::Shell(format!("unsupported syntax pattern: {}", syntax)).into(),
                );
            }
        }
        let allowed: HashSet<&str> = self.allowed_commands.iter().map(String::as_str).collect();
        if allowed.is_empty() {
            return Err(PolicyError::Invalid("allowedCommands must not be empty".into()).into());
        }

        let segments = parse_shell_command(command)?;
        for segment in segments {
            let name = Path::new(&segment.binary)
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or(&segment.binary);
            if !allowed.contains(name) {
                return Err(PolicyError::Command(name.into()).into());
            }

            // Statically validate redirections against the filesystem ACL
            for (op, target) in &segment.redirects {
                let access = match op.as_str() {
                    ">" | ">>" | "2>" | "2>>" => Access::Write,
                    "<" => Access::Read,
                    _ => continue, // ignore other redirections for ACL
                };
                self.authorize(
                    cwd,
                    Path::new(target),
                    access,
                    matches!(access, Access::Read),
                )?;
            }
        }
        Ok(())
    }

    pub fn canonicalize_command(
        &self,
        command: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let segments = parse_shell_command(command)?;
        let canonical_segments: Vec<String> = segments.iter().map(|s| s.canonicalize()).collect();
        // Join command segments back with single spaces
        Ok(canonical_segments.join(" && "))
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct CommandSegment {
    pub binary: String,
    pub args: Vec<String>,
    pub redirects: Vec<(String, String)>, // (operator, target)
}

impl CommandSegment {
    pub fn canonicalize(&self) -> String {
        let mut result = self.binary.clone();
        let mut args = self.args.clone();

        // If the first argument is a subcommand (doesn't start with -), keep it attached
        if let Some(first) = args.first() {
            if !first.starts_with('-') {
                result.push(' ');
                result.push_str(first);
                args.remove(0);
            }
        }

        // Separate and sort options/flags
        let mut options = Vec::new();
        let mut positional = Vec::new();

        let mut iter = args.into_iter().peekable();
        while let Some(arg) = iter.next() {
            if arg.starts_with('-') {
                // If it's a flag with value (e.g. -p value or --port value), pair them
                if let Some(next) = iter.peek() {
                    if !next.starts_with('-') {
                        let val = iter.next().unwrap();
                        options.push(format!("{} {}", arg, val));
                        continue;
                    }
                }
                options.push(arg);
            } else {
                positional.push(arg);
            }
        }

        options.sort();

        for opt in options {
            result.push(' ');
            result.push_str(&opt);
        }

        for pos in positional {
            result.push(' ');
            result.push_str(&pos);
        }

        // Canonicalize and sort redirects
        let mut redirects = self.redirects.clone();
        redirects.sort();
        for (op, target) in redirects {
            result.push(' ');
            result.push_str(&op);
            result.push(' ');
            result.push_str(&target);
        }

        result
    }
}

pub fn parse_shell_command(command: &str) -> Result<Vec<CommandSegment>, PolicyError> {
    let raw_tokens = tokenize(command).map_err(|e| PolicyError::Shell(e))?;
    let mut tokens = Vec::new();
    for token in raw_tokens {
        let words = shell_words::split(&token).map_err(|e| PolicyError::Shell(e.to_string()))?;
        if let Some(word) = words.first() {
            tokens.push(word.clone());
        }
    }

    let mut segments = Vec::new();
    let mut current_binary: Option<String> = None;
    let mut current_args = Vec::new();
    let mut current_redirects = Vec::new();

    let mut iter = tokens.into_iter().peekable();
    while let Some(token) = iter.next() {
        match token.as_str() {
            "|" | "&&" | "||" | ";" | "&" => {
                if let Some(bin) = current_binary.take() {
                    segments.push(CommandSegment {
                        binary: bin,
                        args: std::mem::take(&mut current_args),
                        redirects: std::mem::take(&mut current_redirects),
                    });
                }
            }
            ">" | ">>" | "<" | "2>" | "2>>" => {
                let target = iter.next().ok_or_else(|| {
                    PolicyError::Shell(format!("missing target for redirect '{}'", token))
                })?;
                current_redirects.push((token, target));
            }
            _ => {
                if current_binary.is_none() {
                    current_binary = Some(token);
                } else {
                    current_args.push(token);
                }
            }
        }
    }

    if let Some(bin) = current_binary {
        segments.push(CommandSegment {
            binary: bin,
            args: current_args,
            redirects: current_redirects,
        });
    }

    Ok(segments)
}

pub fn tokenize(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if escaped {
            current.push(c);
            escaped = false;
            i += 1;
            continue;
        }

        if c == '\\' && !in_single_quote {
            escaped = true;
            current.push(c);
            i += 1;
            continue;
        }

        if c == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            current.push(c);
            i += 1;
            continue;
        }

        if c == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            current.push(c);
            i += 1;
            continue;
        }

        if in_single_quote || in_double_quote {
            current.push(c);
            i += 1;
            continue;
        }

        if c.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            i += 1;
            continue;
        }

        let mut matched_operator = None;
        for op in &["&&", "||", ">>", "2>>", "2>"] {
            let len = op.len();
            if i + len <= chars.len() {
                let segment: String = chars[i..i + len].iter().collect();
                if segment == *op {
                    matched_operator = Some((op, len));
                    break;
                }
            }
        }

        if matched_operator.is_none() {
            for op in &["|", ";", "&", ">", "<"] {
                if c == op.chars().next().unwrap() {
                    matched_operator = Some((op, 1));
                    break;
                }
            }
        }

        if let Some((op, len)) = matched_operator {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            tokens.push(op.to_string());
            i += len;
        } else {
            current.push(c);
            i += 1;
        }
    }

    if in_single_quote || in_double_quote {
        return Err("unclosed quote".to_string());
    }
    if escaped {
        return Err("trailing backslash".to_string());
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn policy() -> Policy {
        Policy {
            version: 1,
            read: vec![PathBuf::from(".")],
            write: vec![PathBuf::from(".")],
            deny: vec![PathBuf::from(".git")],
            allowed_commands: vec![
                "git".into(),
                "rg".into(),
                "echo".into(),
                "cat".into(),
                "npm".into(),
            ],
            allow_network: false,
        }
    }

    #[test]
    fn authorizes_existing_reads_and_missing_writes_in_workspace() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("input.txt"), "value").unwrap();
        assert!(
            policy()
                .authorize(dir.path(), Path::new("input.txt"), Access::Read, true)
                .is_ok()
        );
        assert!(
            policy()
                .authorize(
                    dir.path(),
                    Path::new("new/dir/output.txt"),
                    Access::Write,
                    false
                )
                .is_ok()
        );
    }

    #[test]
    fn deny_rule_overrides_workspace_root() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/config"), "secret").unwrap();
        assert!(matches!(
            policy().authorize(dir.path(), Path::new(".git/config"), Access::Read, true),
            Err(PolicyError::Denied(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;
        let workspace = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("secret"), "secret").unwrap();
        symlink(outside.path().join("secret"), workspace.path().join("link")).unwrap();
        assert!(matches!(
            policy().authorize(workspace.path(), Path::new("link"), Access::Read, true),
            Err(PolicyError::Denied(_))
        ));
    }

    #[test]
    fn validates_every_static_command_segment() {
        let dir = tempdir().unwrap();
        assert!(
            policy()
                .validate_command("git status && rg TODO", dir.path())
                .is_ok()
        );
        assert!(
            policy()
                .validate_command("git status; rm -rf .", dir.path())
                .is_err()
        );
        assert!(
            policy()
                .validate_command("git $(printf status)", dir.path())
                .is_err()
        );
    }

    #[test]
    fn validates_redirections_against_acl() {
        let dir = tempdir().unwrap();
        // git status > allowed.log -> allowed write path in workspace (.)
        assert!(
            policy()
                .validate_command("git status > allowed.log", dir.path())
                .is_ok()
        );

        // git status > .git/secret.log -> blocked by deny list
        assert!(
            policy()
                .validate_command("git status > .git/secret.log", dir.path())
                .is_err()
        );
    }

    #[test]
    fn canonicalizes_commands_correctly() {
        let p = policy();
        assert_eq!(p.canonicalize_command("git status").unwrap(), "git status");
        assert_eq!(
            p.canonicalize_command("npm install lodash -y --save")
                .unwrap(),
            "npm install --save -y lodash"
        );
        assert_eq!(
            p.canonicalize_command("echo 'hello | world' > output.log")
                .unwrap(),
            "echo hello | world > output.log"
        );
    }
}
