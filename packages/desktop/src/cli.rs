//! CLI surface for the desktop shell. Mirrors the TUI's hostd resolution so
//! both frontends share one launch contract.

use std::{env, path::PathBuf};

#[derive(Debug, Clone)]
pub struct CliArgs {
    pub hostd_command: String,
    pub hostd_args: Vec<String>,
    pub log_level: Option<String>,
}

impl CliArgs {
    pub fn parse() -> Self {
        Self::parse_from(env::args().skip(1))
    }

    pub fn parse_from(args: impl IntoIterator<Item = String>) -> Self {
        let mut hostd_command = None;
        let mut hostd_args = Vec::new();
        let mut log_level = None;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--hostd-command" => {
                    hostd_command = args.next();
                }
                "--hostd-args" => {
                    hostd_args = args
                        .next()
                        .map(|value| value.split_whitespace().map(str::to_string).collect())
                        .unwrap_or_default();
                }
                "--log-level" => {
                    log_level = args.next();
                }
                other => {
                    eprintln!("unknown argument: {other}");
                    std::process::exit(2);
                }
            }
        }
        let hostd_command = hostd_command.unwrap_or_else(resolve_hostd_command);
        if log_level.is_none() {
            log_level = env::var("PIKO_LOG_LEVEL")
                .ok()
                .or_else(|| env::var("RUST_LOG").ok());
        }
        Self {
            hostd_command,
            hostd_args,
            log_level,
        }
    }
}

/// Resolve the hostd executable path in priority order:
///
/// 1. `PIKO_HOSTD_PATH` / `PIKO_HOSTD_COMMAND` env var
/// 2. `piko-hostd` next to the running `piko-desktop` binary
/// 3. `<workspace-root>/target/debug/piko-hostd`
/// 4. `<workspace-root>/target/release/piko-hostd`
/// 5. Fall back to bare `"piko-hostd"` (rely on PATH)
fn resolve_hostd_command() -> String {
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
    let mut dir = env::current_dir().ok()?;
    loop {
        if dir.join("Cargo.toml").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hostd_command_and_args() {
        let args = CliArgs::parse_from(
            [
                "--hostd-command",
                "/bin/piko-hostd",
                "--hostd-args",
                "--port 8080",
                "--log-level",
                "debug",
            ]
            .into_iter()
            .map(str::to_string),
        );
        assert_eq!(args.hostd_command, "/bin/piko-hostd");
        assert_eq!(args.hostd_args, vec!["--port", "8080"]);
        assert_eq!(args.log_level.as_deref(), Some("debug"));
    }

    #[test]
    fn defaults_to_env_resolved_command() {
        let args = CliArgs::parse_from(Vec::<String>::new());
        assert!(!args.hostd_command.is_empty());
    }
}
