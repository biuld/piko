//! Long-lived PTY processes (F-08 slice 2).
//!
//! A [`ProcessManager`] owns processes started by the workspace `process`
//! tool. Each [`PtyProcess`] keeps its PTY open across tool calls: output
//! accumulates into a bounded buffer for incremental reads, stdin is
//! writable (`write_stdin`), and stop/cleanup signal the whole process group
//! (SIGTERM → SIGKILL after the grace period).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::unix::{SpawnedPty, kill_group, map_status};
use super::{ExecError, ExitStatus, SpawnConfig};

const READ_CHUNK: usize = 8192;

/// Default cap for a long-lived process's unread output buffer.
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 262_144;

/// A chunk of accumulated output drained by [`PtyProcess::try_read_output`].
#[derive(Debug, Clone, PartialEq)]
pub struct OutputChunk {
    pub bytes: Vec<u8>,
    /// True when bytes were discarded because the unread buffer hit its cap.
    pub truncated: bool,
    pub exited: bool,
    pub status: Option<ExitStatus>,
}

/// Read-only snapshot of a live process, mirroring codex-rs
/// `BackgroundTerminalInfo` plus piko's exit state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub process_id: String,
    pub pid: u32,
    pub command: String,
    pub cwd: PathBuf,
    pub exited: bool,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
}

/// A running PTY process owned by a [`ProcessManager`].
pub struct PtyProcess {
    id: String,
    pid: u32,
    command: String,
    cwd: PathBuf,
    master: Arc<tokio::io::unix::AsyncFd<std::fs::File>>,
    unread: Arc<Mutex<Vec<u8>>>,
    truncated: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
    status: Arc<Mutex<Option<ExitStatus>>>,
    status_notify: Arc<tokio::sync::Notify>,
}

