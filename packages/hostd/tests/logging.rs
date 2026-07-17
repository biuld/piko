use std::path::Path;

use piko_hostd::logging::{DEFAULT_FILTER, HostdLogCli, LogConfig, init, resolve_config};
use tempfile::tempdir;
use tracing::info;

#[test]
fn logging_init_writes_to_file() {
    let dir = tempdir().expect("tempdir");
    let log_path = dir.path().join("hostd-test.log");
    let cli = HostdLogCli {
        log_file: Some(log_path.clone()),
        log_level: Some(DEFAULT_FILTER.to_string()),
        log_stderr: false,
        no_log: false,
    };
    let config = resolve_config(&cli).expect("resolve config");
    assert_eq!(
        config,
        LogConfig {
            filter: DEFAULT_FILTER.to_string(),
            log_file: Some(log_path.clone()),
            log_stderr: false,
            ansi: false,
        }
    );

    let _guard = init(config).expect("init logging");
    info!(session_id = "sess_test", "integration test log line");

    drop(_guard);
    std::thread::sleep(std::time::Duration::from_millis(50));

    let contents = std::fs::read_to_string(&log_path).expect("read log file");
    assert!(
        contents.contains("integration test log line"),
        "log contents: {contents}"
    );
    assert!(contents.contains("sess_test"));
    assert!(Path::new(&log_path).exists());
}
