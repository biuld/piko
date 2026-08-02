use piko_hostd::logging::{
    DEFAULT_FILTER, HostdLogCli, LogConfig, init, parse_hostd_log_cli, resolve_config,
};
use tracing::info;

#[test]
fn resolve_config_uses_default_filter() {
    let config = resolve_config(&HostdLogCli::default()).expect("resolve config");
    assert_eq!(
        config,
        LogConfig {
            filter: DEFAULT_FILTER.to_string(),
            ansi: true,
        }
    );
}

#[test]
fn parse_hostd_log_cli_ignores_removed_file_flags() {
    let cli = parse_hostd_log_cli([
        "--log-file".to_string(),
        "/tmp/hostd.log".to_string(),
        "--log-level".to_string(),
        "debug".to_string(),
        "--no-log".to_string(),
    ]);
    assert_eq!(cli.log_level.as_deref(), Some("debug"));
    // Removed flags parse to nothing (unknown args are ignored).
    assert!(!cli.log_stderr);
}

#[test]
fn logging_init_installs_console_subscriber() {
    let config = resolve_config(&HostdLogCli::default()).expect("resolve config");
    let _guard = init(config, None).expect("init logging");
    info!(session_id = "sess_test", "integration test log line");
    drop(_guard);
}
