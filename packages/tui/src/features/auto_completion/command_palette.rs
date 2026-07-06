use piko_protocol::CommandCatalogItem;
use std::path::Path;

use crate::features::auto_completion::{
    CellStyle, CompletionCell, CompletionRow, provider::AutoCompleteProvider,
};

pub struct CommandPaletteProvider;

impl AutoCompleteProvider for CommandPaletteProvider {
    fn trigger(&self) -> &str {
        "/"
    }

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
        commands: &[CommandCatalogItem],
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
            .filter(|command| command.slash_name.starts_with(prefix))
            .map(|command| CompletionRow {
                replacement: format!("{} ", command.slash_name),
                start: 0,
                end,
                cells: vec![
                    CompletionCell {
                        text: command.slash_name.clone(),
                        style: CellStyle::Accent,
                    },
                    CompletionCell {
                        text: command.detail.clone(),
                        style: CellStyle::Dim,
                    },
                ],
                keep_active: false,
            })
            .collect()
    }

    fn title(&self, selected: usize, total: usize) -> String {
        format!("command palette [{selected}/{total}] | Tab cycle | Enter execute")
    }
}
