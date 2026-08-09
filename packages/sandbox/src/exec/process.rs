//! Long-lived pipe/PTY processes for `exec_command` and `write_stdin`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::unix::{SpawnedPipe, SpawnedPty, kill_group, map_status};
use super::{ExecError, ExitStatus, SpawnConfig};

const READ_CHUNK: usize = 8192;

pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 262_144;

#[derive(Debug, Clone, PartialEq)]
pub struct OutputChunk {
    pub bytes: Vec<u8>,
    /// True when bytes were discarded because the unread buffer hit its cap.
    pub truncated: bool,
    pub exited: bool,
    pub status: Option<ExitStatus>,
    pub termination: Option<TerminationReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationReason {
    TimedOut,
    Cancelled,
    Terminated,
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

pub struct PtyProcess {
    id: String,
    pid: u32,
    command: String,
    cwd: PathBuf,
    input: ProcessInput,
    unread: Arc<Mutex<Vec<u8>>>,
    truncated: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
    status: Arc<Mutex<Option<ExitStatus>>>,
    status_notify: Arc<tokio::sync::Notify>,
    termination: Arc<Mutex<Option<TerminationReason>>>,
    sandboxed: bool,
}

enum ProcessSpawn {
    Pty(SpawnedPty),
    Pipe(SpawnedPipe),
}

enum ProcessInput {
    Pty(Arc<tokio::io::unix::AsyncFd<std::fs::File>>),
    Pipe(Arc<tokio::sync::Mutex<tokio::process::ChildStdin>>),
}

fn append_output(unread: &Mutex<Vec<u8>>, truncated: &AtomicBool, max: usize, bytes: &[u8]) {
    let mut buffer = unread.lock().expect("unread buffer");
    let take = max.saturating_sub(buffer.len()).min(bytes.len());
    buffer.extend_from_slice(&bytes[..take]);
    if take < bytes.len() {
        truncated.store(true, Ordering::Relaxed);
    }
}

fn spawn_pipe_reader<R>(
    mut reader: R,
    unread: Arc<Mutex<Vec<u8>>>,
    truncated: Arc<AtomicBool>,
    max: usize,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut chunk = [0u8; READ_CHUNK];
        loop {
            match reader.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(count) => append_output(&unread, &truncated, max, &chunk[..count]),
            }
        }
    });
}

