use std::path::Path;

use crate::app::command::TuiCommandEntry;
use crate::features::auto_completion::CompletionRow;

pub trait AutoCompleteProvider {
    /// Checks if this provider is triggered by the current token.
    fn is_triggered(&self, text: &str, cursor: usize) -> bool;

    /// Fetches and filters completion items.
    fn update(
        &mut self,
        cwd: &Path,
        commands: &[TuiCommandEntry],
        text: &str,
        cursor: usize,
    ) -> Vec<CompletionRow>;

    /// Short left title for [`Pane`] chrome (selection counter is a title affix).
    fn label(&self) -> &'static str;

    /// Footer key hints for the suggest dock pane.
    fn hints(&self) -> &'static str;
}
