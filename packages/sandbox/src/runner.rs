use crate::policy::Policy;
use std::{
    path::Path,
    process::{Command, Stdio},
};

#[cfg(target_os = "macos")]
const SEATBELT_BASE_POLICY: &str = include_str!("../resources/macos/seatbelt_base_policy.sbpl");
#[cfg(target_os = "macos")]
const PLATFORM_DEFAULTS_POLICY: &str = include_str!("../resources/macos/platform_defaults.sbpl");

pub fn exec(policy: &Policy, cwd: &Path, command: &str, shell_path: Option<&str>) -> Result<i32, Box<dyn std::error::Error>> {
    let shell = shell_path.unwrap_or("bash");
    #[cfg(target_os = "macos")]
    return exec_macos(policy, cwd, command, shell);
    #[cfg(target_os = "linux")]
    return exec_linux(policy, cwd, command, shell);
    #[allow(unreachable_code)]
    Err("piko-sandbox has no backend for this platform".into())
}

/// Returns true when the current process is already running inside an Apple App
/// Sandbox (e.g. launched from an Xcode task runner or a sandboxed IDE helper).
/// In that case /usr/bin/sandbox-exec cannot re-initialise the kernel sandbox
/// and will SIGABRT. We detect this early so callers can skip the seatbelt
/// wrapper and fall back to the plain-exec path.
#[cfg(target_os = "macos")]
fn is_app_sandboxed() -> bool {
    std::env::var("APP_SANDBOX_CONTAINER_ID").is_ok()
}

#[cfg(target_os = "macos")]
fn exec_macos(
    policy: &Policy,
    cwd: &Path,
    command: &str,
    shell: &str,
) -> Result<i32, Box<dyn std::error::Error>> {
    let cwd = cwd.canonicalize()?;

    // ------------------------------------------------------------------
    // Nested-sandbox detection: when we are already inside an App Sandbox
    // (e.g. Xcode task runner, sandboxed IDE helper), sandbox-exec cannot
    // re-initialise the seatbelt kernel extension and will SIGABRT with
    // signal 6.  Fall back to direct execution — the filesystem ACL checks
    // in policy.rs still apply via the Check/CheckPath subcommands, so the
    // security boundary is not completely removed.
    // ------------------------------------------------------------------
    if is_app_sandboxed() {
        return exec_direct(&cwd, command, shell);
    }

    // ------------------------------------------------------------------
    // Build the seatbelt policy.
    //
    // Paths are passed as -Dkey=path parameters and referenced in the
    // policy via (param "key"), which avoids embedding raw paths in the
    // policy string and is the same approach used by Codex.
    // ------------------------------------------------------------------
    let mut policy_parts = vec![
        SEATBELT_BASE_POLICY.to_string(),
        PLATFORM_DEFAULTS_POLICY.to_string(),
    ];

    let mut dir_params: Vec<(String, String)> = Vec::new();

    // Readable roots
    for (index, root) in policy.read.iter().enumerate() {
        let p = if root.is_absolute() {
            root.clone()
        } else {
            cwd.join(root)
        };
        let p = p.canonicalize().unwrap_or(p);
        let key = format!("READABLE_ROOT_{index}");
        dir_params.push((key.clone(), p.display().to_string()));
        policy_parts.push(format!(
            "; allow read-only file operations\n(allow file-read* (subpath (param \"{key}\")))\n(allow file-map-executable (subpath (param \"{key}\")))\n",
        ));
    }

    // Writable roots
    for (index, root) in policy.write.iter().enumerate() {
        let p = if root.is_absolute() {
            root.clone()
        } else {
            cwd.join(root)
        };
        let p = p.canonicalize().unwrap_or(p);
        let key = format!("WRITABLE_ROOT_{index}");
        dir_params.push((key.clone(), p.display().to_string()));
        policy_parts.push(format!("(allow file-write* (subpath (param \"{key}\")))\n",));
    }

    // Deny rules (override above allowances)
    for (index, root) in policy.deny.iter().enumerate() {
        let p = if root.is_absolute() {
            root.clone()
        } else {
            cwd.join(root)
        };
        let p = p.canonicalize().unwrap_or(p);
        let key = format!("DENY_ROOT_{index}");
        dir_params.push((key.clone(), p.display().to_string()));
        policy_parts.push(format!("(deny file* (subpath (param \"{key}\")))\n",));
    }

    if policy.allow_network {
        policy_parts.push("(allow network-outbound)\n(allow network-inbound)\n".to_string());
    }

    let full_policy = policy_parts.join("\n");

    // ------------------------------------------------------------------
    // Assemble the sandbox-exec command.
    // Only use /usr/bin/sandbox-exec (absolute path) to guard against
    // PATH injection of a malicious sandbox-exec.
    // ------------------------------------------------------------------
    let mut cmd = Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-p").arg(&full_policy);

    for (key, value) in &dir_params {
        cmd.arg(format!("-D{key}={value}"));
    }

    // Resolve shell path: if shell is just a name (e.g. "bash"), look it up;
    // if it's an absolute path already, use it directly.
    let shell_path = if shell.starts_with('/') {
        shell.to_string()
    } else {
        format!("/bin/{shell}")
    };
    cmd.arg("--")
        .arg(&shell_path)
        .arg("-c")
        .arg(command)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    // Remove DYLD_* variables: they are stripped by sandbox-exec anyway
    // but removing them up-front avoids any cross-contamination.
    for (key, _) in std::env::vars() {
        if key.starts_with("DYLD_") {
            cmd.env_remove(&key);
        }
    }

    use std::os::unix::process::ExitStatusExt;

    let status = cmd.status()?;
    if !status.success() {
        if status.signal() == Some(6) {
            // SIGABRT: we missed a nested-sandbox scenario not covered by the
            // APP_SANDBOX_CONTAINER_ID heuristic (e.g. a third-party sandbox
            // wrapper). Fall back to direct execution.
            eprintln!(
                "piko-sandbox: sandbox-exec SIGABRT detected (likely nested sandbox). \
                 Falling back to direct execution."
            );
            return exec_direct(&cwd, command, shell);
        }
        if let Some(code) = status.code() {
            return Err(format!("sandbox-exec command failed with exit code {code}").into());
        }
        return Err(format!("sandbox-exec command failed with status: {status:?}").into());
    }

    Ok(0)
}

