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

/// Scan a source file for any occurrence of the given crate-path tokens,
/// not just `use` statements: fully-qualified expressions
/// (`piko_orchd_api::stable_internal_id(...)`, `crate::infra::...`) are
/// equally real layering violations.
fn find_path_tokens<'a>(
    source: &str,
    path: &std::path::Path,
    tokens: &'a [&'a str],
) -> Vec<String> {
    let mut violations = Vec::new();
    for (line_no, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        for token in tokens {
            if trimmed.contains(token) {
                violations.push(format!("{}:{}: {}", path.display(), line_no + 1, trimmed));
                break;
            }
        }
    }
    violations
}

#[test]
fn domain_must_not_depend_on_orchd_or_infra() {
    let tokens = [
        "piko_orchd::",
        "piko_orchd_api::",
        "crate::infra::",
        "crate::adapters::",
        "crate::ports::",
    ];
    let mut violations = Vec::new();
    for path in domain_rs_files() {
        let source = fs::read_to_string(&path).expect("read domain rs");
        violations.extend(find_path_tokens(&source, &path, &tokens));
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
    let tokens = ["crate::infra::", "crate::adapters::"];
    // `application::host_app` is the sanctioned composition root (see its
    // module docs): it constructs the default filesystem adapters via
    // fully-qualified paths so `HostServer::new()` keeps working. Files
    // listed here are also loaded as `#[cfg(test)]` modules from a parent
    // file (`#[path = "..."]`), so their `#[cfg(test)]` marker is not
    // inline; they are unit-test fixtures and fully exempt.
    let exempt_files = ["host_app.rs", "lifecycle_live_tests.rs"];
    let mut violations = Vec::new();
    for path in rs_files_under("src/application") {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if exempt_files.contains(&file_name) {
            continue;
        }
        let source = fs::read_to_string(&path).expect("read application rs");
        // Unit-test fixtures (`#[cfg(test)]` modules) are exempt; scanning
        // stops at the first `#[cfg(test)]` line in each file.
        let body = source
            .split("#[cfg(test)]")
            .next()
            .expect("split always yields a first part");
        violations.extend(find_path_tokens(body, &path, &tokens));
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
    let as_file = root.join("src/adapters/agent_runner/orch_runner.rs");
    let as_dir = root.join("src/adapters/agent_runner/orch_runner/mod.rs");
    assert!(
        as_file.exists() || as_dir.exists(),
        "OrchAgentRunRunner must live at adapters/agent_runner/orch_runner.rs or orch_runner/mod.rs"
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
    let run = fs::read_to_string(root.join("src/adapters/agent_runner/orch_runner/run.rs"))
        .expect("read OrchAgentRunRunner run adapter");
    assert!(!run.contains("ExecutionChanged"));
    assert!(!run.contains("ExecutionObservationSnapshot"));
    assert!(!run.contains("let execution_id = turn_id"));
}

#[test]
fn production_session_storage_has_no_v3_or_mutable_manifest_path() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for relative in [
        "src/infra/storage/jsonl_io.rs",
        "src/infra/storage/session_store/manifest.rs",
        "src/infra/storage/session_store/io.rs",
        "src/infra/storage/session_store/create.rs",
        "src/infra/storage/session_store/commit",
    ] {
        assert!(
            !root.join(relative).exists(),
            "legacy storage remains: {relative}"
        );
    }
    let mut pending = vec![root.join("src/infra/storage/session_store")];
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
                let source = std::fs::read_to_string(&path).unwrap();
                assert!(!source.contains("update_manifest"));
                assert!(!source.contains("load_manifest"));
                assert!(!source.contains("AgentShardHeader"));
            }
        }
    }
}
