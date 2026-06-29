use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalScope {
    Session,
    Workspace,
    Permanent,
}

static BASH_WRAPPERS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s = HashSet::new();
    s.insert("sudo");
    s.insert("env");
    s.insert("nice");
    s.insert("nohup");
    s.insert("time");
    s.insert("flock");
    s.insert("chroot");
    s.insert("su");
    s.insert("doas");
    s
});

static BASH_PROGRAM_LEVEL: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s = HashSet::new();
    let programs = vec![
        "git", "ls", "cat", "find", "grep", "rg", "fd", "curl", "wget", "ssh", "scp", "docker",
        "kubectl", "python", "python3", "node", "tsc", "biome", "eslint", "prettier", "make",
        "cmake", "echo", "printf", "date", "which", "whoami", "uname", "head", "tail", "wc",
        "sort", "uniq", "cut", "tr", "awk", "sed", "xargs", "tee", "diff", "patch", "zip", "unzip",
        "tar", "ps", "df", "du", "ping", "nslookup", "dig",
    ];
    for p in programs {
        s.insert(p);
    }
    s
});

static BASH_PACKAGE_MANAGERS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s = HashSet::new();
    let managers = vec![
        "npm", "yarn", "pnpm", "bun", "pip", "pip3", "poetry", "brew", "apt", "apt-get", "dnf",
        "pacman", "snap", "gem", "cargo", "go", "composer", "rustup", "dotnet",
    ];
    for m in managers {
        s.insert(m);
    }
    s
});

struct ParsedBashCommand {
    program: String,
    subcommand: Option<String>,
}

fn parse_bash_command(command: &str) -> Option<ParsedBashCommand> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let mut i = 0;

    // Skip wrapper prefixes and their flags/assignments.
    while i < tokens.len() && BASH_WRAPPERS.contains(tokens[i]) {
        i += 1; // skip wrapper
        while i < tokens.len() {
            let tok = tokens[i];
            if tok.starts_with('-') && tok.len() > 1 {
                if tok.contains('=') {
                    i += 1;
                    continue;
                }
                i += 1; // skip flag
                if i < tokens.len() && !tokens[i].starts_with('-') && !tokens[i].contains('=') {
                    i += 1; // skip value
                }
                continue;
            }
            if tok.contains('=') {
                i += 1;
                continue;
            }
            break;
        }
    }

    // Skip leading flags/assignments before the real program
    while i < tokens.len() && (tokens[i].starts_with('-') || tokens[i].contains('=')) {
        i += 1;
    }

    if i >= tokens.len() {
        return None;
    }

    let program = tokens[i].to_string();

    // Package manager -> find subcommand
    if BASH_PACKAGE_MANAGERS.contains(program.as_str()) {
        let mut j = i + 1;
        while j < tokens.len() && (tokens[j].starts_with('-') || tokens[j].contains('=')) {
            j += 1;
        }
        if j < tokens.len() {
            return Some(ParsedBashCommand {
                program,
                subcommand: Some(tokens[j].to_string()),
            });
        }
        return Some(ParsedBashCommand {
            program,
            subcommand: None,
        });
    }

    Some(ParsedBashCommand {
        program,
        subcommand: None,
    })
}

pub fn compute_bash_fingerprint(args: &serde_json::Value) -> String {
    let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
    if command.trim().is_empty() {
        return "bash".to_string();
    }

    let parsed = match parse_bash_command(command) {
        Some(p) => p,
        None => return "bash".to_string(),
    };

    let program = parsed.program.as_str();

    // Known program -> program-level fingerprint
    if BASH_PROGRAM_LEVEL.contains(program) {
        return format!("bash:{}", program);
    }

    // Package manager -> program:subcommand fingerprint
    if BASH_PACKAGE_MANAGERS.contains(program) {
        if let Some(ref sub) = parsed.subcommand {
            return format!("bash:{}:{}", program, sub);
        }
        return format!("bash:{}", program);
    }

    // Unknown -> full command string (fallback)
    format!("bash:{}", command)
}

pub fn compute_path_fingerprint(tool_name: &str, _args: &serde_json::Value) -> String {
    tool_name.to_string()
}

