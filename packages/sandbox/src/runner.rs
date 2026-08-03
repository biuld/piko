#[cfg(target_os = "macos")]
use crate::platform::SandboxCommand;
use crate::platform::build_sandbox_command;
use crate::policy::Policy;
use std::{
    path::Path,
    process::{Command, Stdio},
};

pub fn exec(
    policy: &Policy,
    cwd: &Path,
    command: &str,
    shell_path: Option<&str>,
) -> Result<i32, Box<dyn std::error::Error>> {
    let shell = shell_path.unwrap_or("bash");
    #[cfg(target_os = "macos")]
    return exec_macos(policy, cwd, command, shell);
    #[cfg(target_os = "linux")]
    return exec_linux(policy, cwd, command, shell);
    #[allow(unreachable_code)]
    Err("piko-sandbox has no backend for this platform".into())
}

#[cfg(target_os = "macos")]
fn exec_macos(
    policy: &Policy,
    cwd: &Path,
    command: &str,
    shell: &str,
) -> Result<i32, Box<dyn std::error::Error>> {
    let cwd = cwd.canonicalize()?;
    let shell_path = resolve_shell_path(shell);

    match build_sandbox_command(policy, &cwd)? {
        Some(wrapper) => run_wrapped(&cwd, &wrapper, &shell_path, command),
        // Nested-sandbox fallback: direct execution with the filesystem ACL
        // checks in policy.rs still applied at the tool layer.
        None => exec_direct(&cwd, command, &shell_path),
    }
}

/// Resolve shell path: if shell is just a name (e.g. "bash"), look it up;
/// if it's an absolute path already, use it directly.
fn resolve_shell_path(shell: &str) -> String {
    if shell.starts_with('/') {
        shell.to_string()
    } else {
        format!("/bin/{shell}")
    }
}

/// Run `<shell> -c <command>` inside the OS-sandbox wrapper.
#[cfg(target_os = "macos")]
fn run_wrapped(
    cwd: &Path,
    wrapper: &SandboxCommand,
    shell_path: &str,
    command: &str,
) -> Result<i32, Box<dyn std::error::Error>> {
    let mut cmd = Command::new(&wrapper.program);
    cmd.args(&wrapper.args)
        .arg(shell_path)
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    // Remove DYLD_* variables: they are stripped by sandbox-exec anyway but
    // removing them up-front avoids any cross-contamination.
    if wrapper.strip_loader_vars {
        for (key, _) in std::env::vars() {
            if key.starts_with("DYLD_") {
                cmd.env_remove(&key);
            }
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
            return exec_direct(cwd, command, shell_path);
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
    let shell_path = resolve_shell_path(shell);
    let wrapper = build_sandbox_command(policy, &cwd)?
        .ok_or("piko-sandbox: no Linux sandbox wrapper available")?;
    let status = Command::new(&wrapper.program)
        .args(&wrapper.args)
        .arg(&shell_path)
        .arg("-c")
        .arg(command)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    Ok(status.code().unwrap_or(126))
}
