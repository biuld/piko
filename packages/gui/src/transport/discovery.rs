//! Resolve the hostd executable path.
//!
//! Discovery priority (mirrors TUI `resolve_hostd_command`):
//!
//! 1. `PIKO_HOSTD_PATH` or `PIKO_HOSTD_COMMAND` environment variable.
//! 2. `piko-hostd` binary adjacent to the running `piko-gui` executable.
//! 3. `<workspace-root>/target/debug/piko-hostd`
//! 4. `<workspace-root>/target/release/piko-hostd`
//! 5. Bare `"piko-hostd"` — rely on `PATH` lookup at spawn time.

use std::env;
use std::path::PathBuf;

/// Returns the resolved hostd command string.
pub fn resolve_hostd_command() -> String {
    if let Ok(path) = env::var("PIKO_HOSTD_PATH").or_else(|_| env::var("PIKO_HOSTD_COMMAND")) {
        return path;
    }

    if let Ok(exe) = env::current_exe() {
        let sibling = exe.parent().unwrap_or(&exe).join("piko-hostd");
        if sibling.exists() {
            return sibling.to_string_lossy().into_owned();
        }
    }

    if let Some(root) = find_workspace_root() {
        let debug = root.join("target/debug/piko-hostd");
        if debug.exists() {
            return debug.to_string_lossy().into_owned();
        }
        let release = root.join("target/release/piko-hostd");
        if release.exists() {
            return release.to_string_lossy().into_owned();
        }
    }

    "piko-hostd".to_string()
}

fn find_workspace_root() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let mut dir = exe.parent()?.to_path_buf();
    loop {
        if dir.join("Cargo.toml").exists() && dir.join("packages/hostd").exists() {
            return Some(dir);
        }
        let parent = dir.parent()?.to_path_buf();
        if parent == dir {
            return None;
        }
        dir = parent;
    }
}
