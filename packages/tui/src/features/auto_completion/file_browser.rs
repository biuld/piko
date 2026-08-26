use std::fs;
use std::path::{Path, PathBuf};

use crate::app::command::TuiCommandEntry;
use crate::features::auto_completion::{CompletionRow, provider::AutoCompleteProvider};
use crate::ui::components::selectable_list::ColumnCell;
use crate::ui::interaction_hints::InteractionHints;

pub struct FileBrowserProvider;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileQuery {
    pub start: usize,
    pub end: usize,
    pub value: String,
}

impl AutoCompleteProvider for FileBrowserProvider {
    fn is_triggered(&self, text: &str, cursor: usize) -> bool {
        query(text, cursor).is_some()
    }

    fn update(
        &mut self,
        _cwd: &Path,
        _commands: &[TuiCommandEntry],
        _text: &str,
        _cursor: usize,
    ) -> Vec<CompletionRow> {
        Vec::new()
    }

    fn label(&self) -> &'static str {
        "file browser"
    }

    fn hints(&self) -> InteractionHints<'static> {
        InteractionHints::new("Tab cycle | Enter accept")
    }
}

pub fn query(text: &str, cursor: usize) -> Option<FileQuery> {
    let (start, token) = current_token(text, cursor);
    let value = token.strip_prefix('@')?;
    Some(FileQuery {
        start,
        end: cursor,
        value: value.to_string(),
    })
}

pub fn search(cwd: &Path, query: &FileQuery) -> Vec<CompletionRow> {
    let start = query.start;
    let cursor = query.end;
    let path_prefix = query.value.as_str();

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
                        ColumnCell::primary(if is_dir {
                            format!("@{name}/")
                        } else {
                            format!("@{name}")
                        }),
                        ColumnCell::secondary(detail),
                    ],
                    keep_active: false,
                    submit_on_accept: false,
                }
            })
            .collect::<Vec<_>>();
        completions.sort_by(|a, b| a.cells[0].text.cmp(&b.cells[0].text));
        completions
    } else {
        let mut matched_files = Vec::new();
        recursive_search(cwd, cwd, path_prefix, &mut matched_files);

        let mut completions = matched_files
            .into_iter()
            .map(|(score, rel_path, size)| {
                let rel_str = rel_path.to_string_lossy().to_string();
                (
                    score,
                    CompletionRow {
                        replacement: format!("@{rel_str} "),
                        start,
                        end: cursor,
                        cells: vec![
                            ColumnCell::primary(format!("@{rel_str}")),
                            ColumnCell::secondary(format!("file ({})", format_size(size))),
                        ],
                        keep_active: false,
                        submit_on_accept: false,
                    },
                )
            })
            .collect::<Vec<_>>();
        completions.sort_by(|(score_a, a), (score_b, b)| {
            score_b
                .cmp(score_a)
                .then_with(|| a.cells[0].text.cmp(&b.cells[0].text))
        });
        completions
            .into_iter()
            .take(100)
            .map(|(_, row)| row)
            .collect()
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

fn recursive_search(cwd: &Path, dir: &Path, query: &str, results: &mut Vec<(i64, PathBuf, u64)>) {
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
            recursive_search(cwd, &path, query, results);
        } else {
            let rel_path = path.strip_prefix(cwd).unwrap_or(&path);
            let rel_str = rel_path.to_string_lossy().to_string();
            if let Some(score) = fuzzy_score(&rel_str, query) {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                results.push((score, rel_path.to_path_buf(), size));
            }
        }
    }
}

fn fuzzy_score(text: &str, query: &str) -> Option<i64> {
    let text_lower = text.to_lowercase();
    let query_lower = query.to_lowercase();
    if query_lower.is_empty() {
        return Some(0);
    }
    if let Some(index) = text_lower.find(&query_lower) {
        return Some(10_000 - index as i64 * 10 - text_lower.len() as i64);
    }
    let mut score = 0i64;
    let mut search_from = 0usize;
    let mut previous = None;
    for needle in query_lower.chars() {
        let relative = text_lower[search_from..].find(needle)?;
        let index = search_from + relative;
        score += 100;
        if previous.is_some_and(|previous| previous + 1 == index) {
            score += 40;
        }
        score -= index as i64;
        previous = Some(index);
        search_from = index + needle.len_utf8();
    }
    Some(score - text_lower.len() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_score_supports_non_contiguous_matches_and_prefers_substrings() {
        let substring = fuzzy_score("packages/tui/src/main.rs", "tui").unwrap();
        let subsequence = fuzzy_score("tests/ui/main.rs", "tui").unwrap();
        assert!(substring > subsequence);
        assert!(fuzzy_score("src/main.rs", "xyz").is_none());
    }
}
