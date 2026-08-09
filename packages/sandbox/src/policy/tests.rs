use std::{fs, path::Path};

use super::*;
use tempfile::tempdir;

fn policy() -> EffectivePermissions {
    EffectivePermissions {
        version: 1,
        read_roots: vec![PathBuf::from(".")],
        write_roots: vec![PathBuf::from(".")],
        scratch_roots: vec![],
        denied_read_roots: vec![PathBuf::from(".git")],
        denied_write_roots: vec![],
        network: false.into(),
    }
}

#[test]
fn authorizes_existing_reads_and_missing_writes_in_workspace() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("input.txt"), "value").unwrap();
    assert!(
        policy()
            .authorize(dir.path(), Path::new("input.txt"), Access::Read, true)
            .is_ok()
    );
    assert!(
        policy()
            .authorize(
                dir.path(),
                Path::new("new/dir/output.txt"),
                Access::Write,
                false
            )
            .is_ok()
    );
}

#[test]
fn deny_rule_overrides_workspace_root() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    fs::write(dir.path().join(".git/config"), "secret").unwrap();
    assert!(matches!(
        policy().authorize(dir.path(), Path::new(".git/config"), Access::Read, true),
        Err(PolicyError::Denied(_))
    ));
}

#[test]
fn verify_resolved_accepts_stable_paths_and_detects_swaps() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("target.txt"), "one").unwrap();
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("outside.txt"), "secret").unwrap();

    let cwd = dir.path();
    let policy = policy();
    let resolved = policy
        .authorize(cwd, Path::new("target.txt"), Access::Write, true)
        .expect("in-roots file authorizes");

    // A stable path verifies against its original resolution.
    policy
        .verify_resolved(cwd, Path::new("target.txt"), Access::Write, true, &resolved)
        .expect("unchanged path verifies");

    // Swap the target for a symlink pointing outside the roots: the
    // re-resolution either maps elsewhere or fails authorization, and in
    // both cases the write must not proceed.
    #[cfg(unix)]
    {
        std::fs::remove_file(dir.path().join("target.txt")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("outside.txt"),
            dir.path().join("target.txt"),
        )
        .unwrap();
        assert!(
            policy
                .verify_resolved(cwd, Path::new("target.txt"), Access::Write, true, &resolved)
                .is_err(),
            "swapped path must fail verification"
        );
    }
}

#[cfg(unix)]
#[test]
fn rejects_symlink_escape() {
    use std::os::unix::fs::symlink;
    let workspace = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("secret"), "secret").unwrap();
    symlink(outside.path().join("secret"), workspace.path().join("link")).unwrap();
    assert!(matches!(
        policy().authorize(workspace.path(), Path::new("link"), Access::Read, true),
        Err(PolicyError::Denied(_))
    ));
}
