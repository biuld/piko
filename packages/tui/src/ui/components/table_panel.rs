#![allow(clippy::type_complexity, clippy::large_enum_variant)]

//! Table panel — tabular overlay body on shared [`Pane`](crate::ui::components::pane) chrome.
//!
//! Layout: title · search · content · tip · footer (hints or reserved interactive).

use crate::theme::Theme;
use crate::ui::components::pane::{PaneFooter, PaneSearch, PaneSpec, PaneTitleAffix, render_pane};
use crate::ui::components::{row_primary_style, with_selected_bg};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::Line,
    widgets::{Paragraph, Row, Table, TableState},
};

/// ActionPrompt represents the footer area content of the table panel.
pub enum ActionPrompt<'a> {
    /// A single line of help/legend text
    Legend(&'a str),
    /// A custom interactive sub-panel, e.g. for user input (like SummaryPrompt)
    Interactive {
        height: u16,
        render: Box<dyn FnOnce(&mut Frame<'_>, Rect) + 'a>,
    },
}

/// TableBody represents the main scrollable content area.
pub enum TableBody<'a> {
    /// Render standard table rows
    Rows {
        widths: &'a [Constraint],
        rows: Vec<Row<'a>>,
        selected_idx: usize,
    },
    /// Render placeholder/status message
    Message(Paragraph<'a>),
}

/// A high-level container for searchable list/table overlays (session list, tree).
pub struct TablePanel<'a> {
    pub left_title: String,
    /// Right-title chips (scope/mode · selection · …). Owned by Pane.
    pub affixes: Vec<PaneTitleAffix>,
    /// Muted tip under content (secondary bindings / mode copy).
    pub help_text: &'a str,
    pub search_line: Line<'a>,
    pub body: TableBody<'a>,
    pub action_prompt: ActionPrompt<'a>,
    /// Kept for API stability; Pane layout no longer uses an extra gap row.
    pub gap: bool,
    /// When true (default for open overlays), frame uses accent border.
    pub focused: bool,
}

impl<'a> TablePanel<'a> {
    pub fn render(self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let (footer, interactive) = match self.action_prompt {
            ActionPrompt::Legend(txt) => (PaneFooter::Hints(txt), None),
            ActionPrompt::Interactive { height, render } => {
                (PaneFooter::Reserved { height }, Some(render))
            }
        };

        let tip = if self.help_text.is_empty() {
            None
        } else {
            Some(self.help_text)
        };

        let spec = PaneSpec::new(&self.left_title)
            .mode(crate::ui::components::pane::PaneMode::Standard)
            .title_affixes(self.affixes)
            .search(PaneSearch::Custom(self.search_line))
            .tip(tip)
            .footer(footer)
            .focused(self.focused);

        let Some(areas) = render_pane(frame, area, &spec, theme) else {
            return;
        };

        match self.body {
            TableBody::Message(p) => {
                frame.render_widget(p, areas.content);
            }
            TableBody::Rows {
                widths,
                rows,
                selected_idx,
            } => {
                let mut table_state = TableState::default().with_selected(Some(selected_idx));
                let highlight = with_selected_bg(row_primary_style(true, theme), true, theme);
                let table = Table::new(rows, widths).row_highlight_style(highlight);
                frame.render_stateful_widget(table, areas.content, &mut table_state);
            }
        }

        if let (Some(footer_area), Some(render_footer)) = (areas.footer, interactive) {
            render_footer(frame, footer_area);
        }

        let _ = self.gap;
    }
}
