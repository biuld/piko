//! Layering guards for hostd DDD boundaries.

use std::fs;
use std::path::PathBuf;

fn domain_rs_files() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/domain");
    let mut files = Vec::new();
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("read domain dir") {
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

#[test]
fn orch_turn_runner_lives_under_adapters() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let as_file = root.join("src/adapters/turns/orch_runner.rs");
    let as_dir = root.join("src/adapters/turns/orch_runner/mod.rs");
    assert!(
        as_file.exists() || as_dir.exists(),
        "OrchTurnRunner must live at adapters/turns/orch_runner.rs or orch_runner/mod.rs"
    );
    let legacy = root.join("src/domain/turns/orch_runner.rs");
    assert!(
        !legacy.exists(),
        "OrchTurnRunner must not remain under domain/turns"
    );
}

#[test]
fn turn_completion_never_synthesizes_execution_observation() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let run = fs::read_to_string(root.join("src/adapters/turns/orch_runner/run.rs"))
        .expect("read OrchTurnRunner run adapter");
    assert!(!run.contains("ExecutionChanged"));
    assert!(!run.contains("ExecutionObservationSnapshot"));
    assert!(!run.contains("let execution_id = turn_id"));
}
