//! Pane — reusable framed chrome for overlay surfaces.
//!
//! Matches product overlay language (Settings / lists):
//!
//! ```text
//! ┌─ Title                                 [x] ─┐
//! │ / to search                                 │
//! │ ─────────────────────────────────────────── │  ← rule under search
//! │ Content…                                    │
//! │ Tip · …                                     │
//! │ ↑/↓ nav | Enter open | Esc close            │
//! └─────────────────────────────────────────────┘
//! ```
//!
//! Surface feature logic lives outside Pane; Pane owns frame + vertical zones.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::theme::Theme;
use crate::ui::components::feedback::{frame_border_style, hint_line};

/// Default search placeholder (product convention: type to filter).
pub const SEARCH_PLACEHOLDER: &str = "to search";

/// Whether the pane reserves a search line under the title.
#[derive(Clone, Debug, Default)]
pub enum PaneSearch<'a> {
    /// No search row.
    #[default]
    Hidden,
    /// Placeholder when empty; live filter text when non-empty.
    Shown {
        filter: &'a str,
        /// Text after `/ ` when empty; defaults to [`SEARCH_PLACEHOLDER`].
        placeholder: Option<&'a str>,
    },
    /// Fully custom search / prompt line (label editor, scoped filters, …).
    Custom(Line<'a>),
}

/// Footer zone under content (and optional tip).
#[derive(Clone, Copy, Debug, Default)]
pub enum PaneFooter<'a> {
    #[default]
    None,
    /// Dim binding legend; multi-line if `text` contains `\n`.
    Hints(&'a str),
    /// Reserve `height` rows; caller paints into [`PaneAreas::footer`].
    Reserved { height: u16 },
}

/// Spec for chrome paint. Content is filled by the caller after layout.
#[derive(Clone, Debug)]
pub struct PaneSpec<'a> {
    pub title: &'a str,
    /// Right-aligned title affix (e.g. `[x]`, `3/9`).
    pub title_right: Option<&'a str>,
    pub search: PaneSearch<'a>,
    /// Hairline rule under the search row (on when search is visible, unless cleared).
    pub search_rule: bool,
    pub footer: PaneFooter<'a>,
    /// Optional tip line above the footer.
    pub tip: Option<&'a str>,
    pub focused: bool,
}

