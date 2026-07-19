use std::fs;
use std::path::{Path, PathBuf};

use crate::app::command::TuiCommandEntry;
use crate::features::auto_completion::{
    CellStyle, CompletionCell, CompletionRow, provider::AutoCompleteProvider,
};

pub struct FileBrowserProvider;

impl AutoCompleteProvider for FileBrowserProvider {
    fn is_triggered(&self, text: &str, cursor: usize) -> bool {
        let (_, token) = current_token(text, cursor);
        token.starts_with('@')
    }

    fn update(
        &mut self,
        cwd: &Path,
        _commands: &[TuiCommandEntry],
        text: &str,
        cursor: usize,
    ) -> Vec<CompletionRow> {
        let (start, token) = current_token(text, cursor);
        let Some(path_prefix) = token.strip_prefix('@') else {
            return Vec::new();
        };

        if path_prefix.is_empty() {
            // Read top-level files & directories in cwd
            let Ok(entries) = fs::read_dir(cwd) else {
                return Vec::new();
            };
            let mut completions = entries
                .filter_map(Result::ok)
                .map(|entry| {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false);
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);

                    let replacement = if is_dir {
                        format!("@{name}/ ")
                    } else {
                        format!("@{name} ")
                    };

                    let detail = if is_dir {
                        "directory".to_string()
                    } else {
                        format!("file ({})", format_size(size))
                    };

                    CompletionRow {
                        replacement,
                        start,
                        end: cursor,
                        cells: vec![
                            CompletionCell {
                                text: if is_dir {
                                    format!("@{name}/")
                                } else {
                                    format!("@{name}")
                                },
                                style: CellStyle::Default,
                            },
                            CompletionCell {
                                text: detail,
                                style: CellStyle::Dim,
                            },
                        ],
                        keep_active: false,
                    }
                })
                .collect::<Vec<_>>();
            completions.sort_by(|a, b| a.cells[0].text.cmp(&b.cells[0].text));
            completions
        } else {
            // Recursive fuzzy search files
            let mut matched_files = Vec::new();
            recursive_search(cwd, cwd, path_prefix, &mut matched_files, 100, 0);

            let mut completions = matched_files
                .into_iter()
                .map(|(rel_path, size)| {
                    let rel_str = rel_path.to_string_lossy().to_string();
                    CompletionRow {
                        replacement: format!("@{rel_str} "),
                        start,
                        end: cursor,
                        cells: vec![
                            CompletionCell {
                                text: format!("@{rel_str}"),
                                style: CellStyle::Default,
                            },
                            CompletionCell {
                                text: format!("file ({})", format_size(size)),
                                style: CellStyle::Dim,
                            },
                        ],
                        keep_active: false,
                    }
                })
                .collect::<Vec<_>>();
            completions.sort_by(|a, b| a.cells[0].text.cmp(&b.cells[0].text));
            completions
        }
    }

    fn title(&self, selected: usize, total: usize) -> String {
        format!("file browser [{selected}/{total}] | Tab cycle | Enter accept")
    }
}

fn current_token(text: &str, cursor: usize) -> (usize, &str) {
    if cursor > text.len() || !text.is_char_boundary(cursor) {
        return (0, "");
    }
    let before = &text[..cursor];
    let start = before
        .rfind(char::is_whitespace)
        .map(|index| index + 1)
        .unwrap_or(0);
    (start, &text[start..cursor])
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn recursive_search(
    cwd: &Path,
    dir: &Path,
    query: &str,
    results: &mut Vec<(PathBuf, u64)>,
    max_results: usize,
    depth: usize,
) {
    if results.len() >= max_results || depth > 5 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            if name == ".git"
                || name == "node_modules"
                || name == "target"
                || name == "dist"
                || name == "build"
            {
                continue;
            }
            recursive_search(cwd, &path, query, results, max_results, depth + 1);
        } else {
            let rel_path = path.strip_prefix(cwd).unwrap_or(&path);
            let rel_str = rel_path.to_string_lossy().to_string();
            if fuzzy_match(&rel_str, query) {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                results.push((rel_path.to_path_buf(), size));
            }
        }
    }
}

fn fuzzy_match(text: &str, query: &str) -> bool {
    let text_lower = text.to_lowercase();
    let query_lower = query.to_lowercase();
    text_lower.contains(&query_lower)
}
