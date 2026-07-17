//! Layering guards for hostd DDD boundaries.

use std::fs;
use std::path::PathBuf;

fn rs_files_under(relative_dir: &str) -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative_dir);
    let mut files = Vec::new();
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }
    walk(&root, &mut files);
    files
}

fn domain_rs_files() -> Vec<PathBuf> {
    rs_files_under("src/domain")
}

#[test]
fn domain_must_not_depend_on_orchd_or_infra() {
    let mut violations = Vec::new();
    for path in domain_rs_files() {
        let source = fs::read_to_string(&path).expect("read domain rs");
        for (line_no, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if trimmed.contains("use orchd")
                || trimmed.contains("use orchd_api")
                || trimmed.contains("use crate::infra")
                || trimmed.contains("use crate::adapters")
            {
                violations.push(format!("{}:{}: {}", path.display(), line_no + 1, trimmed));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "domain layering violations:\n{}",
        violations.join("\n")
    );
}

/// `application` must reach storage/prompt-loading only through
/// `crate::ports` (implemented by `crate::adapters`), never by importing
/// `crate::infra` / `crate::adapters` directly.
///
/// `application::host_app` is the application-layer composition root (see
/// its module docs): it is allowed to *construct* the default filesystem
/// adapters via fully-qualified paths (`crate::adapters::storage::...`) so
/// `HostServer::new()` keeps working without a caller-supplied port, but it
/// must not `use` them. Unit-test fixtures (`#[cfg(test)]` modules, which
/// commonly need a concrete adapter to set up on-disk fixtures) are exempt;
/// scanning stops at the first `#[cfg(test)]` line in each file.
#[test]
fn application_must_not_depend_on_infra_or_adapters() {
    let mut violations = Vec::new();
    for path in rs_files_under("src/application") {
        let source = fs::read_to_string(&path).expect("read application rs");
        for (line_no, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("#[cfg(test)]") {
                break;
            }
            if trimmed.starts_with("//") {
                continue;
            }
            if trimmed.contains("use crate::infra") || trimmed.contains("use crate::adapters") {
                violations.push(format!("{}:{}: {}", path.display(), line_no + 1, trimmed));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "application layering violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn orch_turn_runner_lives_under_adapters() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let as_file = root.join("src/adapters/turns/orch_runner.rs");
    let as_dir = root.join("src/adapters/turns/orch_runner/mod.rs");
    assert!(
        as_file.exists() || as_dir.exists(),
        "OrchAgentRunRunner must live at adapters/turns/orch_runner.rs or orch_runner/mod.rs"
    );
    let legacy = root.join("src/domain/turns/orch_runner.rs");
    assert!(
        !legacy.exists(),
        "OrchAgentRunRunner must not remain under domain/turns"
    );
}

#[test]
fn turn_completion_never_synthesizes_execution_observation() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let run = fs::read_to_string(root.join("src/adapters/turns/orch_runner/run.rs"))
        .expect("read OrchAgentRunRunner run adapter");
    assert!(!run.contains("ExecutionChanged"));
    assert!(!run.contains("ExecutionObservationSnapshot"));
    assert!(!run.contains("let execution_id = turn_id"));
}
