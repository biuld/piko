use std::path::Path;

use crate::app::command::TuiCommandEntry;
use crate::features::auto_completion::{CompletionRow, provider::AutoCompleteProvider};
use crate::ui::components::selectable_list::ColumnCell;

pub struct CommandPaletteProvider;

impl AutoCompleteProvider for CommandPaletteProvider {
    fn is_triggered(&self, text: &str, cursor: usize) -> bool {
        if cursor > text.len() || !text.is_char_boundary(cursor) {
            return false;
        }
        if text.starts_with('/') {
            let command_end = text.find(char::is_whitespace).unwrap_or(text.len());
            cursor <= command_end
        } else {
            false
        }
    }

    fn update(
        &mut self,
        _cwd: &Path,
        commands: &[TuiCommandEntry],
        text: &str,
        cursor: usize,
    ) -> Vec<CompletionRow> {
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
            .filter(|command| command.slash.starts_with(prefix))
            .map(|command| CompletionRow {
                replacement: format!("{} ", command.slash),
                start: 0,
                end,
                cells: vec![
                    ColumnCell::primary(command.slash.clone()),
                    ColumnCell::secondary(command.detail.clone()),
                ],
                keep_active: false,
            })
            .collect()
    }

    fn label(&self) -> &'static str {
        "command palette"
    }

    fn hints(&self) -> &'static str {
        "Tab cycle | Enter execute"
    }
}
