//! Global tracing initialization for hostd (and orchd via shared subscriber).

use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use thiserror::Error;
use tracing_subscriber::{EnvFilter, Registry, fmt, layer::SubscriberExt, util::SubscriberInitExt};

pub const DEFAULT_FILTER: &str = "info,hostd=info,orchd=info";
pub const DEBUG_FILTER: &str = "debug,hostd=debug,orchd=debug";

/// Resolved logging configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogConfig {
    pub filter: String,
    pub log_file: Option<PathBuf>,
    pub log_stderr: bool,
    pub ansi: bool,
}

/// Holds the non-blocking file writer guard; dropping flushes pending log lines.
pub struct LogGuard {
    _file_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

#[derive(Debug, Error)]
pub enum LogError {
    #[error("logging already initialized")]
    AlreadyInitialized,
    #[error("invalid log filter: {0}")]
    InvalidFilter(String),
    #[error("failed to prepare log file {path}: {source}")]
    LogFile { path: PathBuf, source: io::Error },
    #[error("failed to initialize tracing subscriber: {0}")]
    Init(String),
}

/// CLI flags parsed from hostd binary arguments.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HostdLogCli {
    pub log_file: Option<PathBuf>,
    pub log_level: Option<String>,
    pub log_stderr: bool,
    pub no_log: bool,
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Expand a leading `~/` prefix to the user's home directory.
pub fn expand_tilde(path: &Path) -> PathBuf {
    let Some(raw) = path.to_str() else {
        return path.to_path_buf();
    };
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    }
    if let Some(rest) = raw.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    path.to_path_buf()
}

/// Default log directory under the piko home (`~/.piko/logs/`).
pub fn default_log_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".piko/logs")
}

/// Timestamped default log file name: `hostd-YYYYMMDD-HHMMSS.log` (local time).
pub fn default_log_filename() -> String {
    format!("hostd-{}.log", chrono::Local::now().format("%Y%m%d-%H%M%S"))
}

/// Full default log file path under [`default_log_dir`].
pub fn default_log_file_path() -> PathBuf {
    default_log_dir().join(default_log_filename())
}

/// Parse hostd log-related CLI flags from an argument iterator.
pub fn parse_hostd_log_cli<I>(args: I) -> HostdLogCli
where
    I: IntoIterator<Item = String>,
{
    let mut cli = HostdLogCli::default();
    let mut iter = args.into_iter().peekable();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--log-file" => {
                if let Some(value) = iter.next() {
                    cli.log_file = Some(PathBuf::from(value));
                }
            }
            "--log-level" => {
                if let Some(value) = iter.next() {
                    cli.log_level = Some(value);
                }
            }
            "--log-stderr" => {
                cli.log_stderr = true;
            }
            "--no-log" => {
                cli.no_log = true;
            }
            _ => {}
        }
    }

    cli
}

/// Resolve [`LogConfig`] from CLI flags and environment variables.
///
/// Priority: CLI > environment > defaults.
pub fn resolve_config(cli: &HostdLogCli) -> Result<LogConfig, LogError> {
    let filter = cli
        .log_level
        .clone()
        .or_else(|| std::env::var("PIKO_LOG_LEVEL").ok())
        .or_else(|| std::env::var("RUST_LOG").ok())
        .unwrap_or_else(|| DEFAULT_FILTER.to_string());

    let log_file = if logging_disabled(cli) {
        None
    } else {
        cli.log_file
            .clone()
            .or_else(|| std::env::var("PIKO_LOG_FILE").ok().map(PathBuf::from))
            .map(|path| expand_tilde(&path))
            .or(Some(default_log_file_path()))
    };

    let stderr_env = std::env::var("PIKO_LOG_STDERR")
        .ok()
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));

    let log_stderr = if cli.log_stderr || stderr_env {
        true
    } else {
        log_file.is_none()
    };

    Ok(LogConfig {
        filter,
        log_file,
        log_stderr,
        ansi: log_stderr,
    })
}

