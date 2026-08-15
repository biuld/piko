use std::path::{Path, PathBuf};

const RUST_FILE_LINE_CEILING: usize = 500;

#[test]
fn rust_sources_respect_file_size_ceiling() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rust_files(&root, &mut files);

    let oversized: Vec<String> = files
        .into_iter()
        .filter_map(|path| {
            let source = std::fs::read_to_string(&path).expect("read Rust source");
            let lines = source.lines().count();
            (lines > RUST_FILE_LINE_CEILING).then(|| {
                format!(
                    "{}: {lines} lines",
                    path.strip_prefix(&root).unwrap_or(&path).to_string_lossy()
                )
            })
        })
        .collect();

    assert!(
        oversized.is_empty(),
        "Rust files exceed the {RUST_FILE_LINE_CEILING}-line ceiling:\n{}",
        oversized.join("\n")
    );
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read source directory") {
        let path = entry.expect("read directory entry").path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
