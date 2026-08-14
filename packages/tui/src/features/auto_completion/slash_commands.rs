use crate::app::command::TuiCommandEntry;
use crate::features::auto_completion::{CompletionRow, provider::AutoCompleteProvider};
use crate::ui::components::selectable_list::ColumnCell;
use crate::ui::interaction_hints::InteractionHints;
use piko_protocol::HostCommandInvoke;
use std::path::Path;
pub struct SlashCommandProvider;
impl AutoCompleteProvider for SlashCommandProvider {
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
        let prefix = text[..end].to_lowercase();
        commands
            .iter()
            .filter(|command| {
                command.slash.to_lowercase().starts_with(&prefix)
                    || command
                        .title
                        .to_lowercase()
                        .contains(prefix.trim_start_matches('/'))
                    || command
                        .detail
                        .to_lowercase()
                        .contains(prefix.trim_start_matches('/'))
            })
            .map(|command| CompletionRow {
                replacement: format!("{} ", command.slash),
                start: 0,
                end,
                cells: vec![
                    ColumnCell::primary(command.slash.clone()),
                    ColumnCell::secondary(command.detail.clone()),
                ],
                keep_active: false,
                submit_on_accept: matches!(command.invoke, HostCommandInvoke::Immediate),
            })
            .collect()
    }
    fn label(&self) -> &'static str {
        "slash commands"
    }
    fn hints(&self) -> InteractionHints<'static> {
        InteractionHints::new("Tab cycle | Enter execute")
    }
}
