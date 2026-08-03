//! Execution-environment capability discovery (F-08 slice 2).
//!
//! Resolves a *usable* shell (configured → `$SHELL` → known candidates),
//! normalizes `PATH`, and probes for common tools. The result feeds the
//! [`super::ShellSnapshot`] used by every command run and is exposed to the
//! model through the workspace `environment` tool.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Fallback PATH when the environment carries none.
pub const DEFAULT_PATH: &str = "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";

/// Shell fallback chain after the configured/`$SHELL` values.
const CANDIDATE_SHELLS: &[&str] = &["bash", "zsh", "sh", "fish"];

/// Tools probed during discovery (base names only, never secrets).
const COMMON_TOOLS: &[&str] = &[
    "git", "curl", "wget", "rg", "fd", "jq", "node", "npm", "npx", "bun", "cargo", "rustc",
    "python3", "python", "go", "make", "gcc", "clang",
];

/// A snapshot of the execution environment's capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentProfile {
    /// Resolved shell path/name (usable, not just configured).
    pub shell: String,
    pub cwd: PathBuf,
    /// Parsed `PATH` entries, deduplicated and order-preserving.
    pub path: Vec<PathBuf>,
    pub os: &'static str,
    pub arch: &'static str,
    /// Base names of commonly available tools discovered on `PATH`.
    pub tools: Vec<String>,
}

impl EnvironmentProfile {
    /// Discover the execution environment.
    pub fn discover(configured_shell: Option<&str>) -> Self {
        let shell = resolve_shell(configured_shell);
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let path = parse_path(&std::env::var("PATH").unwrap_or_else(|_| DEFAULT_PATH.into()));
        let tools = probe_tools();
        Self {
            shell,
            cwd,
            path,
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            tools,
        }
    }

    /// Rebuild a platform PATH string from the normalized entries.
    pub fn path_string(&self) -> String {
        std::env::join_paths(&self.path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| DEFAULT_PATH.into())
    }
}

/// Pick the first usable shell: configured override → `$SHELL` → known
/// candidates. A shell is usable when it resolves through `PATH` (or exists
/// for absolute paths) and is executable.
pub fn resolve_shell(configured: Option<&str>) -> String {
    let non_empty = |s: &str| !s.trim().is_empty();
    let mut candidates: Vec<String> = Vec::new();
    if let Some(shell) = configured.filter(|s| non_empty(s)) {
        candidates.push(shell.to_string());
    }
    if let Some(shell) = std::env::var("SHELL").ok().filter(|s| non_empty(s)) {
        candidates.push(shell);
    }
    candidates.extend(CANDIDATE_SHELLS.iter().map(|s| s.to_string()));
    for candidate in candidates {
        if shell_usable(&candidate) {
            return candidate;
        }
    }
    "bash".into()
}

fn shell_usable(shell: &str) -> bool {
    let path = Path::new(shell);
    if path.is_absolute() {
        return is_executable(path);
    }
    which(shell).is_some()
}

fn which(name: &str) -> Option<PathBuf> {
    for dir in parse_path(&std::env::var("PATH").unwrap_or_else(|_| DEFAULT_PATH.into())) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok();
        if let Some(c_path) = c_path {
            return unsafe { libc::access(c_path.as_ptr(), libc::X_OK) } == 0;
        }
        false
    }
    #[cfg(not(unix))]
    {
        std::fs::metadata(path).is_ok_and(|m| m.is_file())
    }
}

/// Parse and deduplicate a PATH string, preserving order.
pub fn parse_path(value: &str) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    std::env::split_paths(value)
        .filter(|entry| seen.insert(entry.clone()))
        .collect()
}

/// Probe for the common tools with a single `sh -c` run.
fn probe_tools() -> Vec<String> {
    let script = COMMON_TOOLS
        .iter()
        .map(|tool| format!("command -v {tool} 2>/dev/null"))
        .collect::<Vec<_>>()
        .join("; ");
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&script)
        .output()
        .ok();
    let Some(output) = output else {
        return Vec::new();
    };
    let mut found: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let path = Path::new(line.trim());
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that mutate process env vars must run one at a time (shared
    /// with `exec::tests` so the two modules cannot race each other).
    use super::super::tests::ENV_LOCK;

    #[test]
    fn parse_path_dedupes_and_preserves_order() {
        let path = parse_path("/usr/bin:/bin:/usr/bin:/opt/homebrew/bin");
        assert_eq!(
            path,
            vec![
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
                PathBuf::from("/opt/homebrew/bin"),
            ]
        );
    }

    #[test]
    fn resolve_shell_prefers_configured_when_usable() {
        let _guard = ENV_LOCK.lock().unwrap();
        let original = std::env::var("SHELL").ok();
        unsafe { std::env::set_var("SHELL", "/bin/zsh") };
        // A configured absolute path that exists wins over $SHELL.
        if std::path::Path::new("/bin/sh").exists() {
            assert_eq!(resolve_shell(Some("/bin/sh")), "/bin/sh");
        }
        // $SHELL wins over the fallback candidates when it is usable.
        assert_eq!(resolve_shell(None), "/bin/zsh");
        match original {
            Some(v) => unsafe { std::env::set_var("SHELL", v) },
            None => unsafe { std::env::remove_var("SHELL") },
        }
    }

    #[test]
    fn resolve_shell_falls_back_to_bash() {
        let _guard = ENV_LOCK.lock().unwrap();
        let original = std::env::var("SHELL").ok();
        unsafe { std::env::remove_var("SHELL") };
        // bash is the first candidate and present on macOS/Linux CI.
        assert_eq!(resolve_shell(None), "bash");
        if let Some(v) = original {
            unsafe { std::env::set_var("SHELL", v) };
        }
    }

    #[test]
    fn profile_exposes_constants_and_tools() {
        let profile = EnvironmentProfile::discover(None);
        assert_eq!(profile.os, std::env::consts::OS);
        assert_eq!(profile.arch, std::env::consts::ARCH);
        assert!(!profile.path.is_empty());
        // git is virtually always present on dev machines; the probe is
        // best-effort so only assert the structure when tools were found.
        if !profile.tools.is_empty() {
            assert!(profile.tools.iter().all(|t| !t.is_empty()));
        }
    }
}
