use std::{fs, path::Path};

#[derive(Clone)]
pub struct Completion {
    pub label: String,
    pub detail: String,
    pub replacement: String,
    pub start: usize,
    pub end: usize,
}

const COMMANDS: &[(&str, &str)] = &[
    ("/help", "show keyboard shortcuts and slash commands"),
    ("/commands", "open command palette"),
    ("/new", "create a new session"),
    ("/sessions", "list and open sessions"),
    ("/tree", "inspect and navigate current session tree"),
    ("/fork", "fork current session, optional entry id"),
    ("/clone", "clone current session at current leaf"),
    ("/name", "rename current session"),
    ("/rename", "rename current session"),
    ("/import", "import a session JSONL file"),
    ("/delete", "delete current session; requires confirm"),
    ("/models", "list and set default model"),
    ("/settings", "open hostd-backed runtime settings"),
    ("/status", "show turn, queue, approval, and tool state"),
    ("/login", "start OAuth login, optional provider argument"),
    ("/logout", "remove credentials, optional provider argument"),
    ("/compact", "compact the current session"),
];

pub fn complete(cwd: &Path, text: &str, cursor: usize) -> Vec<Completion> {
    let mut items = command_completions(text, cursor);
    items.extend(file_completions(cwd, text, cursor));
    items.truncate(8);
    items
}

fn command_completions(text: &str, cursor: usize) -> Vec<Completion> {
    if !text.starts_with('/') {
        return Vec::new();
    }
    let end = text[..cursor]
        .find(char::is_whitespace)
        .unwrap_or(cursor)
        .min(cursor);
    let prefix = &text[..end];
    COMMANDS
        .iter()
        .filter(|(command, _)| command.starts_with(prefix))
        .map(|(command, detail)| Completion {
            label: (*command).to_string(),
            detail: (*detail).to_string(),
            replacement: (*command).to_string(),
            start: 0,
            end,
        })
        .collect()
}

fn file_completions(cwd: &Path, text: &str, cursor: usize) -> Vec<Completion> {
    let (start, token) = current_token(text, cursor);
    let Some(path_prefix) = token.strip_prefix('@') else {
        return Vec::new();
    };
    let path = Path::new(path_prefix);
    let dir_part = path.parent().unwrap_or_else(|| Path::new(""));
    let file_prefix = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let base_dir = cwd.join(dir_part);
    let Ok(entries) = fs::read_dir(base_dir) else {
        return Vec::new();
    };
    let mut completions = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(file_prefix) {
                return None;
            }
            let is_dir = entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false);
            let mut replacement_path = dir_part.join(&name).to_string_lossy().to_string();
            if is_dir {
                replacement_path.push('/');
            }
            Some(Completion {
                label: format!("@{replacement_path}"),
                detail: if is_dir { "directory" } else { "file" }.to_string(),
                replacement: format!("@{replacement_path}"),
                start,
                end: cursor,
            })
        })
        .collect::<Vec<_>>();
    completions.sort_by(|a, b| a.label.cmp(&b.label));
    completions
}

fn current_token(text: &str, cursor: usize) -> (usize, &str) {
    let before = &text[..cursor];
    let start = before
        .rfind(char::is_whitespace)
        .map(|index| index + 1)
        .unwrap_or(0);
    (start, &text[start..cursor])
}