pub fn compute_fingerprint(tool_name: &str, tool_args: &serde_json::Value) -> String {
    match tool_name {
        "bash" => compute_bash_fingerprint(tool_args),
        "edit" | "write" | "read" => compute_path_fingerprint(tool_name, tool_args),
        _ => tool_name.to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredApproval {
    pub tool_name: String,
    pub fingerprint: String,
    pub sample_args: Option<serde_json::Value>,
    pub granted_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApprovalsFile {
    pub fingerprints: HashMap<String, StoredApproval>,
}

fn read_approvals_file(path: &Path) -> ApprovalsFile {
    if !path.exists() {
        return ApprovalsFile::default();
    }
    let Ok(content) = fs::read_to_string(path) else {
        return ApprovalsFile::default();
    };
    let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) else {
        return ApprovalsFile::default();
    };
    // Migrate old format: tools[toolName] -> fingerprints[fingerprint]
    if data.get("tools").is_some() && data.get("fingerprints").is_none() {
        return ApprovalsFile::default();
    }
    serde_json::from_value(data).unwrap_or_default()
}

fn write_approvals_file(path: &Path, data: &ApprovalsFile) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(serialized) = serde_json::to_string_pretty(data) {
        let _ = fs::write(path, serialized);
    }
}

pub struct ApprovalStore {
    session_approvals: Mutex<HashMap<String, StoredApproval>>,
    workspace_path: PathBuf,
    permanent_path: PathBuf,
}

impl ApprovalStore {
    pub fn new(cwd: &str) -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let workspace_path = Path::new(cwd).join(".piko").join("approvals.json");
        let permanent_path = home_dir.join(".piko").join("approvals.json");

        Self {
            session_approvals: Mutex::new(HashMap::new()),
            workspace_path,
            permanent_path,
        }
    }

    pub fn is_approved(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
    ) -> Option<ApprovalScope> {
        let fp = compute_fingerprint(tool_name, tool_args);

        // 1. Session scope
        {
            let session = self.session_approvals.lock().unwrap();
            if session.contains_key(&fp) {
                return Some(ApprovalScope::Session);
            }
        }

        // 2. Workspace scope
        {
            let workspace = read_approvals_file(&self.workspace_path);
            if workspace.fingerprints.contains_key(&fp) {
                return Some(ApprovalScope::Workspace);
            }
        }

        // 3. Permanent scope
        {
            let permanent = read_approvals_file(&self.permanent_path);
            if permanent.fingerprints.contains_key(&fp) {
                return Some(ApprovalScope::Permanent);
            }
        }

        None
    }

    pub fn grant(&self, tool_name: &str, tool_args: &serde_json::Value, scope: ApprovalScope) {
        let fp = compute_fingerprint(tool_name, tool_args);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let entry = StoredApproval {
            tool_name: tool_name.to_string(),
            fingerprint: fp.clone(),
            sample_args: Some(tool_args.clone()),
            granted_at: now,
        };

        match scope {
            ApprovalScope::Session => {
                let mut session = self.session_approvals.lock().unwrap();
                session.insert(fp, entry);
            }
            ApprovalScope::Workspace => {
                let mut workspace = read_approvals_file(&self.workspace_path);
                workspace.fingerprints.insert(fp.clone(), entry);
                write_approvals_file(&self.workspace_path, &workspace);
            }
            ApprovalScope::Permanent => {
                let mut permanent = read_approvals_file(&self.permanent_path);
                permanent.fingerprints.insert(fp.clone(), entry);
                write_approvals_file(&self.permanent_path, &permanent);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bash_command() {
        // simple command
        let p = parse_bash_command("git status").unwrap();
        assert_eq!(p.program, "git");
        assert_eq!(p.subcommand, None);

        // wrapper command
        let p = parse_bash_command("sudo git status").unwrap();
        assert_eq!(p.program, "git");
        assert_eq!(p.subcommand, None);

        // wrapper command with flags/assignments
        let p = parse_bash_command("sudo -u root env VAR=1 git status").unwrap();
        assert_eq!(p.program, "git");
        assert_eq!(p.subcommand, None);

        // package manager command
        let p = parse_bash_command("npm install express").unwrap();
        assert_eq!(p.program, "npm");
        assert_eq!(p.subcommand.as_deref(), Some("install"));

        // package manager command with wrapper and flags
        let p = parse_bash_command("sudo yarn --silent add react").unwrap();
        assert_eq!(p.program, "yarn");
        assert_eq!(p.subcommand.as_deref(), Some("add"));
    }

    #[test]
    fn test_compute_bash_fingerprint() {
        let args = serde_json::json!({ "command": "git status" });
        assert_eq!(compute_bash_fingerprint(&args), "bash:git");

        let args = serde_json::json!({ "command": "sudo npm install react" });
        assert_eq!(compute_bash_fingerprint(&args), "bash:npm:install");

        let args = serde_json::json!({ "command": "custom-script.sh -f" });
        assert_eq!(compute_bash_fingerprint(&args), "bash:custom-script.sh -f");
    }

    #[test]
    fn test_approval_store_workflow() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().to_str().unwrap();
        let store = ApprovalStore::new(cwd);

        let args = serde_json::json!({ "command": "git status" });

        // Initially not approved
        assert_eq!(store.is_approved("bash", &args), None);

        // Grant session
        store.grant("bash", &args, ApprovalScope::Session);
        assert_eq!(
            store.is_approved("bash", &args),
            Some(ApprovalScope::Session)
        );

        // Create new store in same cwd (session is cleared, but workspace/permanent remains)
        let store2 = ApprovalStore::new(cwd);
        assert_eq!(store2.is_approved("bash", &args), None);

        // Grant workspace
        store2.grant("bash", &args, ApprovalScope::Workspace);
        assert_eq!(
            store2.is_approved("bash", &args),
            Some(ApprovalScope::Workspace)
        );

        // New store in same cwd should load workspace approval
        let store3 = ApprovalStore::new(cwd);
        assert_eq!(
            store3.is_approved("bash", &args),
            Some(ApprovalScope::Workspace)
        );
    }
}