impl PtyProcess {
    fn new(id: String, spawned: ProcessSpawn, config: SpawnConfig, sandboxed: bool) -> Arc<Self> {
        let command = config.command;
        let cwd = config.shell.cwd;
        let max_output_bytes = config.max_output_bytes;
        let timeout = config.timeout;
        let cancel = config.cancel;
        let kill_grace = config.kill_grace;
        let (mut child, pid, input, pty_reader, pipe_readers) = match spawned {
            ProcessSpawn::Pty(SpawnedPty { child, pid, master }) => {
                let master = Arc::new(master);
                (
                    child,
                    pid,
                    ProcessInput::Pty(Arc::clone(&master)),
                    Some(master),
                    None,
                )
            }
            ProcessSpawn::Pipe(SpawnedPipe {
                child,
                pid,
                stdin,
                stdout,
                stderr,
            }) => (
                child,
                pid,
                ProcessInput::Pipe(Arc::new(tokio::sync::Mutex::new(stdin))),
                None,
                Some((stdout, stderr)),
            ),
        };

        let unread: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let truncated = Arc::new(AtomicBool::new(false));
        let status: Arc<Mutex<Option<ExitStatus>>> = Arc::new(Mutex::new(None));
        let status_notify = Arc::new(tokio::sync::Notify::new());
        let exited = Arc::new(AtomicBool::new(false));
        let termination = Arc::new(Mutex::new(None));

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

        // Deadline/cancellation monitor. Both paths terminate the entire
        // process group and record why; a normal child exit wins the race.
        if timeout.is_some() || cancel.is_some() {
            let exited = Arc::clone(&exited);
            let status_notify = Arc::clone(&status_notify);
            let termination = Arc::clone(&termination);
            tokio::spawn(async move {
                let exited_wait = async {
                    if exited.load(Ordering::Relaxed) {
                        return;
                    }
                    status_notify.notified().await;
                };
                tokio::pin!(exited_wait);

                let reason = match (timeout, cancel) {
                    (Some(timeout), Some(cancel)) => tokio::select! {
                        _ = &mut exited_wait => None,
                        _ = tokio::time::sleep(timeout) => Some(TerminationReason::TimedOut),
                        _ = cancel.cancelled() => Some(TerminationReason::Cancelled),
                    },
                    (Some(timeout), None) => tokio::select! {
                        _ = &mut exited_wait => None,
                        _ = tokio::time::sleep(timeout) => Some(TerminationReason::TimedOut),
                    },
                    (None, Some(cancel)) => tokio::select! {
                        _ = &mut exited_wait => None,
                        _ = cancel.cancelled() => Some(TerminationReason::Cancelled),
                    },
                    (None, None) => None,
                };

                let Some(reason) = reason else { return };
                *termination.lock().expect("termination") = Some(reason);
                kill_group(pid, libc::SIGTERM);
                let exited_wait = async {
                    if exited.load(Ordering::Relaxed) {
                        return;
                    }
                    status_notify.notified().await;
                };
                if tokio::time::timeout(kill_grace, exited_wait).await.is_err() {
                    kill_group(pid, libc::SIGKILL);
                }
            });
        }

        // Reader: drains the master into the bounded unread buffer.
        if let Some(master) = pty_reader {
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
                            append_output(&unread, &truncated, max, &chunk[..n]);
                        }
                        Ok(Err(_)) => break,
                        Err(_) => continue,
                    }
                }
            });
        }
        if let Some((stdout, stderr)) = pipe_readers {
            spawn_pipe_reader(
                stdout,
                Arc::clone(&unread),
                Arc::clone(&truncated),
                max_output_bytes,
            );
            spawn_pipe_reader(
                stderr,
                Arc::clone(&unread),
                Arc::clone(&truncated),
                max_output_bytes,
            );
        }

        Arc::new(Self {
            id,
            pid,
            command,
            cwd,
            input,
            unread,
            truncated,
            exited,
            status,
            status_notify,
            termination,
            sandboxed,
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

    pub fn termination(&self) -> Option<TerminationReason> {
        *self.termination.lock().expect("termination")
    }

    pub fn sandboxed(&self) -> bool {
        self.sandboxed
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

    pub async fn write_stdin(&self, data: &[u8]) -> std::io::Result<usize> {
        match &self.input {
            ProcessInput::Pty(master) => {
                let mut written = 0;
                while written < data.len() {
                    let mut guard = master.writable().await?;
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
            ProcessInput::Pipe(stdin) => {
                use tokio::io::AsyncWriteExt;
                stdin.lock().await.write_all(data).await?;
                Ok(data.len())
            }
        }
    }

    pub fn try_read_output(&self) -> OutputChunk {
        let bytes = std::mem::take(&mut *self.unread.lock().expect("unread buffer"));
        OutputChunk {
            bytes,
            truncated: self.truncated.swap(false, Ordering::Relaxed),
            exited: self.exited(),
            status: self.status(),
            termination: self.termination(),
        }
    }

    /// Wait up to `timeout` for the process to exit.
    pub async fn wait_for_exit(&self, timeout: Duration) -> Option<ExitStatus> {
        let notified = self.status_notify.notified();
        if self.exited() {
            return self.status();
        }
        tokio::time::timeout(timeout, notified).await.ok()?;
        self.status()
    }

    /// Terminate the whole process group: SIGTERM, then SIGKILL after the
    /// grace period, and return the final exit status.
    pub async fn stop(&self, grace: Duration) -> ExitStatus {
        *self.termination.lock().expect("termination") = Some(TerminationReason::Terminated);
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
            let sandboxed = config.policy.is_some();
            let wrapper = match &config.policy {
                Some(policy) => Some(
                    crate::platform::build_sandbox_command(policy, &cwd)?.ok_or_else(|| {
                        ExecError::SandboxUnavailable(
                            "the platform sandbox cannot be nested in this environment".into(),
                        )
                    })?,
                ),
                None => None,
            };
            let spawned = if config.tty {
                ProcessSpawn::Pty(super::unix::spawn_pty(&config, &cwd, wrapper.as_ref())?)
            } else {
                ProcessSpawn::Pipe(super::unix::spawn_pipe(&config, &cwd, wrapper.as_ref())?)
            };
            let id = format!("proc-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
            let process = PtyProcess::new(id, spawned, config, sandboxed);
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

    /// Forget a process after its terminal observation has been delivered.
    pub fn remove(&self, id: &str) -> Option<Arc<PtyProcess>> {
        self.processes.lock().expect("process map").remove(id)
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
mod tests;