impl<'a> PaneSpec<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            title_right: None,
            search: PaneSearch::Hidden,
            search_rule: true,
            footer: PaneFooter::None,
            tip: None,
            focused: true,
        }
    }

    pub fn title_right(mut self, right: impl Into<Option<&'a str>>) -> Self {
        self.title_right = right.into();
        self
    }

    pub fn search_filter(mut self, filter: &'a str) -> Self {
        self.search = PaneSearch::Shown {
            filter,
            placeholder: None,
        };
        self
    }

    pub fn search(mut self, search: PaneSearch<'a>) -> Self {
        self.search = search;
        self
    }

    pub fn search_rule(mut self, on: bool) -> Self {
        self.search_rule = on;
        self
    }

    pub fn hints(mut self, hints: &'a str) -> Self {
        self.footer = if hints.is_empty() {
            PaneFooter::None
        } else {
            PaneFooter::Hints(hints)
        };
        self
    }

    pub fn footer(mut self, footer: PaneFooter<'a>) -> Self {
        self.footer = footer;
        self
    }

    pub fn tip(mut self, tip: impl Into<Option<&'a str>>) -> Self {
        self.tip = tip.into();
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

/// Zones after chrome is painted.
#[derive(Clone, Copy, Debug)]
pub struct PaneAreas {
    /// Main body (list, table, form, …).
    pub content: Rect,
    /// Present only for [`PaneFooter::Reserved`].
    pub footer: Option<Rect>,
    /// Full block interior (search + content + tip + footer).
    #[allow(dead_code)]
    pub inner: Rect,
}

/// Clear area, draw border/title/search/tip/footer chrome, return body rects.
///
/// Returns `None` when the area is too small to show content.
pub fn render_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    spec: &PaneSpec<'_>,
    theme: &Theme,
) -> Option<PaneAreas> {
    frame.render_widget(Clear, area);

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(frame_border_style(spec.focused, theme))
        .title(
            Line::from(Span::styled(
                format!(" {} ", spec.title),
                Style::default().fg(theme.text),
            ))
            .alignment(Alignment::Left),
        );
    if let Some(right) = spec.title_right {
        block = block.title(
            Line::from(Span::styled(
                format!(" {right} "),
                Style::default().fg(theme.dim),
            ))
            .alignment(Alignment::Right),
        );
    }

    let bordered = block.inner(area);
    frame.render_widget(block, area);

    // 1-cell inset so text doesn't sit flush against the border.
    let inner = inset(bordered, PANE_PADDING);
    if inner.width == 0 || inner.height == 0 {
        return None;
    }

    let show_search = !matches!(spec.search, PaneSearch::Hidden);
    let show_rule = show_search && spec.search_rule;
    let show_tip = spec.tip.is_some_and(|t| !t.is_empty());
    let footer_h = footer_height(spec.footer);

    let search_h: u16 = u16::from(show_search);
    let rule_h: u16 = u16::from(show_rule);
    let tip_h: u16 = u16::from(show_tip);
    let chrome = search_h
        .saturating_add(rule_h)
        .saturating_add(tip_h)
        .saturating_add(footer_h);

    if inner.height <= chrome {
        return paint_fallback(frame, inner, spec, theme, footer_h);
    }

    let mut constraints: Vec<Constraint> = Vec::with_capacity(5);
    if show_search {
        constraints.push(Constraint::Length(1));
    }
    if show_rule {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(1));
    if show_tip {
        constraints.push(Constraint::Length(1));
    }
    if footer_h > 0 {
        constraints.push(Constraint::Length(footer_h));
    }

    let chunks = Layout::vertical(constraints).split(inner);
    let mut idx = 0usize;

    if show_search {
        paint_search(frame, chunks[idx], &spec.search, theme);
        idx += 1;
    }
    if show_rule {
        paint_rule(frame, chunks[idx], theme);
        idx += 1;
    }

    let content = chunks[idx];
    idx += 1;

    if show_tip {
        if let Some(tip) = spec.tip {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    tip.to_string(),
                    Style::default().fg(theme.muted),
                ))),
                chunks[idx],
            );
        }
        idx += 1;
    }

    let footer = if footer_h > 0 {
        let footer_area = chunks[idx];
        match spec.footer {
            PaneFooter::Hints(hints) => {
                paint_hints(frame, footer_area, hints, theme);
                None
            }
            PaneFooter::Reserved { .. } => Some(footer_area),
            PaneFooter::None => None,
        }
    } else {
        None
    };

    Some(PaneAreas {
        content,
        footer,
        inner,
    })
}

/// Inner padding (cells) between border and chrome zones.
const PANE_PADDING: u16 = 1;

fn inset(area: Rect, pad: u16) -> Rect {
    if pad == 0 {
        return area;
    }
    let pad_x = pad.min(area.width.saturating_sub(1) / 2);
    let pad_y = pad.min(area.height.saturating_sub(1) / 2);
    Rect {
        x: area.x.saturating_add(pad_x),
        y: area.y.saturating_add(pad_y),
        width: area.width.saturating_sub(pad_x.saturating_mul(2)),
        height: area.height.saturating_sub(pad_y.saturating_mul(2)),
    }
}

fn footer_height(footer: PaneFooter<'_>) -> u16 {
    match footer {
        PaneFooter::None => 0,
        PaneFooter::Hints("") => 0,
        PaneFooter::Hints(text) => text.lines().filter(|l| !l.is_empty()).count().max(1) as u16,
        PaneFooter::Reserved { height } => height.max(1),
    }
}

