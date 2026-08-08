use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use std::path::Path;

use crate::app::command::TuiCommandEntry;
use crate::ui::components::selectable_list::{
    ColumnCell, SelectableItem, SelectableList, SelectablePanelBody, paint_selectable_panel,
};
use crate::ui::components::{NO_MATCHES, pane::PaneSpec, pane::PaneTitleAffix};

pub mod command_palette;
pub mod file_browser;
pub mod provider;

use command_palette::CommandPaletteProvider;
use file_browser::FileBrowserProvider;
use provider::AutoCompleteProvider;

/// One completion suggestion (domain payload + column cells for paint).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRow {
    pub replacement: String,
    pub start: usize,
    pub end: usize,
    pub cells: Vec<ColumnCell>,
    pub keep_active: bool,
}

pub struct AutoComplete {
    pub active: bool,
    pub list: SelectableList<CompletionRow>,
    pub active_provider_idx: Option<usize>,
    pub providers: Vec<Box<dyn AutoCompleteProvider>>,
}

impl Default for AutoComplete {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoComplete {
    pub fn new() -> Self {
        Self {
            active: false,
            list: SelectableList::new(Vec::new()),
            active_provider_idx: None,
            providers: vec![
                Box::new(CommandPaletteProvider),
                Box::new(FileBrowserProvider),
            ],
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn select_next(&mut self) {
        self.list.select_next("", |_| true);
    }

    pub fn select_prev(&mut self) {
        self.list.select_prev("", |_| true);
    }

    /// Select the suggestion at `idx` (pointer clicks), clamped to the list.
    pub fn select_index(&mut self, idx: usize) {
        if idx < self.list.len() {
            self.list.selected = idx;
        }
    }

    /// Accepts the currently selected completion item.
    /// Clears selection and deactivates if keep_active is false.
    pub fn accept(&mut self) -> Option<CompletionRow> {
        let item = self.list.selected_item().cloned();
        if item.as_ref().is_some_and(|i| !i.keep_active) {
            self.clear();
        }
        item
    }

    pub fn clear(&mut self) {
        self.active = false;
        self.list.clear();
        self.active_provider_idx = None;
    }

    /// Updates completions state based on current editor text and cursor.
    pub fn update(&mut self, cwd: &Path, commands: &[TuiCommandEntry], text: &str, cursor: usize) {
        let matched_idx = self
            .providers
            .iter()
            .position(|provider| provider.is_triggered(text, cursor));

        self.active_provider_idx = matched_idx;
        self.active = matched_idx.is_some();

        let mut items = if let Some(idx) = matched_idx {
            self.providers[idx].update(cwd, commands, text, cursor)
        } else {
            Vec::new()
        };

        // Safety limit to avoid performance issues
        items.truncate(100);

        let prev = self.list.selected;
        self.list = SelectableList::new(items);
        self.list.selected = prev.min(self.list.len().saturating_sub(1));
    }

    /// Renders the completions list in the allocated area (Minimal pane, no search).
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &crate::theme::Theme) {
        let (label, hints) = if let Some(idx) = self.active_provider_idx {
            (self.providers[idx].label(), self.providers[idx].hints())
        } else {
            ("suggestions", "Esc cancel")
        };

        let total = self.list.len();
        let selected_one = if total == 0 {
            0
        } else {
            self.list.selected + 1
        };
        let spec = PaneSpec::minimal(label)
            .no_search()
            .affix(PaneTitleAffix::selection(selected_one, total))
            .hints(hints)
            .focused(true);

        let items: Vec<SelectableItem> = self
            .list
            .items
            .iter()
            .map(|row| SelectableItem::columns(row.cells.clone()))
            .collect();

        let body = if items.is_empty() {
            SelectablePanelBody::Message(Paragraph::new(Line::from(vec![Span::styled(
                format!("  {NO_MATCHES}"),
                Style::default().fg(theme.dim),
            )])))
        } else {
            SelectablePanelBody::Columns {
                items: &items,
                selected: self.list.selected,
                widths: None,
            }
        };

        let _ = paint_selectable_panel(frame, area, theme, &spec, body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::command::{CommandTarget, LocalCommandId};

    fn commands() -> Vec<TuiCommandEntry> {
        vec![TuiCommandEntry {
            slash: "/resume".to_string(),
            title: "Sessions".to_string(),
            detail: "list and open sessions".to_string(),
            target: CommandTarget::Local(LocalCommandId::Sessions),
        }]
    }

    #[test]
    fn slash_trigger_stays_active_with_no_matches() {
        let mut ac = AutoComplete::new();
        ac.update(Path::new("."), &commands(), "/zzz", 4);
        assert!(ac.active);
        assert!(ac.list.is_empty());
    }

    #[test]
    fn slash_completion_uses_command_token_range() {
        let mut ac = AutoComplete::new();
        ac.update(Path::new("."), &commands(), "/res", 4);
        assert!(ac.active);
        let resume = ac
            .list
            .items
            .iter()
            .find(|item| item.cells[0].text == "/resume")
            .unwrap();
        assert_eq!(resume.start, 0);
        assert_eq!(resume.end, 4);
        assert_eq!(resume.replacement, "/resume ");
    }

    #[test]
    fn slash_trigger_inactive_in_arguments() {
        let mut ac = AutoComplete::new();
        ac.update(Path::new("."), &commands(), "/resume now", 8);
        assert!(!ac.active);
    }
}
