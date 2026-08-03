// ---- Platform OS-sandbox wrapper builders ----
//
// Shared by the blocking runner (`runner.rs`) and the async PTY runner
// (`exec.rs`) so the seatbelt/bwrap command line can never drift between the
// two execution paths.

use std::path::Path;

use crate::policy::{Policy, PolicyError};

/// The OS-sandbox wrapper command around `<shell> -c <command>`.
#[derive(Debug, Clone)]
pub struct SandboxCommand {
    /// Absolute (or PATH-resolved) wrapper program.
    pub program: String,
    /// Arguments before the trailing `--`; the caller appends the shell and
    /// the command after it.
    pub args: Vec<String>,
    /// Whether loader-injection variables (`DYLD_*`/`LD_*`) must be stripped
    /// from the environment before spawning.
    pub strip_loader_vars: bool,
}

#[cfg(target_os = "macos")]
const SEATBELT_BASE_POLICY: &str = include_str!("../resources/macos/seatbelt_base_policy.sbpl");
#[cfg(target_os = "macos")]
const PLATFORM_DEFAULTS_POLICY: &str = include_str!("../resources/macos/platform_defaults.sbpl");

/// Returns true when the current process is already running inside an Apple
/// App Sandbox (e.g. launched from an Xcode task runner or a sandboxed IDE
/// helper). In that case `/usr/bin/sandbox-exec` cannot re-initialise the
/// kernel sandbox and will SIGABRT, so callers skip the seatbelt wrapper.
#[cfg(target_os = "macos")]
fn is_app_sandboxed() -> bool {
    std::env::var("APP_SANDBOX_CONTAINER_ID").is_ok()
}

/// Build the platform OS-sandbox wrapper for `policy`.
///
/// Returns `Ok(None)` when the platform cannot nest another OS sandbox (for
/// example we are already inside an Apple App Sandbox) — callers then
/// execute directly; the filesystem ACL checks in `policy.rs` still apply.
pub fn build_sandbox_command(
    policy: &Policy,
    cwd: &Path,
) -> Result<Option<SandboxCommand>, PolicyError> {
    #[cfg(target_os = "macos")]
    {
        return macos_build(policy, cwd);
    }
    #[cfg(target_os = "linux")]
    {
        return linux_build(policy, cwd);
    }
    #[allow(unreachable_code)]
    Err(PolicyError::Invalid(
        "piko-sandbox has no OS-sandbox backend for this platform".into(),
    ))
}

/// Resolve a policy root against `cwd` and canonicalize it when possible.
fn resolve_root(root: &std::path::PathBuf, cwd: &Path) -> std::path::PathBuf {
    let p = if root.is_absolute() {
        root.clone()
    } else {
        cwd.join(root)
    };
    p.canonicalize().unwrap_or(p)
}

#[cfg(target_os = "macos")]
fn macos_build(policy: &Policy, cwd: &Path) -> Result<Option<SandboxCommand>, PolicyError> {
    if is_app_sandboxed() {
        return Ok(None);
    }
    let cwd = cwd.canonicalize()?;

    // Paths are passed as -Dkey=path parameters and referenced in the
    // policy via (param "key"), which avoids embedding raw paths in the
    // policy string.
    let mut policy_parts = vec![
        SEATBELT_BASE_POLICY.to_string(),
        PLATFORM_DEFAULTS_POLICY.to_string(),
    ];
    let mut dir_params: Vec<(String, String)> = Vec::new();

    for (index, root) in policy.read.iter().enumerate() {
        let p = resolve_root(root, &cwd);
        let key = format!("READABLE_ROOT_{index}");
        dir_params.push((key.clone(), p.display().to_string()));
        policy_parts.push(format!(
            "; allow read-only file operations\n(allow file-read* (subpath (param \"{key}\")))\n(allow file-map-executable (subpath (param \"{key}\")))\n",
        ));
    }
    for (index, root) in policy.write.iter().enumerate() {
        let p = resolve_root(root, &cwd);
        let key = format!("WRITABLE_ROOT_{index}");
        dir_params.push((key.clone(), p.display().to_string()));
        policy_parts.push(format!("(allow file-write* (subpath (param \"{key}\")))\n",));
    }
    for (index, root) in policy.deny.iter().enumerate() {
        let p = resolve_root(root, &cwd);
        let key = format!("DENY_ROOT_{index}");
        dir_params.push((key.clone(), p.display().to_string()));
        policy_parts.push(format!("(deny file* (subpath (param \"{key}\")))\n",));
    }

    if policy.allow_network {
        policy_parts.push("(allow network-outbound)\n(allow network-inbound)\n".to_string());
    }

    let full_policy = policy_parts.join("\n");
    let mut args = vec!["-p".to_string(), full_policy];
    for (key, value) in dir_params {
        args.push(format!("-D{key}={value}"));
    }
    args.push("--".to_string());

    Ok(Some(SandboxCommand {
        program: "/usr/bin/sandbox-exec".into(),
        args,
        strip_loader_vars: true,
    }))
}

#[cfg(target_os = "linux")]
fn linux_build(policy: &Policy, cwd: &Path) -> Result<Option<SandboxCommand>, PolicyError> {
    let cwd = cwd.canonicalize()?;
    let mut args = vec![
        "--die-with-parent".to_string(),
        "--unshare-all".to_string(),
        "--new-session".to_string(),
        "--proc".to_string(),
        "/proc".to_string(),
        "--dev".to_string(),
        "/dev".to_string(),
        "--tmpfs".to_string(),
        "/tmp".to_string(),
        "--ro-bind".to_string(),
        "/usr".to_string(),
        "/usr".to_string(),
        "--ro-bind".to_string(),
        "/bin".to_string(),
        "/bin".to_string(),
    ];
    for root in &policy.read {
        let p = resolve_root(root, &cwd);
        args.push("--ro-bind".into());
        args.push(p.display().to_string());
        args.push(p.display().to_string());
    }
    for root in &policy.write {
        let p = resolve_root(root, &cwd);
        args.push("--bind".into());
        args.push(p.display().to_string());
        args.push(p.display().to_string());
    }
    for root in &policy.deny {
        let p = resolve_root(root, &cwd);
        if p.is_dir() {
            args.push("--tmpfs".into());
            args.push(p.display().to_string());
        } else if p.exists() {
            args.push("--ro-bind".into());
            args.push("/dev/null".into());
            args.push(p.display().to_string());
        }
    }
    if policy.allow_network {
        // Must follow --unshare-all: the later flag wins for the network
        // namespace, joining the host namespace instead of an empty one.
        args.push("--share-net".into());
    }
    args.push("--chdir".into());
    args.push(cwd.display().to_string());
    args.push("--".to_string());

    Ok(Some(SandboxCommand {
        program: "bwrap".into(),
        args,
        strip_loader_vars: false,
    }))
}
