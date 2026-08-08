//! Browse / overlay paint: [`Pane`] chrome + selectable body (replaces TablePanel).

use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::Line,
    widgets::Paragraph,
};

use super::{
    SelectableItem,
    columns::{paint_column_items_with_widths, paint_rich_lines},
};
use crate::theme::Theme;
use crate::ui::components::pane::{PaneAreas, PaneSpec, render_pane};

/// Content area of a Standard (or custom) selectable overlay.
#[allow(clippy::large_enum_variant)]
pub enum SelectablePanelBody<'a> {
    /// Multi-column rows already filtered; `selected` indexes into `items`.
    Columns {
        items: &'a [SelectableItem],
        selected: usize,
        /// Data-column constraints (no caret column). `None` ⇒ auto-size.
        widths: Option<&'a [Constraint]>,
    },
    /// Full-line paint (session tree: connectors + mixed roles).
    RichLines {
        lines: &'a [Line<'static>],
        selected: usize,
    },
    /// Loading / empty / error placeholder.
    Message(Paragraph<'a>),
}

/// Paint Pane chrome + body. Returns layout areas so callers can fill a reserved
/// footer (e.g. SummaryPrompt).
pub fn paint_selectable_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: &Theme,
    spec: &PaneSpec<'_>,
    body: SelectablePanelBody<'_>,
) -> Option<PaneAreas> {
    let areas = render_pane(frame, area, spec, theme)?;
    match body {
        SelectablePanelBody::Message(p) => {
            frame.render_widget(p, areas.content);
        }
        SelectablePanelBody::Columns {
            items,
            selected,
            widths,
        } => {
            if !items.is_empty() {
                let refs: Vec<&SelectableItem> = items.iter().collect();
                paint_column_items_with_widths(
                    frame,
                    areas.content,
                    &refs,
                    selected,
                    theme,
                    widths,
                );
            }
        }
        SelectablePanelBody::RichLines { lines, selected } => {
            paint_rich_lines(frame, areas.content, lines, selected, theme);
        }
    }
    Some(areas)
}
