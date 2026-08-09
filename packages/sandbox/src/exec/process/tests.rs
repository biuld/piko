use super::*;

fn bash_config(cwd: std::path::PathBuf, tty: bool) -> SpawnConfig {
    let mut config = SpawnConfig::default();
    config.shell.shell_path = "bash".into();
    config.tty = tty;
    config.shell.cwd = cwd;
    config.shell.env = vec![("PATH".into(), "/usr/bin:/bin".into())];
    config.max_output_bytes = DEFAULT_MAX_OUTPUT_BYTES;
    config
}

async fn collect(process: &PtyProcess) -> String {
    let mut output = String::new();
    for _ in 0..40 {
        let chunk = process.try_read_output();
        output.push_str(&String::from_utf8_lossy(&chunk.bytes));
        if process.exited() {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let tail = process.try_read_output();
            output.push_str(&String::from_utf8_lossy(&tail.bytes));
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    output
}

#[tokio::test]
async fn pipe_output_has_stable_newlines_and_nonzero_status() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manager = ProcessManager::new();
    let mut config = bash_config(temp.path().to_path_buf(), false);
    config.command = "printf 'one\\ntwo\\n'; exit 7".into();
    let process = manager.start(config).await.expect("start");
    let output = collect(&process).await;
    assert_eq!(output, "one\ntwo\n");
    assert_eq!(process.status().and_then(|status| status.code), Some(7));
}

#[tokio::test]
async fn output_accumulates_for_incremental_reads() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manager = ProcessManager::new();
    let mut config = bash_config(temp.path().to_path_buf(), true);
    config.command = "echo one; sleep 0.3; echo two; sleep 0.3; echo three".into();
    let process = manager.start(config).await.expect("start");
    let output = collect(&process).await;
    assert!(output.contains("one") && output.contains("two") && output.contains("three"));
    assert_eq!(process.status().and_then(|status| status.code), Some(0));
}

#[tokio::test]
async fn write_stdin_feeds_pipe_and_pty_processes() {
    for tty in [false, true] {
        let temp = tempfile::tempdir().expect("tempdir");
        let manager = ProcessManager::new();
        let mut config = bash_config(temp.path().to_path_buf(), tty);
        config.command = "cat".into();
        let process = manager.start(config).await.expect("start");
        assert_eq!(
            process.write_stdin(b"hello-piko\n").await.expect("write"),
            11
        );
        let mut echoed = String::new();
        for _ in 0..40 {
            let chunk = process.try_read_output();
            echoed.push_str(&String::from_utf8_lossy(&chunk.bytes));
            if echoed.contains("hello-piko") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(echoed.contains("hello-piko"), "tty={tty}, got {echoed:?}");
        manager.stop(process.id(), Duration::from_secs(2)).await;
    }
}

#[tokio::test]
async fn stop_terminates_the_process_group() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manager = ProcessManager::new();
    let mut config = bash_config(temp.path().to_path_buf(), true);
    config.command = "echo $$; sleep 30 & wait".into();
    let process = manager.start(config).await.expect("start");
    let pgid: i32 = loop {
        let chunk = process.try_read_output();
        if let Some(line) = String::from_utf8_lossy(&chunk.bytes).lines().next() {
            break line.trim().parse().expect("pgid");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let status = manager
        .stop(process.id(), Duration::from_secs(1))
        .await
        .expect("stop");
    assert!(
        matches!(status.signal, Some(9 | 15)),
        "unexpected {status:?}"
    );
    let probe = std::process::Command::new("kill")
        .args(["-0", &format!("-{pgid}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("kill probe");
    assert!(!probe.success(), "process group {pgid} still alive");
}

#[tokio::test]
async fn list_and_get_round_trip() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manager = ProcessManager::new();
    let mut config = bash_config(temp.path().to_path_buf(), false);
    config.command = "sleep 30".into();
    let process = manager.start(config).await.expect("start");
    assert_eq!(manager.list(), vec![process.id().to_string()]);
    assert!(manager.get(process.id()).is_some());
    let snapshot = manager.list_processes();
    assert_eq!(snapshot[0].command, "sleep 30");
    assert_eq!(snapshot[0].cwd, temp.path());
    assert_eq!(snapshot[0].pid, process.pid());
    manager.stop(process.id(), Duration::from_secs(1)).await;
    assert!(manager.list().is_empty());
}
