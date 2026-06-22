use crate::policy::Policy;
use std::io::Write;
use std::{
    path::Path,
    process::{Command, Stdio},
};
use tempfile::NamedTempFile;

pub fn exec(policy: &Policy, cwd: &Path, command: &str) -> Result<i32, Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    return exec_macos(policy, cwd, command);
    #[cfg(target_os = "linux")]
    return exec_linux(policy, cwd, command);
    #[allow(unreachable_code)]
    Err("piko-sandbox has no backend for this platform".into())
}

fn escaped(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[cfg(target_os = "macos")]
fn exec_macos(
    policy: &Policy,
    cwd: &Path,
    command: &str,
) -> Result<i32, Box<dyn std::error::Error>> {
    let cwd = cwd.canonicalize()?;
    let mut profile = String::from(
        "(version 1)\n(deny default)\n(allow process*)\n(allow sysctl-read)\n(allow mach-lookup)\n",
    );
    for root in &policy.read {
        profile.push_str(&format!(
            "(allow file-read* (subpath \"{}\"))\n",
            escaped(&if root.is_absolute() {
                root.clone()
            } else {
                cwd.join(root)
            })
        ));
    }
    for root in &policy.write {
        profile.push_str(&format!(
            "(allow file-write* (subpath \"{}\"))\n",
            escaped(&if root.is_absolute() {
                root.clone()
            } else {
                cwd.join(root)
            })
        ));
    }
    for root in &policy.deny {
        profile.push_str(&format!(
            "(deny file* (subpath \"{}\"))\n",
            escaped(&if root.is_absolute() {
                root.clone()
            } else {
                cwd.join(root)
            })
        ));
    }
    if policy.allow_network {
        profile.push_str("(allow network*)\n");
    }
    let mut file = NamedTempFile::new()?;
    file.write_all(profile.as_bytes())?;
    let status = Command::new("/usr/bin/sandbox-exec")
        .arg("-f")
        .arg(file.path())
        .arg("/bin/bash")
        .arg("-c")
        .arg(command)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    Ok(status.code().unwrap_or(126))
}

#[cfg(target_os = "linux")]
fn exec_linux(
    policy: &Policy,
    cwd: &Path,
    command: &str,
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
    let status = child
        .arg("--chdir")
        .arg(&cwd)
        .arg("/bin/bash")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    Ok(status.code().unwrap_or(126))
}
