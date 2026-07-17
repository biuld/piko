use piko_protocol::CommandCatalogItem;
use std::path::Path;

use crate::features::auto_completion::CompletionRow;

pub trait AutoCompleteProvider {
    /// Checks if this provider is triggered by the current token.
    fn is_triggered(&self, text: &str, cursor: usize) -> bool;

    /// Fetches and filters completion items.
    fn update(
        &mut self,
        cwd: &Path,
        commands: &[CommandCatalogItem],
        text: &str,
        cursor: usize,
    ) -> Vec<CompletionRow>;

    /// Title displayed in the Suggestions block header.
    fn title(&self, selected: usize, total: usize) -> String;
}