fn logging_disabled(cli: &HostdLogCli) -> bool {
    cli.no_log
        || std::env::var("PIKO_LOG_DISABLE")
            .ok()
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

/// Initialize the global tracing subscriber once.
pub fn init(config: LogConfig) -> Result<LogGuard, LogError> {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return Err(LogError::AlreadyInitialized);
    }

    let env_filter = EnvFilter::try_new(&config.filter)
        .map_err(|err| LogError::InvalidFilter(err.to_string()))?;

    let mut file_guard = None;

    if let Some(log_path) = config.log_file.as_ref() {
        prepare_log_file(log_path)?;
        let directory = log_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let filename = log_path.file_name().ok_or_else(|| LogError::LogFile {
            path: log_path.clone(),
            source: io::Error::new(io::ErrorKind::InvalidInput, "log file path has no filename"),
        })?;
        let file_appender = tracing_appender::rolling::never(directory, filename);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        file_guard = Some(guard);
        let file_layer = fmt::layer().with_writer(non_blocking).with_ansi(false);
        if config.log_stderr {
            Registry::default()
                .with(env_filter)
                .with(file_layer)
                .with(
                    fmt::layer()
                        .with_writer(std::io::stderr)
                        .with_ansi(config.ansi),
                )
                .try_init()
                .map_err(|err| LogError::Init(err.to_string()))?;
        } else {
            Registry::default()
                .with(env_filter)
                .with(file_layer)
                .try_init()
                .map_err(|err| LogError::Init(err.to_string()))?;
        }
    } else {
        Registry::default()
            .with(env_filter)
            .with(
                fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_ansi(config.ansi),
            )
            .try_init()
            .map_err(|err| LogError::Init(err.to_string()))?;
    }

    Ok(LogGuard {
        _file_guard: file_guard,
    })
}

fn prepare_log_file(path: &Path) -> Result<(), LogError> {
    if let Some(parent) = path.parent() {
        create_dir_mode_0700(parent).map_err(|source| LogError::LogFile {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

fn create_dir_mode_0700(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::fs::DirBuilder;
        use std::os::unix::fs::DirBuilderExt;

        DirBuilder::new().recursive(true).mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_home_and_prefix() {
        let home = dirs::home_dir().expect("home dir");
        assert_eq!(
            expand_tilde(Path::new("~/logs/test.log")),
            home.join("logs/test.log")
        );
        assert_eq!(expand_tilde(Path::new("~")), home);
    }

    #[test]
    fn parse_hostd_log_cli_flags() {
        let cli = parse_hostd_log_cli([
            "--log-file".to_string(),
            "~/logs/hostd.log".to_string(),
            "--log-level".to_string(),
            "debug,orchd=debug".to_string(),
            "--log-stderr".to_string(),
        ]);
        assert_eq!(cli.log_file, Some(PathBuf::from("~/logs/hostd.log")));
        assert_eq!(cli.log_level.as_deref(), Some("debug,orchd=debug"));
        assert!(cli.log_stderr);
    }

    #[test]
    fn resolve_config_cli_overrides_env() {
        let cli = HostdLogCli {
            log_file: Some(PathBuf::from("/tmp/custom.log")),
            log_level: Some("warn".to_string()),
            log_stderr: true,
            no_log: false,
        };
        let config = resolve_config(&cli).expect("resolve");
        assert_eq!(config.filter, "warn");
        assert_eq!(config.log_file, Some(PathBuf::from("/tmp/custom.log")));
        assert!(config.log_stderr);
    }

    #[test]
    fn resolve_config_defaults_to_file_without_overrides() {
        let config = resolve_config(&HostdLogCli::default()).expect("resolve");
        assert_eq!(config.filter, DEFAULT_FILTER);
        assert!(config.log_file.is_some());
        assert!(
            config
                .log_file
                .expect("log file")
                .to_string_lossy()
                .contains(".piko/logs/hostd-")
        );
        assert!(!config.log_stderr);
    }

    #[test]
    fn resolve_config_no_log_disables_file() {
        let config = resolve_config(&HostdLogCli {
            no_log: true,
            ..HostdLogCli::default()
        })
        .expect("resolve");
        assert!(config.log_file.is_none());
        assert!(config.log_stderr);
    }

    #[test]
    fn resolve_config_file_only_without_explicit_stderr() {
        let cli = HostdLogCli {
            log_file: Some(PathBuf::from("/tmp/hostd.log")),
            ..HostdLogCli::default()
        };
        let config = resolve_config(&cli).expect("resolve");
        assert_eq!(config.log_file, Some(PathBuf::from("/tmp/hostd.log")));
        assert!(!config.log_stderr);
    }
}
