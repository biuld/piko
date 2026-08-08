use super::*;
use std::{
    collections::HashSet,
    fs,
    path::{Component, Path},
};

use super::command::parse_shell_command;

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

    pub(super) fn root(root: &Path, cwd: &Path) -> Result<PathBuf, PolicyError> {
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

    pub(super) fn resolve_missing(candidate: &Path) -> Result<PathBuf, PolicyError> {
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

    /// Absolute writable roots resolved against `cwd`, using the same
    /// canonicalization as [`Policy::authorize`] (F-12 safety assessment).
    ///
    /// The projection is the write boundary that `authorize` enforces; it
    /// lets approval-time callers decide whether a write target is fully
    /// constrained without touching the filesystem beyond canonicalization.
    pub fn writable_roots(&self, cwd: &Path) -> Vec<PathBuf> {
        self.write
            .iter()
            .filter_map(|root| Self::root(root, cwd).ok())
            .collect()
    }

    /// Re-resolve `input` and verify it still maps to the previously
    /// authorized `expected` path (TOCTOU guard for file writes).
    ///
    /// Between authorization and execution a concurrent process could swap a
    /// symlink so the same lexical input resolves elsewhere; re-checking with
    /// the caller's original input right before the write closes that window.
    pub fn verify_resolved(
        &self,
        cwd: &Path,
        input: &Path,
        access: Access,
        must_exist: bool,
        expected: &Path,
    ) -> Result<(), PolicyError> {
        let resolved = self.authorize(cwd, input, access, must_exist)?;
        if resolved != expected {
            return Err(PolicyError::Denied(format!(
                "path changed between authorization and execution: {}",
                input.display()
            )));
        }
        Ok(())
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
