//! Unix PTY implementation for [`super::run`].

use std::ffi::CString;
use std::os::fd::FromRawFd;
use std::os::unix::io::RawFd;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::process::Command;

use crate::platform::{SandboxCommand, build_sandbox_command};

use super::{CommandOutcome, ExecError, ExitStatus, SpawnConfig};

const READ_CHUNK: usize = 8192;
/// Grace allowed for buffered output to drain after the child exits (a
/// background grandchild may keep the PTY open past the shell's death).
const DRAIN_GRACE: Duration = Duration::from_millis(500);

pub(super) async fn run(config: SpawnConfig) -> Result<CommandOutcome, ExecError> {
    if config
        .cancel
        .as_ref()
        .is_some_and(|token| token.is_cancelled())
    {
        return Ok(CommandOutcome {
            status: ExitStatus {
                code: None,
                signal: None,
            },
            output: String::new(),
            truncated: false,
            timed_out: false,
            cancelled: true,
        });
    }

    let cwd = config.shell.cwd.clone();
    let wrapper = match &config.policy {
        Some(policy) => build_sandbox_command(policy, &cwd)?,
        None => None,
    };

    let outcome = run_once(&config, spawn_pty(&config, &cwd, wrapper.as_ref())?).await?;

    // Nested-sandbox SIGABRT not covered by the APP_SANDBOX_CONTAINER_ID
    // heuristic: retry once without the OS wrapper (mirrors runner.rs).
    #[cfg(target_os = "macos")]
    if wrapper.is_some() && outcome.status.signal == Some(6) && outcome.output.is_empty() {
        eprintln!(
            "piko-sandbox: sandbox-exec SIGABRT detected (likely nested sandbox). \
             Falling back to direct execution."
        );
        return run_once(&config, spawn_pty(&config, &cwd, None)?).await;
    }

    Ok(outcome)
}

/// A spawned PTY process: the child plus its process-group id and master fd.
pub(super) struct SpawnedPty {
    pub child: tokio::process::Child,
    pub pid: u32,
    pub master: tokio::io::unix::AsyncFd<std::fs::File>,
}

