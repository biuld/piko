use std::{env, path::PathBuf};

const DEBUG_LOG_FILTER: &str = "debug,piko_hostd=debug,piko_orchd=debug";

#[derive(Debug, Clone)]
pub struct HostLogConfig {
    pub log_level: Option<String>,
}

#[derive(Debug)]
pub struct CliArgs {
    pub hostd_command: String,
    pub hostd_args: Vec<String>,
    pub session_id: Option<String>,
    pub continue_session: bool,
    pub model_id: Option<String>,
    pub provider: Option<String>,
    pub api_key: Option<String>,
    pub thinking_level: Option<String>,
    pub session_name: Option<String>,
    pub no_tools: bool,
    pub log_level: Option<String>,
    pub debug: bool,
}

impl CliArgs {
    pub fn parse() -> Self {
        Self::parse_from(env::args().skip(1))
    }

    pub fn parse_from<I>(raw_args: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        let mut hostd_command = resolve_hostd_command();
        let mut hostd_args: Vec<String> = env::var("PIKO_HOSTD_ARGS")
            .ok()
            .map(|args| args.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default();
        let mut session_id = None;
        let mut continue_session = false;
        let mut model_id = None;
        let mut provider = None;
        let mut api_key = None;
        let mut thinking_level = None;
        let mut session_name = None;
        let mut no_tools = false;
        let mut log_level = None;
        let mut debug = false;
        let mut args = raw_args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--hostd" => {
                    if let Some(value) = args.next() {
                        hostd_command = value;
                    }
                }
                "--hostd-arg" => {
                    if let Some(value) = args.next() {
                        hostd_args.push(value);
                    }
                }
                "--session" => {
                    session_id = args.next();
                }
                "--continue" | "-c" => {
                    continue_session = true;
                }
                "-m" | "--model" => {
                    model_id = args.next();
                }
                "-p" | "--provider" => {
                    provider = args.next();
                }
                "-k" | "--api-key" => {
                    api_key = args.next();
                }
                "--thinking-level" => {
                    thinking_level = args.next();
                }
                "--name" => {
                    session_name = args.next();
                }
                "--no-tools" => {
                    no_tools = true;
                }
                "--log-level" => {
                    log_level = args.next();
                }
                "--debug" => {
                    debug = true;
                }
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                _ => {}
            }
        }

        Self {
            hostd_command,
            hostd_args,
            session_id,
            continue_session,
            model_id,
            provider,
            api_key,
            thinking_level,
            session_name,
            no_tools,
            log_level,
            debug,
        }
    }

    pub fn host_log_config(&self) -> HostLogConfig {
        let mut log_level = self.log_level.clone();
        if self.debug && log_level.is_none() {
            log_level = Some(DEBUG_LOG_FILTER.to_string());
        }
        if log_level.is_none() {
            log_level = env::var("PIKO_LOG_LEVEL")
                .ok()
                .or_else(|| env::var("RUST_LOG").ok());
        }

        HostLogConfig { log_level }
    }
}

/// Resolve the hostd executable path in priority order:
///
/// 1. `PIKO_HOSTD_PATH` / `PIKO_HOSTD_COMMAND` env var
/// 2. `piko-hostd` next to the running `piko-tui` binary
/// 3. `<workspace-root>/target/debug/piko-hostd`
/// 4. `<workspace-root>/target/release/piko-hostd`
/// 5. Fall back to bare `"piko-hostd"` (rely on PATH)
fn resolve_hostd_command() -> String {
    // 1. explicit env override
    if let Ok(path) = env::var("PIKO_HOSTD_PATH").or_else(|_| env::var("PIKO_HOSTD_COMMAND")) {
        return path;
    }

    // 2. same directory as the running binary
    if let Ok(exe) = env::current_exe() {
        let sibling = exe.parent().unwrap_or(&exe).join("piko-hostd");
        if sibling.exists() {
            return sibling.to_string_lossy().into_owned();
        }
    }

    // 3 & 4. cargo workspace target directories
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

    // 5. rely on PATH
    "piko-hostd".to_string()
}

/// Walk up the directory tree from the current exe looking for the workspace
/// root (identified by having both `Cargo.toml` and `packages/hostd`).
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

fn print_help() {
    println!(
        "piko-tui — Rust ratatui frontend for piko hostd

Usage: piko-tui [options]

Options:
  -m, --model <id>         Model ID to use
  -p, --provider <name>    Provider name
  -k, --api-key <key>      API key (forwarded to hostd)
  --thinking-level <l>     Thinking level (off|low|medium|high)
  --session <id>           Open a specific session by ID
  -c, --continue           Continue the most recent session
  --name <name>            Name for a newly created session
  --no-tools               Disable all tools
  --hostd <path>           Override hostd executable path
  --hostd-arg <arg>        Pass an extra argument to hostd (repeatable)
  --log-level <filter>     Tracing filter for hostd (default: info,piko_hostd=info,piko_orchd=info)
  --debug                  Set hostd log level to debug
  -h, --help               Show this help

Environment:
  PIKO_HOSTD_PATH          hostd executable path (overrides auto-discovery)
  PIKO_HOSTD_COMMAND       alias for PIKO_HOSTD_PATH
  PIKO_HOSTD_ARGS          extra hostd arguments (space-separated)
  PIKO_LOG_LEVEL           tracing filter (also accepts RUST_LOG)

Hostd auto-discovery order:
  1. PIKO_HOSTD_PATH / PIKO_HOSTD_COMMAND env var
  2. hostd binary next to piko-tui
  3. <workspace>/target/debug/hostd
  4. <workspace>/target/release/hostd
  5. hostd on PATH"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_flag_sets_debug_level() {
        let args = CliArgs::parse_from(["--debug".to_string()]);
        let config = args.host_log_config();
        assert_eq!(config.log_level.as_deref(), Some(DEBUG_LOG_FILTER));
    }

    #[test]
    fn log_level_flag_sets_level() {
        let args = CliArgs::parse_from(["--log-level".to_string(), "debug".to_string()]);
        let config = args.host_log_config();
        assert_eq!(config.log_level.as_deref(), Some("debug"));
    }

    #[test]
    fn default_tui_has_no_level_override() {
        let args = CliArgs::parse_from(["--session".to_string(), "abc".to_string()]);
        let config = args.host_log_config();
        assert!(config.log_level.is_none());
    }

    #[test]
    fn removed_file_flags_are_ignored() {
        let args = CliArgs::parse_from([
            "--no-log".to_string(),
            "--log-file".to_string(),
            "/tmp/x.log".to_string(),
            "--debug".to_string(),
        ]);
        let config = args.host_log_config();
        assert_eq!(config.log_level.as_deref(), Some(DEBUG_LOG_FILTER));
    }
}
