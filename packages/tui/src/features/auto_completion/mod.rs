use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};
use std::path::Path;

use piko_protocol::CommandCatalogItem;

pub mod command_palette;
pub mod file_browser;
pub mod provider;

use command_palette::CommandPaletteProvider;
use file_browser::FileBrowserProvider;
use provider::AutoCompleteProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellStyle {
    Default,
    Dim,
    Accent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionCell {
    pub text: String,
    pub style: CellStyle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRow {
    pub replacement: String,
    pub start: usize,
    pub end: usize,
    pub cells: Vec<CompletionCell>,
    pub keep_active: bool,
}

pub struct AutoComplete {
    pub active: bool,
    pub items: Vec<CompletionRow>,
    pub selected: usize,
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
            items: Vec::new(),
            selected: 0,
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
        self.items.len()
    }

    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1).min(self.items.len() - 1);
        }
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Accepts the currently selected completion item.
    /// Clears selection and deactivates if keep_active is false.
    pub fn accept(&mut self) -> Option<CompletionRow> {
        let item = self.items.get(self.selected).cloned();
        if item.as_ref().is_some_and(|i| !i.keep_active) {
            self.active = false;
            self.items.clear();
            self.selected = 0;
            self.active_provider_idx = None;
        }
        item
    }

    pub fn clear(&mut self) {
        self.active = false;
        self.items.clear();
        self.selected = 0;
        self.active_provider_idx = None;
    }

    /// Updates completions state based on current editor text and cursor.
    pub fn update(
        &mut self,
        cwd: &Path,
        commands: &[CommandCatalogItem],
        text: &str,
        cursor: usize,
    ) {
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

        self.items = items;
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
    }

    /// Renders the completions list in the allocated area.
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &crate::theme::Theme) {
        if self.items.is_empty() {
            let list = List::new(vec![ListItem::new(Line::from(vec![Span::styled(
                "  no matches",
                Style::default().fg(theme.dim),
            )]))])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border_muted))
                    .title("suggestions [0/0]"),
            );
            frame.render_widget(list, area);
            return;
        }

        // Calculate maximum width of each column to align them perfectly.
        // Assume all rows have the same number of columns.
        let num_cols = self.items[0].cells.len();
        let mut max_col_widths = vec![0; num_cols];
        for item in &self.items {
            for (col_idx, cell) in item.cells.iter().enumerate() {
                if col_idx < num_cols {
                    max_col_widths[col_idx] = max_col_widths[col_idx].max(cell.text.len());
                }
            }
        }
        // Cap column widths at reasonable limits to prevent stretching
        for width in max_col_widths.iter_mut().take(num_cols.saturating_sub(1)) {
            *width = (*width).min(40);
        }

        let list_items: Vec<ListItem<'_>> = self
            .items
            .iter()
            .enumerate()
            .map(|(idx, row)| {
                let is_selected = idx == self.selected;
                let marker = if is_selected { "> " } else { "  " };

                let mut spans = vec![Span::raw(marker)];
                for (col_idx, cell) in row.cells.iter().enumerate() {
                    let style = match cell.style {
                        CellStyle::Default => {
                            if is_selected {
                                Style::default()
                                    .fg(theme.accent)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default()
                            }
                        }
                        CellStyle::Dim => Style::default().fg(theme.dim),
                        CellStyle::Accent => {
                            if is_selected {
                                Style::default()
                                    .fg(theme.accent)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme.accent)
                            }
                        }
                    };

                    let cell_text = if col_idx < num_cols - 1 {
                        // Pad all but the last column
                        format!("{:<width$}", cell.text, width = max_col_widths[col_idx])
                    } else {
                        cell.text.clone()
                    };

                    spans.push(Span::styled(cell_text, style));
                    if col_idx < num_cols - 1 {
                        spans.push(Span::raw("  ")); // Spacing between columns
                    }
                }
                ListItem::new(Line::from(spans))
            })
            .collect();

        let title = if let Some(idx) = self.active_provider_idx {
            self.providers[idx].title(self.selected + 1, self.items.len())
        } else {
            format!("suggestions [{}/{}]", self.selected + 1, self.items.len())
        };

        let list = List::new(list_items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_muted))
                .title(title),
        );

        let mut state = ListState::default();
        state.select(Some(self.selected));
        frame.render_stateful_widget(list, area, &mut state);
    }
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
        let mut ac = AutoComplete::new();
        ac.update(Path::new("."), &commands(), "/zzz", 4);
        assert!(ac.active);
        assert!(ac.items.is_empty());
    }

    #[test]
    fn slash_completion_uses_command_token_range() {
        let mut ac = AutoComplete::new();
        ac.update(Path::new("."), &commands(), "/he", 3);
        assert!(ac.active);
        let help = ac
            .items
            .iter()
            .find(|item| item.cells[0].text == "/help")
            .unwrap();
        assert_eq!(help.start, 0);
        assert_eq!(help.end, 3);
        assert_eq!(help.replacement, "/help ");
    }

    #[test]
    fn slash_trigger_inactive_in_arguments() {
        let mut ac = AutoComplete::new();
        ac.update(Path::new("."), &commands(), "/help now", 6);
        assert!(!ac.active);
    }

    #[test]
    fn test_command_completions_deduplicates_aliases() {
        let commands = vec![
            CommandCatalogItem {
                id: "models".to_string(),
                title: "Models".to_string(),
                detail: "List and set default model".to_string(),
                action: CommandCatalogAction::Models,
                slash_names: vec!["/models".to_string(), "/model".to_string()],
                visible_in_palette: true,
            },
            CommandCatalogItem {
                id: "sessions".to_string(),
                title: "Sessions".to_string(),
                detail: "List and open sessions".to_string(),
                action: CommandCatalogAction::Sessions,
                slash_names: vec![
                    "/sessions".to_string(),
                    "/session".to_string(),
                    "/resume".to_string(),
                ],
                visible_in_palette: true,
            },
        ];

        let mut ac = AutoComplete::new();
        ac.update(Path::new("."), &commands, "/m", 2);
        assert_eq!(ac.items.len(), 1);
        assert_eq!(ac.items[0].cells[0].text, "/models");

        ac.update(Path::new("."), &commands, "/se", 3);
        assert_eq!(ac.items.len(), 1);
        assert_eq!(ac.items[0].cells[0].text, "/sessions");

        ac.update(Path::new("."), &commands, "/res", 4);
        assert_eq!(ac.items.len(), 1);
        assert_eq!(ac.items[0].cells[0].text, "/resume");
    }
}
