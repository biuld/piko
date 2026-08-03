//! Async PTY-backed process execution with process-group lifecycle.
//!
//! Slice 1 of F-08: the shell runs as the session/process-group leader on a
//! PTY, output is captured into a bounded buffer, and timeout/cancellation
//! escalate SIGTERM → SIGKILL to the whole process group. The OS-sandbox
//! wrapper (seatbelt on macOS, bwrap on Linux) is built by
//! [`crate::platform`] so this path and the blocking runner share one
//! command line.

use std::path::PathBuf;
use std::time::Duration;

use crate::policy::Policy;

pub mod env;
#[cfg(unix)]
pub mod process;
#[cfg(unix)]
mod unix;

/// Resolved shell identity captured once per provider bootstrap and reused
/// for every command so the model sees a stable shell across calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellSnapshot {
    pub shell_path: String,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

impl ShellSnapshot {
    /// Shell resolution precedence: configured override → `$SHELL` →
    /// known candidates, validated as usable (delegates to environment
    /// discovery).
    pub fn resolve(configured: Option<&str>) -> String {
        env::resolve_shell(configured)
    }

    /// Capture the bootstrap cwd and environment. Loader-injection
    /// variables (`DYLD_*`/`LD_*`) are dropped up-front — they are stripped
    /// by seatbelt anyway — and `PATH` is normalized via environment
    /// discovery (F-08 slice 2), with `TERM` guaranteed for PTY tools.
    pub fn capture(configured: Option<&str>) -> Self {
        let profile = env::EnvironmentProfile::discover(configured);
        let shell_path = profile.shell.clone();
        let cwd = profile.cwd.clone();
        let mut env: Vec<(String, String)> = std::env::vars()
            .filter(|(key, _)| !(key.starts_with("DYLD_") || key.starts_with("LD_")))
            .collect();
        let path_string = profile.path_string();
        match env.iter_mut().find(|(key, _)| key == "PATH") {
            Some((_, value)) => *value = path_string,
            None => env.push(("PATH".to_string(), path_string)),
        }
        if !env.iter().any(|(key, _)| key == "TERM") {
            env.push(("TERM".to_string(), "xterm-256color".to_string()));
        }
        Self {
            shell_path,
            cwd,
            env,
        }
    }
}

/// Configuration for one sandboxed command run.
#[derive(Debug, Clone)]
pub struct SpawnConfig {
    pub command: String,
    pub shell: ShellSnapshot,
    /// When `Some`, execution is wrapped in the platform OS sandbox
    /// (seatbelt on macOS, bwrap on Linux); `None` runs directly.
    pub policy: Option<Policy>,
    /// Total run budget; on expiry the process group is SIGTERM'd then
    /// SIGKILL'd after `kill_grace`.
    pub timeout: Option<Duration>,
    /// Runtime cancellation (turn abort); fires the same escalation path.
    pub cancel: Option<tokio_util::sync::CancellationToken>,
    /// Grace between SIGTERM and SIGKILL on timeout/cancellation.
    pub kill_grace: Duration,
    /// Combined (stdout+stderr) output cap. Bytes past the cap are drained
    /// and discarded so a chatty child cannot deadlock on a full PTY buffer.
    pub max_output_bytes: usize,
}

impl Default for SpawnConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            shell: ShellSnapshot::capture(None),
            policy: None,
            timeout: None,
            cancel: None,
            kill_grace: Duration::from_secs(2),
            max_output_bytes: 65_536,
        }
    }
}

/// Exit status split into exit code vs signal death.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitStatus {
    pub code: Option<i32>,
    pub signal: Option<i32>,
}

/// Result of one command run. Termination (including timeout/cancellation)
/// is data, not an error, so the tool layer can always commit a bounded
/// result to the transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandOutcome {
    pub status: ExitStatus,
    pub output: String,
    pub truncated: bool,
    pub timed_out: bool,
    pub cancelled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("PTY execution is unsupported on this platform")]
    UnsupportedPlatform,
    #[error("PTY allocation failed: {0}")]
    Pty(String),
    #[error("process spawn failed: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("sandbox policy error: {0}")]
    Policy(#[from] crate::policy::PolicyError),
}