/// Allocate a PTY, spawn `<wrapper args> -- <shell> -c <command>` (or
/// `<shell> -c <command>` without a wrapper) as a session/process-group
/// leader, and return the child plus its non-blocking master fd.
pub(super) fn spawn_pty(
    config: &SpawnConfig,
    cwd: &Path,
    wrapper: Option<&SandboxCommand>,
) -> Result<SpawnedPty, ExecError> {
    let (master, slave) = open_pty()?;

    let mut cmd = if let Some(wrapper) = wrapper {
        let mut cmd = Command::new(&wrapper.program);
        cmd.args(&wrapper.args).arg(&config.shell.shell_path);
        cmd
    } else {
        Command::new(&config.shell.shell_path)
    };
    cmd.arg("-c")
        .arg(&config.command)
        .current_dir(cwd)
        .env_clear()
        .envs(config.shell.env.iter().cloned())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    // Make the child the session/process-group leader and give it the PTY
    // as its controlling terminal; the single inherited slave fd becomes
    // stdin/stdout/stderr inside the child.
    let slave_fd = slave;
    unsafe {
        cmd.pre_exec(move || {
            let _ = libc::setsid();
            let request: libc::c_ulong = libc::TIOCSCTTY.into();
            libc::ioctl(slave_fd, request, 0);
            if libc::dup2(slave_fd, 0) < 0
                || libc::dup2(slave_fd, 1) < 0
                || libc::dup2(slave_fd, 2) < 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let child = cmd.spawn()?;
    let pid = child
        .id()
        .ok_or_else(|| std::io::Error::other("spawned process has no pid"))?;
    // The parent must close its slave reference or the master never sees EOF
    // once the child exits.
    unsafe {
        libc::close(slave);
    }

    // Master -> non-blocking tokio file; a reader task drains it into the
    // shared bounded buffer.
    let flags = unsafe { libc::fcntl(master, libc::F_GETFL) };
    if flags >= 0 {
        unsafe {
            libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }
    let master_file = unsafe { std::fs::File::from_raw_fd(master) };
    let master = tokio::io::unix::AsyncFd::new(master_file)?;

    Ok(SpawnedPty { child, pid, master })
}

async fn run_once(config: &SpawnConfig, spawned: SpawnedPty) -> Result<CommandOutcome, ExecError> {
    let SpawnedPty {
        mut child,
        pid,
        master,
    } = spawned;

    let output_buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let truncated = Arc::new(AtomicBool::new(false));
    // Internal process-EOF signal (not a piko-comms contract): a sticky
    // Notify permit, set once when the master reaches EOF or errors.
    let eof = Arc::new(tokio::sync::Notify::new());
    {
        let output_buf = Arc::clone(&output_buf);
        let truncated = Arc::clone(&truncated);
        let eof = Arc::clone(&eof);
        let max = config.max_output_bytes;
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
                        let mut buf = output_buf.lock().expect("output buffer");
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
            eof.notify_one();
        });
    }

    let deadline = config.timeout.map(|d| tokio::time::Instant::now() + d);
    let cancel_future = async {
        match &config.cancel {
            Some(token) => token.cancelled().await,
            None => std::future::pending::<()>().await,
        }
    };

    let (timed_out, cancelled, status) = if let Some(deadline) = deadline {
        tokio::select! {
        st = child.wait() => (false, false, map_status(st.expect("child wait"))),
            _ = tokio::time::sleep_until(deadline) => {
                (true, false, terminate_and_wait(&mut child, pid, config.kill_grace).await)
            }
            _ = cancel_future => {
                (false, true, terminate_and_wait(&mut child, pid, config.kill_grace).await)
            }
        }
    } else {
        tokio::select! {
            st = child.wait() => (false, false, map_status(st.expect("child wait"))),
            _ = cancel_future => {
                (false, true, terminate_and_wait(&mut child, pid, config.kill_grace).await)
            }
        }
    };

    // Drain buffered output for a short grace, then take whatever arrived.
    let _ = tokio::time::timeout(DRAIN_GRACE, eof.notified()).await;
    let was_truncated = truncated.load(Ordering::Relaxed);
    let mut output =
        String::from_utf8_lossy(&output_buf.lock().expect("output buffer")).to_string();
    if was_truncated {
        output.push_str(&format!(
            "\n... [output truncated at {} bytes]",
            config.max_output_bytes
        ));
    }

    Ok(CommandOutcome {
        status,
        output,
        truncated: was_truncated,
        timed_out,
        cancelled,
    })
}

/// SIGTERM the process group, wait up to `grace`, then escalate to SIGKILL.
async fn terminate_and_wait(
    child: &mut tokio::process::Child,
    pid: u32,
    grace: Duration,
) -> ExitStatus {
    kill_group(pid, libc::SIGTERM);
    match tokio::time::timeout(grace, child.wait()).await {
        Ok(st) => map_status(st.expect("child wait")),
        Err(_) => {
            kill_group(pid, libc::SIGKILL);
            map_status(child.wait().await.expect("child wait"))
        }
    }
}

pub(super) fn kill_group(pid: u32, signal: i32) {
    // A negative pid targets the whole process group (pgid == pid).
    unsafe {
        libc::kill(-(pid as i32), signal);
    }
}

pub(super) fn map_status(status: std::process::ExitStatus) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus {
        code: status.code(),
        signal: status.signal(),
    }
}

/// Allocate a PTY pair. The slave is returned without `O_NOCTTY` so the
/// child can acquire it as its controlling terminal via `TIOCSCTTY`.
fn open_pty() -> Result<(RawFd, RawFd), ExecError> {
    let master = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY) };
    if master < 0 {
        return Err(ExecError::Pty(format!(
            "posix_openpt: {}",
            std::io::Error::last_os_error()
        )));
    }
    let fail = |what: &str| {
        let err = std::io::Error::last_os_error();
        unsafe {
            libc::close(master);
        }
        ExecError::Pty(format!("{what}: {err}"))
    };
    if unsafe { libc::grantpt(master) } != 0 {
        return Err(fail("grantpt"));
    }
    if unsafe { libc::unlockpt(master) } != 0 {
        return Err(fail("unlockpt"));
    }
    let name_ptr = unsafe { libc::ptsname(master) };
    if name_ptr.is_null() {
        return Err(fail("ptsname"));
    }
    let name = unsafe { std::ffi::CStr::from_ptr(name_ptr) };
    let slave_name = CString::from(name);
    let slave = unsafe { libc::open(slave_name.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
    if slave < 0 {
        return Err(fail("open slave pty"));
    }
    // Default PTY settings echo input back and post-process output; disable
    // both so captured output is clean.
    let mut termios: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(slave, &mut termios) } == 0 {
        termios.c_lflag &= !(libc::ECHO | libc::ECHONL);
        termios.c_oflag &= !libc::OPOST;
        unsafe {
            libc::tcsetattr(slave, libc::TCSANOW, &termios);
        }
    }
    Ok((master, slave))
}