impl PtyProcess {
    fn new(
        id: String,
        spawned: SpawnedPty,
        command: String,
        cwd: PathBuf,
        max_output_bytes: usize,
    ) -> Arc<Self> {
        let SpawnedPty {
            mut child,
            pid,
            master,
        } = spawned;
        let master = Arc::new(master);

        let unread: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let truncated = Arc::new(AtomicBool::new(false));
        let status: Arc<Mutex<Option<ExitStatus>>> = Arc::new(Mutex::new(None));
        let status_notify = Arc::new(tokio::sync::Notify::new());
        let exited = Arc::new(AtomicBool::new(false));

        // Reaper: waits for the child and records the exit status.
        {
            let exited = Arc::clone(&exited);
            let status = Arc::clone(&status);
            let status_notify = Arc::clone(&status_notify);
            tokio::spawn(async move {
                let st = match child.wait().await {
                    Ok(st) => map_status(st),
                    Err(_) => ExitStatus {
                        code: None,
                        signal: None,
                    },
                };
                *status.lock().expect("status") = Some(st);
                exited.store(true, Ordering::Relaxed);
                status_notify.notify_waiters();
            });
        }

        // Reader: drains the master into the bounded unread buffer.
        {
            let master = Arc::clone(&master);
            let unread = Arc::clone(&unread);
            let truncated = Arc::clone(&truncated);
            let max = max_output_bytes;
            tokio::spawn(async move {
                let mut chunk = [0u8; READ_CHUNK];
                loop {
                    let mut guard = match master.readable().await {
                        Ok(guard) => guard,
                        Err(_) => break,
                    };
                    match guard.try_io(|inner| {
                        use std::io::Read;
                        inner.get_ref().read(&mut chunk)
                    }) {
                        Ok(Ok(0)) => break,
                        Ok(Ok(n)) => {
                            let mut buf = unread.lock().expect("unread buffer");
                            if buf.len() < max {
                                let take = (max - buf.len()).min(n);
                                buf.extend_from_slice(&chunk[..take]);
                            } else {
                                truncated.store(true, Ordering::Relaxed);
                            }
                        }
                        Ok(Err(_)) => break,
                        Err(_) => continue,
                    }
                }
            });
        }

        Arc::new(Self {
            id,
            pid,
            command,
            cwd,
            master,
            unread,
            truncated,
            exited,
            status,
            status_notify,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn exited(&self) -> bool {
        self.exited.load(Ordering::Relaxed)
    }

    pub fn status(&self) -> Option<ExitStatus> {
        *self.status.lock().expect("status")
    }

    pub fn info(&self) -> ProcessInfo {
        let status = self.status();
        ProcessInfo {
            process_id: self.id.clone(),
            pid: self.pid,
            command: self.command.clone(),
            cwd: self.cwd.clone(),
            exited: self.exited(),
            exit_code: status.and_then(|s| s.code),
            signal: status.and_then(|s| s.signal),
        }
    }

    /// Write bytes to the process's stdin (the PTY master).
    pub async fn write_stdin(&self, data: &[u8]) -> std::io::Result<usize> {
        let mut written = 0;
        while written < data.len() {
            let mut guard = self.master.writable().await?;
            match guard.try_io(|inner| {
                use std::io::Write;
                inner.get_ref().write(&data[written..])
            }) {
                Ok(Ok(n)) => written += n,
                Ok(Err(err)) => return Err(err),
                Err(_) => continue,
            }
        }
        Ok(written)
    }

    /// Drain output accumulated since the last read (non-blocking).
    pub fn try_read_output(&self) -> OutputChunk {
        let bytes = std::mem::take(&mut *self.unread.lock().expect("unread buffer"));
        OutputChunk {
            bytes,
            truncated: self.truncated.swap(false, Ordering::Relaxed),
            exited: self.exited(),
            status: self.status(),
        }
    }

    /// Wait up to `timeout` for the process to exit.
    pub async fn wait_for_exit(&self, timeout: Duration) -> Option<ExitStatus> {
        if self.exited() {
            return self.status();
        }
        tokio::time::timeout(timeout, self.status_notify.notified())
            .await
            .ok()?;
        self.status()
    }

    /// Terminate the whole process group: SIGTERM, then SIGKILL after the
    /// grace period, and return the final exit status.
    pub async fn stop(&self, grace: Duration) -> ExitStatus {
        kill_group(self.pid, libc::SIGTERM);
        if self.wait_for_exit(grace).await.is_none() {
            kill_group(self.pid, libc::SIGKILL);
            let _ = self.wait_for_exit(grace).await;
        }
        self.status().unwrap_or(ExitStatus {
            code: None,
            signal: Some(9),
        })
    }
}

/// Owns the workspace's long-lived processes, keyed by `proc-N` ids.
pub struct ProcessManager {
    processes: Mutex<HashMap<String, Arc<PtyProcess>>>,
    next_id: AtomicU64,
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Start a long-lived PTY process from a spawn config.
    pub async fn start(&self, config: SpawnConfig) -> Result<Arc<PtyProcess>, ExecError> {
        #[cfg(unix)]
        {
            let cwd = config.shell.cwd.clone();
            let wrapper = match &config.policy {
                Some(policy) => crate::platform::build_sandbox_command(policy, &cwd)?,
                None => None,
            };
            let spawned = super::unix::spawn_pty(&config, &cwd, wrapper.as_ref())?;
            let id = format!("proc-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
            let process = PtyProcess::new(
                id,
                spawned,
                config.command.clone(),
                config.shell.cwd.clone(),
                config.max_output_bytes,
            );
            self.processes
                .lock()
                .expect("process map")
                .insert(process.id().to_string(), Arc::clone(&process));
            Ok(process)
        }
        #[cfg(not(unix))]
        {
            let _ = config;
            Err(ExecError::UnsupportedPlatform)
        }
    }

    pub fn get(&self, id: &str) -> Option<Arc<PtyProcess>> {
        self.processes.lock().expect("process map").get(id).cloned()
    }

    /// Sorted ids of all live processes.
    pub fn list(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .processes
            .lock()
            .expect("process map")
            .keys()
            .cloned()
            .collect();
        ids.sort();
        ids
    }

    /// Snapshots of all live processes (command, cwd, pid, exit state).
    pub fn list_processes(&self) -> Vec<ProcessInfo> {
        let mut processes: Vec<ProcessInfo> = self
            .processes
            .lock()
            .expect("process map")
            .values()
            .map(|process| process.info())
            .collect();
        processes.sort_by(|a, b| a.process_id.cmp(&b.process_id));
        processes
    }

    /// Stop and forget a process, returning its final status.
    pub async fn stop(&self, id: &str, grace: Duration) -> Option<ExitStatus> {
        let process = self.processes.lock().expect("process map").remove(id)?;
        Some(process.stop(grace).await)
    }

    /// Synchronously SIGKILL every live process group (cleanup path).
    fn kill_all(&self) {
        for process in self.processes.lock().expect("process map").values() {
            kill_group(process.pid(), libc::SIGKILL);
        }
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        // Best-effort: the runtime may be gone, but kill() is a plain syscall
        // and still terminates the groups. Reaper tasks observe the exit.
        self.kill_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bash_config(cwd: std::path::PathBuf) -> SpawnConfig {
        let mut config = SpawnConfig::default();
        config.shell.shell_path = "bash".into();
        config.shell.cwd = cwd;
        config.shell.env = vec![("PATH".into(), "/usr/bin:/bin".into())];
        config.max_output_bytes = DEFAULT_MAX_OUTPUT_BYTES;
        config
    }

    #[tokio::test]
    async fn output_accumulates_for_incremental_reads() {
        let temp = tempfile::tempdir().expect("tempdir");
        let manager = ProcessManager::new();
        let mut config = bash_config(temp.path().to_path_buf());
        config.command = "echo one; sleep 0.3; echo two; sleep 0.3; echo three".into();
        let process = manager.start(config).await.expect("start");

        // First read sees at least the first line; incremental reads follow.
        let mut saw_one = false;
        let mut saw_two = false;
        let mut saw_three = false;
        for _ in 0..40 {
            let chunk = process.try_read_output();
            if !chunk.bytes.is_empty() {
                let text = String::from_utf8_lossy(&chunk.bytes).to_string();
                saw_one |= text.contains("one");
                saw_two |= text.contains("two");
                saw_three |= text.contains("three");
            }
            if process.exited() && saw_one && saw_two && saw_three {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            saw_one && saw_two && saw_three,
            "incremental output missing"
        );
        assert!(process.exited());
        assert_eq!(process.status().and_then(|s| s.code), Some(0));
    }

    #[tokio::test]
    async fn write_stdin_feeds_the_process() {
        let temp = tempfile::tempdir().expect("tempdir");
        let manager = ProcessManager::new();
        let mut config = bash_config(temp.path().to_path_buf());
        config.command = "cat".into();
        let process = manager.start(config).await.expect("start");

        let written = process.write_stdin(b"hello-piko\n").await.expect("write");
        assert_eq!(written, 11);
        let mut echoed = String::new();
        for _ in 0..40 {
            let chunk = process.try_read_output();
            echoed.push_str(&String::from_utf8_lossy(&chunk.bytes));
            if echoed.contains("hello-piko") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(echoed.contains("hello-piko"), "got {echoed:?}");

        // EOF by closing stdin is not directly supported (PTY), so stop it.
        let status = manager
            .stop(process.id(), Duration::from_secs(2))
            .await
            .expect("stop");
        assert!(status.code.is_some() || status.signal.is_some());
        assert!(manager.list().is_empty());
    }

    #[tokio::test]
    async fn stop_terminates_the_process_group() {
        let temp = tempfile::tempdir().expect("tempdir");
        let manager = ProcessManager::new();
        let mut config = bash_config(temp.path().to_path_buf());
        // `$$` is the shell pid == process-group id.
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
        // SIGTERM (15) normally suffices; SIGKILL (9) is the escalation path
        // when the group ignores the first signal. Either satisfies stop.
        assert!(
            status.signal == Some(15) || status.signal == Some(9),
            "unexpected stop status {status:?}"
        );

        // The whole group must be gone.
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
        let mut config = bash_config(temp.path().to_path_buf());
        config.command = "sleep 30".into();
        let process = manager.start(config).await.expect("start");
        assert_eq!(manager.list(), vec![process.id().to_string()]);
        assert!(manager.get(process.id()).is_some());
        assert!(manager.get("proc-999").is_none());

        // The snapshot carries command/cwd/pid for the /ps surface.
        let snapshot = manager.list_processes();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].command, "sleep 30");
        assert_eq!(snapshot[0].cwd, temp.path());
        assert_eq!(snapshot[0].pid, process.pid());
        assert!(!snapshot[0].exited);

        let _ = manager.stop(process.id(), Duration::from_secs(1)).await;
        assert!(manager.list().is_empty());
        assert!(manager.list_processes().is_empty());
    }
}
