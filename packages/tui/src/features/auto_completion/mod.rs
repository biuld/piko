use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table, TableState},
};
use std::path::Path;

use crate::app::command::TuiCommandEntry;
use crate::ui::components::{NO_MATCHES, row_primary_style, selection_prefix, with_selected_bg};

pub mod command_palette;
pub mod file_browser;
pub mod provider;

use command_palette::CommandPaletteProvider;
use file_browser::FileBrowserProvider;
use provider::AutoCompleteProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellStyle {
    /// Primary column: `text` idle, `accent` when the row is selected.
    Default,
    /// Secondary / description column.
    Dim,
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

        self.items = items;
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
    }

    /// Renders the completions list in the allocated area (Minimal pane, no search).
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &crate::theme::Theme) {
        use crate::ui::components::pane::{PaneSpec, PaneTitleAffix, render_pane};

        let (label, hints) = if let Some(idx) = self.active_provider_idx {
            (self.providers[idx].label(), self.providers[idx].hints())
        } else {
            ("suggestions", "Esc cancel")
        };

        let total = self.items.len();
        let selected_one = if total == 0 { 0 } else { self.selected + 1 };
        let spec = PaneSpec::minimal(label)
            .no_search()
            .affix(PaneTitleAffix::selection(selected_one, total))
            .hints(hints)
            .focused(true); // suggestions capture nav while open

        let Some(areas) = render_pane(frame, area, &spec, theme) else {
            return;
        };
        let content = areas.content;

        if self.items.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(vec![Span::styled(
                    format!("  {NO_MATCHES}"),
                    Style::default().fg(theme.dim),
                )])),
                content,
            );
            return;
        }

        // Calculate maximum content width for each provider-defined column.
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

        let mut widths = Vec::with_capacity(num_cols + 1);
        widths.push(ratatui::layout::Constraint::Length(2));
        for (col_idx, width) in max_col_widths.iter().enumerate() {
            let width = (*width as u16).max(1);
            if col_idx < num_cols - 1 {
                widths.push(ratatui::layout::Constraint::Length(width.saturating_add(2)));
            } else {
                widths.push(ratatui::layout::Constraint::Min(width));
            }
        }

        let rows: Vec<Row<'_>> = self
            .items
            .iter()
            .enumerate()
            .map(|(idx, row)| {
                let is_selected = idx == self.selected;
                let marker_style = if is_selected {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let mut cells = vec![Cell::from(Line::from(Span::styled(
                    selection_prefix(is_selected),
                    marker_style,
                )))];
                for cell in &row.cells {
                    let style = match cell.style {
                        CellStyle::Default => with_selected_bg(
                            row_primary_style(is_selected, theme),
                            is_selected,
                            theme,
                        ),
                        CellStyle::Dim => Style::default().fg(theme.dim),
                    };

                    cells.push(Cell::from(Line::from(Span::styled(
                        cell.text.clone(),
                        style,
                    ))));
                }
                Row::new(cells)
            })
            .collect();

        let table = Table::new(rows, widths).row_highlight_style(with_selected_bg(
            row_primary_style(true, theme),
            true,
            theme,
        ));

        let mut state = TableState::default();
        state.select(Some(self.selected));
        frame.render_stateful_widget(table, content, &mut state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::command::{CommandTarget, LocalCommandId};

    fn commands() -> Vec<TuiCommandEntry> {
        vec![TuiCommandEntry {
            slash: "/help".to_string(),
            title: "Help".to_string(),
            detail: "show help".to_string(),
            target: CommandTarget::Local(LocalCommandId::Help),
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
}