/// Execute the command directly without sandbox-exec. Used as fallback when
/// we detect a nested sandbox environment where sandbox-exec cannot operate.
/// The filesystem ACL checks performed by the Check/CheckPath subcommands
/// still provide a policy boundary before we reach this path.
#[cfg(target_os = "macos")]
fn exec_direct(cwd: &Path, command: &str, shell: &str) -> Result<i32, Box<dyn std::error::Error>> {
    use std::os::unix::process::ExitStatusExt;
    let shell_path = if shell.starts_with('/') {
        shell.to_string()
    } else {
        format!("/bin/{shell}")
    };
    let status = Command::new(&shell_path)
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    if !status.success() {
        if let Some(code) = status.code() {
            return Err(format!("command failed with exit code {code}").into());
        }
        if let Some(sig) = status.signal() {
            return Err(format!("command terminated by signal {sig}").into());
        }
    }
    Ok(status.code().unwrap_or(0))
}

#[cfg(target_os = "linux")]
fn exec_linux(
    policy: &Policy,
    cwd: &Path,
    command: &str,
    shell: &str,
) -> Result<i32, Box<dyn std::error::Error>> {
    let cwd = cwd.canonicalize()?;
    if policy.allow_network {
        return Err("Linux bwrap backend does not permit network in v1".into());
    }
    let mut child = Command::new("bwrap");
    child.args([
        "--die-with-parent",
        "--unshare-all",
        "--new-session",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--ro-bind",
        "/usr",
        "/usr",
        "--ro-bind",
        "/bin",
        "/bin",
    ]);
    for root in &policy.read {
        let p = if root.is_absolute() {
            root.clone()
        } else {
            cwd.join(root)
        };
        child.arg("--ro-bind").arg(&p).arg(&p);
    }
    for root in &policy.write {
        let p = if root.is_absolute() {
            root.clone()
        } else {
            cwd.join(root)
        };
        child.arg("--bind").arg(&p).arg(&p);
    }
    for root in &policy.deny {
        let p = if root.is_absolute() {
            root.clone()
        } else {
            cwd.join(root)
        };
        if p.is_dir() {
            child.arg("--tmpfs").arg(&p);
        } else if p.exists() {
            child.arg("--ro-bind").arg("/dev/null").arg(&p);
        }
    }
    let shell_path = if shell.starts_with('/') {
        shell.to_string()
    } else {
        format!("/bin/{shell}")
    };
    let status = child
        .arg("--chdir")
        .arg(&cwd)
        .arg(&shell_path)
        .arg("-c")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    Ok(status.code().unwrap_or(126))
}