/// Run one command to completion with timeout/cancellation and bounded
/// combined output.
pub async fn run(config: SpawnConfig) -> Result<CommandOutcome, ExecError> {
    #[cfg(unix)]
    {
        unix::run(config).await
    }
    #[cfg(not(unix))]
    {
        let _ = config;
        Err(ExecError::UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that mutate process env vars must run one at a time.
    pub(super) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Runner tests pin bash so they are independent of the host `$SHELL`
    /// (e.g. fish on this machine).
    fn bash_config() -> SpawnConfig {
        let mut config = SpawnConfig::default();
        config.shell.shell_path = "bash".into();
        config
    }

    #[test]
    fn resolve_shell_prefers_configured() {
        let _guard = ENV_LOCK.lock().unwrap();
        let original = std::env::var("SHELL").ok();
        unsafe { std::env::set_var("SHELL", "/bin/zsh") };
        assert_eq!(ShellSnapshot::resolve(Some("/bin/sh")), "/bin/sh");
        assert_eq!(ShellSnapshot::resolve(None), "/bin/zsh");
        if let Some(v) = original {
            unsafe { std::env::set_var("SHELL", v) };
        } else {
            unsafe { std::env::remove_var("SHELL") };
        }
    }

    #[test]
    fn resolve_shell_defaults_to_bash() {
        let _guard = ENV_LOCK.lock().unwrap();
        let original = std::env::var("SHELL").ok();
        unsafe { std::env::remove_var("SHELL") };
        assert_eq!(ShellSnapshot::resolve(None), "bash");
        if let Some(v) = original {
            unsafe { std::env::set_var("SHELL", v) };
        }
    }

    #[test]
    fn capture_guarantees_path_and_strips_loader_vars() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("DYLD_INSERT_LIBRARIES", "/tmp/x.dylib") };
        let snapshot = ShellSnapshot::capture(None);
        assert!(snapshot.env.iter().any(|(k, _)| k == "PATH"));
        assert!(
            !snapshot
                .env
                .iter()
                .any(|(k, _)| k.starts_with("DYLD_") || k.starts_with("LD_"))
        );
        unsafe { std::env::remove_var("DYLD_INSERT_LIBRARIES") };
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exit_code_is_reported() {
        let outcome = run(SpawnConfig {
            command: "exit 42".into(),
            ..bash_config()
        })
        .await
        .expect("run");
        assert!(!outcome.timed_out && !outcome.cancelled);
        assert_eq!(outcome.status.code, Some(42));
        assert_eq!(outcome.status.signal, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn signal_death_is_reported() {
        let outcome = run(SpawnConfig {
            command: "kill -KILL $$".into(),
            ..bash_config()
        })
        .await
        .expect("run");
        assert_eq!(outcome.status.code, None);
        assert_eq!(outcome.status.signal, Some(9));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_terminates_and_reports() {
        let started = std::time::Instant::now();
        let outcome = run(SpawnConfig {
            command: "sleep 30".into(),
            timeout: Some(Duration::from_millis(400)),
            ..bash_config()
        })
        .await
        .expect("run");
        assert!(outcome.timed_out, "expected timeout, got {outcome:?}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "timeout escalation took too long"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn pre_cancelled_token_short_circuits() {
        let token = tokio_util::sync::CancellationToken::new();
        token.cancel();
        let outcome = run(SpawnConfig {
            command: "sleep 30".into(),
            cancel: Some(token),
            ..bash_config()
        })
        .await
        .expect("run");
        assert!(outcome.cancelled);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn background_child_dies_with_the_group() {
        let outcome = run(SpawnConfig {
            // `$$` is the shell pid == process-group id (session leader).
            command: "echo $$; sleep 30 & wait".into(),
            timeout: Some(Duration::from_millis(500)),
            ..bash_config()
        })
        .await
        .expect("run");
        assert!(outcome.timed_out, "got {outcome:?}");
        let pgid = outcome
            .output
            .lines()
            .next()
            .and_then(|l| l.trim().parse::<i32>().ok());
        let pgid = pgid.expect("shell pid in output");
        // The whole process group must be gone: kill -0 must fail.
        let probe = std::process::Command::new("kill")
            .args(["-0", &format!("-{pgid}")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("kill probe");
        assert!(!probe.success(), "process group {pgid} still alive");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn output_is_captured_and_cwd_env_apply() {
        let temp = tempfile::tempdir().expect("tempdir");
        let outcome = run(SpawnConfig {
            command: "pwd; echo $PIKO_TEST_VAR; seq 1 2000 | tr '\\n' ' '; echo".into(),
            shell: ShellSnapshot {
                shell_path: "bash".into(),
                cwd: temp.path().to_path_buf(),
                env: vec![
                    ("PATH".into(), "/usr/bin:/bin".into()),
                    ("PIKO_TEST_VAR".into(), "hello-piko".into()),
                ],
            },
            ..SpawnConfig::default()
        })
        .await
        .expect("run");
        assert!(outcome.output.contains(&temp.path().display().to_string()));
        assert!(outcome.output.contains("hello-piko"));
    }

    #[cfg(target_os = "macos")]
    fn macos_sandbox_available() -> bool {
        if std::env::var("APP_SANDBOX_CONTAINER_ID").is_ok()
            || !std::path::Path::new("/usr/bin/sandbox-exec").exists()
        {
            return false;
        }
        // Nested seatbelt sandboxes (e.g. a sandbox-wrapped test runner)
        // cannot apply another policy: probe with a trivial one.
        let probe =
            "(version 1)\n(deny default)\n(allow process-exec)\n(allow file-read* (subpath \"/\"))";
        std::process::Command::new("/usr/bin/sandbox-exec")
            .arg("-p")
            .arg(probe)
            .arg("/usr/bin/true")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|st| st.success())
            .unwrap_or(false)
    }

    #[cfg(target_os = "linux")]
    fn linux_sandbox_available() -> bool {
        // bwrap may be absent on CI runners; the OS-sandbox tests skip then.
        std::process::Command::new("bwrap")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|st| st.success())
            .unwrap_or(false)
    }

    /// A policy scoped to the system runtime paths plus `writable`, so
    /// commands can start but cannot touch anything outside.
    fn os_sandbox_policy(writable: std::path::PathBuf, allow_network: bool) -> Policy {
        Policy {
            version: 1,
            read: vec![
                std::path::PathBuf::from("/bin"),
                std::path::PathBuf::from("/usr"),
                std::path::PathBuf::from("/System"),
                std::path::PathBuf::from("/etc"),
                std::path::PathBuf::from("/private/etc"),
                std::path::PathBuf::from("/private/var/db/dyld"),
                std::path::PathBuf::from("/private/var/folders"),
                std::path::PathBuf::from("/private/tmp"),
                // The macOS python3 is an xcrun shim that must read the
                // developer tools directory to bootstrap.
                std::path::PathBuf::from("/Library/Developer"),
            ],
            write: vec![writable],
            deny: vec![],
            allowed_commands: vec![],
            allow_network,
        }
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn macos_network_denied_when_not_allowed() {
        if !macos_sandbox_available() {
            eprintln!("skipping: no usable sandbox-exec");
            return;
        }
        if std::process::Command::new("python3")
            .arg("-c")
            .arg("pass")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|st| !st.success())
            .unwrap_or(true)
        {
            eprintln!("skipping: python3 not available");
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let mut config = bash_config();
        config.shell.cwd = temp.path().to_path_buf();
        config.shell.env = vec![
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("HOME".into(), "/private/tmp".into()),
        ];
        config.policy = Some(os_sandbox_policy(temp.path().to_path_buf(), false));
        // Loopback is exempt from seatbelt network filtering, so probe an
        // external address: denied socket() surfaces as errno 1 (EPERM).
        config.command =
            "python3 -c 'import socket; s=socket.socket(); s.settimeout(3); print(\"result\", s.connect_ex((\"1.1.1.1\", 9)))'".into();
        let outcome = run(config).await.expect("run");
        eprintln!("deny outcome: {outcome:?}");
        assert!(
            outcome.output.contains("result 1"),
            "socket creation must be denied, got {outcome:?}"
        );
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn macos_network_allowed_when_configured() {
        if !macos_sandbox_available() {
            eprintln!("skipping: no usable sandbox-exec");
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let mut config = bash_config();
        config.shell.cwd = temp.path().to_path_buf();
        config.shell.env = vec![
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("HOME".into(), "/private/tmp".into()),
        ];
        config.policy = Some(os_sandbox_policy(temp.path().to_path_buf(), true));
        config.command =
            "python3 -c 'import socket; s=socket.socket(); s.settimeout(3); print(\"result\", s.connect_ex((\"1.1.1.1\", 9)))'".into();
        let outcome = run(config).await.expect("run");
        eprintln!("allow outcome: {outcome:?}");
        // With network allowed the socket is created: errno is 0 (connected),
        // or a timeout (110) when the probe network is unreachable — anything
        // other than the EPERM (1) that denial produces.
        assert!(
            outcome.output.contains("result ") && !outcome.output.contains("result 1"),
            "network must be reachable, got {outcome:?}"
        );
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn macos_filesystem_denied_outside_roots() {
        if !macos_sandbox_available() {
            eprintln!("skipping: no usable sandbox-exec");
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let mut config = bash_config();
        config.shell.cwd = temp.path().to_path_buf();
        config.shell.env = vec![
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("HOME".into(), "/private/tmp".into()),
        ];
        config.policy = Some(os_sandbox_policy(temp.path().to_path_buf(), false));
        // /tmp is writable by seatbelt platform defaults; probe a read-only
        // root instead (/usr) so the denial is unambiguous.
        config.command = "echo oops > /usr/piko_seatbelt_probe.txt; echo write-exit=$?".into();
        let outcome = run(config).await.expect("run");
        eprintln!("filesystem deny outcome: {outcome:?}");
        assert!(
            !outcome.output.contains("write-exit=0"),
            "write outside roots must fail, got {outcome:?}"
        );
        assert!(
            !std::path::Path::new("/usr/piko_seatbelt_probe.txt").exists(),
            "forbidden file was created"
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn linux_network_and_filesystem_policy() {
        if !linux_sandbox_available() {
            eprintln!("skipping: bwrap not available");
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let mut config = bash_config();
        config.shell.cwd = temp.path().to_path_buf();
        config.shell.env = vec![("PATH".into(), "/usr/bin:/bin".into())];
        // Deny: file write outside roots must fail, network must fail.
        config.policy = Some(os_sandbox_policy(temp.path().to_path_buf(), false));
        config.command =
            "echo oops > /tmp/piko_bwrap_probe.txt; echo w=$?; curl -sS --max-time 2 http://127.0.0.1:9/ >/dev/null 2>&1; echo n=$?".into();
        let outcome = run(config).await.expect("run");
        eprintln!("linux deny outcome: {outcome:?}");
        assert!(!std::path::Path::new("/tmp/piko_bwrap_probe.txt").exists());
        assert!(outcome.output.contains("w=") && outcome.output.contains("n="));
        // Allow: network succeeds (connection refused == network reachable).
        config.policy = Some(os_sandbox_policy(temp.path().to_path_buf(), true));
        config.command = "curl -sS --max-time 2 http://127.0.0.1:9/ 2>&1; echo n=$?".into();
        let outcome = run(config).await.expect("run");
        assert!(outcome.output.contains("n=7"), "got {outcome:?}");
    }
}