fn paint_fallback(
    frame: &mut Frame<'_>,
    inner: Rect,
    spec: &PaneSpec<'_>,
    theme: &Theme,
    footer_h: u16,
) -> Option<PaneAreas> {
    if footer_h > 0 && inner.height > footer_h {
        let chunks =
            Layout::vertical([Constraint::Min(1), Constraint::Length(footer_h)]).split(inner);
        let footer = match spec.footer {
            PaneFooter::Hints(hints) => {
                paint_hints(frame, chunks[1], hints, theme);
                None
            }
            PaneFooter::Reserved { .. } => Some(chunks[1]),
            PaneFooter::None => None,
        };
        Some(PaneAreas {
            content: chunks[0],
            footer,
            inner,
        })
    } else if inner.height >= 1 {
        Some(PaneAreas {
            content: inner,
            footer: None,
            inner,
        })
    } else {
        None
    }
}

fn paint_search(frame: &mut Frame<'_>, area: Rect, search: &PaneSearch<'_>, theme: &Theme) {
    match search {
        PaneSearch::Hidden => {}
        PaneSearch::Custom(line) => {
            frame.render_widget(Paragraph::new(line.clone()), area);
        }
        PaneSearch::Shown {
            filter,
            placeholder,
        } => {
            let placeholder = placeholder.unwrap_or(SEARCH_PLACEHOLDER);
            // Screenshot language: `/` + dim "to search", or `/ <filter>`.
            let line = if filter.is_empty() {
                Line::from(vec![
                    Span::styled("/ ", Style::default().fg(theme.dim)),
                    Span::styled(placeholder.to_string(), Style::default().fg(theme.dim)),
                ])
            } else {
                Line::from(vec![
                    Span::styled("/ ", Style::default().fg(theme.muted)),
                    Span::styled(filter.to_string(), Style::default().fg(theme.text)),
                ])
            };
            frame.render_widget(Paragraph::new(line), area);
        }
    }
}

fn paint_rule(frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
    let width = area.width.max(1) as usize;
    let rule = "─".repeat(width);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            rule,
            Style::default().fg(theme.dim),
        ))),
        area,
    );
}

fn paint_hints(frame: &mut Frame<'_>, area: Rect, hints: &str, theme: &Theme) {
    let lines: Vec<Line<'static>> = hints
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| hint_line(l, theme))
        .collect();
    if lines.is_empty() {
        return;
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Section rule line: `Appearance ────────` (label + fill with dim box-drawing).
pub fn section_rule_line(label: &str, width: usize, theme: &Theme) -> Line<'static> {
    let label = label.trim();
    let label_chars = label.chars().count();
    // Screenshot: label then contiguous fill with a single space gap.
    let fill = width.saturating_sub(label_chars).saturating_sub(1);
    let mut spans = vec![Span::styled(
        label.to_string(),
        Style::default().fg(theme.muted),
    )];
    if fill > 0 {
        spans.push(Span::styled(
            format!(" {}", "─".repeat(fill)),
            Style::default().fg(theme.dim),
        ));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_rule_contains_label() {
        let theme = Theme::dark();
        let line = section_rule_line("Appearance", 40, &theme);
        let joined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(joined.starts_with("Appearance"));
        assert!(joined.contains('─'));
    }

    #[test]
    fn pane_spec_builder() {
        let s = PaneSpec::new("Settings")
            .title_right(Some("[x]"))
            .search_filter("")
            .hints("Esc close")
            .focused(true);
        assert_eq!(s.title, "Settings");
        assert!(matches!(s.search, PaneSearch::Shown { filter: "", .. }));
        assert!(matches!(s.footer, PaneFooter::Hints("Esc close")));
        assert!(s.search_rule);
    }

    #[test]
    fn footer_height_counts_lines() {
        assert_eq!(footer_height(PaneFooter::Hints("a\nb")), 2);
        assert_eq!(footer_height(PaneFooter::Hints("a")), 1);
        assert_eq!(footer_height(PaneFooter::Hints("")), 0);
    }
}
