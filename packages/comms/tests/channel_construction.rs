use std::path::{Path, PathBuf};

const RAW_CONSTRUCTORS: &[&str] = &[
    "mpsc::channel(",
    "mpsc::unbounded_channel(",
    "unbounded_channel(",
    "oneshot::channel(",
    "watch::channel(",
    "broadcast::channel(",
    "std::sync::mpsc::channel(",
    "std::sync::mpsc::sync_channel(",
];

const ALLOWED_ADAPTERS: &[&str] = &[
    "packages/comms/src/wrappers.rs",
    "packages/comms/tests/channel_construction.rs",
    "packages/hostd/tests/support/mod.rs",
    "packages/hostd/tests/support/mock_session.rs",
    "packages/orchd/src/runtime/step/tests.rs",
];

#[test]
fn raw_channel_construction_is_confined_to_declared_adapters() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("comms crate must live under workspace/packages");
    let mut rust_files = Vec::new();
    collect_rust_files(&workspace.join("packages"), &mut rust_files);

    let mut violations = Vec::new();
    for path in rust_files {
        let relative = path
            .strip_prefix(workspace)
            .expect("collected file must be inside workspace")
            .to_string_lossy()
            .replace('\\', "/");
        if ALLOWED_ADAPTERS.contains(&relative.as_str()) {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("Rust source must be readable");
        for (line_index, line) in source.lines().enumerate() {
            if RAW_CONSTRUCTORS
                .iter()
                .any(|constructor| line.contains(constructor))
            {
                violations.push(format!("{relative}:{}: {line}", line_index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "raw channel construction bypasses piko-comms:\n{}",
        violations.join("\n")
    );
}

fn collect_rust_files(directory: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push(path);
        }
    }
}
