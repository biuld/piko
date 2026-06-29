use std::{fs, path::Path};

use piko_protocol::CommandCatalogItem;

#[derive(Clone)]
pub struct Completion {
    pub label: String,
    pub detail: String,
    pub replacement: String,
    pub start: usize,
    pub end: usize,
}

pub struct CompletionResult {
    pub active: bool,
    pub items: Vec<Completion>,
}

pub fn complete(
    cwd: &Path,
    commands: &[CommandCatalogItem],
    text: &str,
    cursor: usize,
) -> CompletionResult {
    let trigger = trigger(text, cursor);
    let mut items = match trigger {
        Some(CompletionTrigger::SlashCommand) => command_completions(commands, text, cursor),
        Some(CompletionTrigger::FilePath) => file_completions(cwd, text, cursor),
        None => Vec::new(),
    };
    items.truncate(8);
    CompletionResult {
        active: trigger.is_some(),
        items,
    }
}

#[derive(Clone, Copy)]
enum CompletionTrigger {
    SlashCommand,
    FilePath,
}

fn trigger(text: &str, cursor: usize) -> Option<CompletionTrigger> {
    if cursor > text.len() || !text.is_char_boundary(cursor) {
        return None;
    }
    if text.starts_with('/') {
        let command_end = text.find(char::is_whitespace).unwrap_or(text.len());
        if cursor <= command_end {
            return Some(CompletionTrigger::SlashCommand);
        }
    }
    let (_, token) = current_token(text, cursor);
    token
        .starts_with('@')
        .then_some(CompletionTrigger::FilePath)
}

fn command_completions(
    commands: &[CommandCatalogItem],
    text: &str,
    cursor: usize,
) -> Vec<Completion> {
    if !text.starts_with('/') {
        return Vec::new();
    }
    let end = text[..cursor]
        .find(char::is_whitespace)
        .unwrap_or(cursor)
        .min(cursor);
    let prefix = &text[..end];
    commands
        .iter()
        .flat_map(|command| {
            command
                .slash_names
                .iter()
                .filter(move |name| name.starts_with(prefix))
                .map(move |name| Completion {
                    label: name.clone(),
                    detail: command.detail.clone(),
                    replacement: name.clone(),
                    start: 0,
                    end,
                })
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

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::CommandCatalogAction;

    fn commands() -> Vec<CommandCatalogItem> {
        vec![CommandCatalogItem {
            id: "help".to_string(),
            title: "Help".to_string(),
            detail: "show help".to_string(),
            action: CommandCatalogAction::Help,
            slash_names: vec!["/help".to_string(), "/?".to_string()],
            visible_in_palette: true,
        }]
    }

    #[test]
    fn slash_trigger_stays_active_with_no_matches() {
        let result = complete(Path::new("."), &commands(), "/zzz", 4);
        assert!(result.active);
        assert!(result.items.is_empty());
    }

    #[test]
    fn slash_completion_uses_command_token_range() {
        let result = complete(Path::new("."), &commands(), "/he", 3);
        assert!(result.active);
        let help = result
            .items
            .iter()
            .find(|item| item.label == "/help")
            .unwrap();
        assert_eq!(help.start, 0);
        assert_eq!(help.end, 3);
        assert_eq!(help.replacement, "/help");
    }

    #[test]
    fn slash_trigger_inactive_in_arguments() {
        let result = complete(Path::new("."), &commands(), "/help now", 6);
        assert!(!result.active);
    }
}
