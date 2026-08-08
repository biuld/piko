//! Shared visual & interaction feedback primitives (component-feedback PRD).
//!
//! One language for selected / active / focused, state glyphs, spinners,
//! empty/loading copy, and hint styling across base components.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme::Theme;

// ── Glyphs ───────────────────────────────────────────────────────────────────

/// Keyboard selection caret (selected row).
pub const SELECTION_CARET: &str = "❯";
/// Active / current value marker (already in force).
pub const ACTIVE_MARKER: &str = "●";
/// Idle / hollow marker.
pub const IDLE_MARKER: &str = "○";
/// Group drill-in affordance (hierarchical menu).
pub const GROUP_DRILL: &str = "▸";
/// Success glyph.
pub const SUCCESS_GLYPH: &str = "✓";
/// Failure / cancelled glyph.
pub const FAIL_GLYPH: &str = "✗";

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

// ── Empty / loading copy ─────────────────────────────────────────────────────

pub const EMPTY_LIST: &str = "No items";
pub const NO_MATCHES: &str = "No matches";
pub const LOADING_LABEL: &str = "loading…";

// ── Glyph helpers ────────────────────────────────────────────────────────────

/// Spinner frame for working / loading. Motion stops when the caller stops
/// advancing `frame_idx`.
pub fn spinner_glyph(frame_idx: usize) -> &'static str {
    SPINNER_FRAMES[frame_idx % SPINNER_FRAMES.len()]
}

/// Leading selection marker: `❯ ` when selected, two spaces otherwise.
pub fn selection_prefix(selected: bool) -> String {
    if selected {
        format!("{SELECTION_CARET} ")
    } else {
        "  ".to_string()
    }
}

// ── Styles ───────────────────────────────────────────────────────────────────

/// Interactive frame chrome: focused → `border`, rest → `borderMuted`.
/// Borders never use `accent` (accent is for selection / marks / labels).
pub fn frame_border_style(focused: bool, theme: &Theme) -> Style {
    Style::default().fg(if focused {
        theme.border
    } else {
        theme.border_muted
    })
}

/// Primary label style for a list/table row.
pub fn row_primary_style(selected: bool, theme: &Theme) -> Style {
    if selected {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    }
}

/// Secondary detail on a row (always dim hierarchy).
pub fn row_detail_style(theme: &Theme) -> Style {
    Style::default().fg(theme.dim)
}

/// Footer key-hint line (currently valid keys only).
pub fn hint_style(theme: &Theme) -> Style {
    Style::default().fg(theme.dim)
}

/// Placeholder text in empty fields.
pub fn placeholder_style(theme: &Theme) -> Style {
    Style::default().fg(theme.dim)
}

/// Optional selected-row background when the theme paints `bg_selected`.
pub fn selected_bg(theme: &Theme) -> Option<Color> {
    let c = theme.bg_selected;
    if matches!(c, Color::Reset) {
        None
    } else {
        Some(c)
    }
}

/// Apply optional selected background onto a style.
pub fn with_selected_bg(style: Style, selected: bool, theme: &Theme) -> Style {
    if selected && let Some(bg) = selected_bg(theme) {
        return style.bg(bg);
    }
    style
}

/// Active/current marker span (`●` in accent), distinct from selection caret.
pub fn active_marker_span(theme: &Theme) -> Span<'static> {
    Span::styled(
        format!(" {ACTIVE_MARKER}"),
        Style::default().fg(theme.accent),
    )
}

/// Loading row: spinner + dim label.
pub fn loading_line(frame_idx: usize, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            spinner_glyph(frame_idx).to_string(),
            Style::default().fg(theme.accent),
        ),
        Span::raw(" "),
        Span::styled(LOADING_LABEL.to_string(), Style::default().fg(theme.dim)),
    ])
}

/// Empty / no-matches line.
pub fn empty_line(has_filter: bool, theme: &Theme) -> Line<'static> {
    let text = if has_filter { NO_MATCHES } else { EMPTY_LIST };
    Line::from(Span::styled(text, Style::default().fg(theme.dim)))
}

/// Default list/overlay footer hints.
pub fn default_list_hints() -> &'static str {
    "↑/↓ navigate · Enter confirm · Esc cancel"
}

/// Settings catalog / branch: open or back (pipe-separated, screenshot language).
pub fn settings_open_hints(at_root: bool) -> &'static str {
    if at_root {
        "↑/↓ nav | Enter open | → expand | Esc close"
    } else {
        "↑/↓ nav | Enter open | Esc back"
    }
}

/// Settings choice leaf: apply value.
pub fn settings_apply_hints() -> &'static str {
    "↑/↓ nav | Enter apply | Esc back"
}

/// Title line for filterable lists: product title + counter.
/// Filter is shown separately on the Pane search row (not duplicated here).
pub fn list_title(title: &str, _filter: &str, selected_one_based: usize, total: usize) -> String {
    format!("{title} [{selected_one_based}/{total}]")
}

/// Hint spans for embedding under a list (dim).
pub fn hint_line(text: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(text.to_string(), hint_style(theme)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn selection_prefix_width_stable() {
        assert_eq!(selection_prefix(true).chars().count(), 2);
        assert_eq!(selection_prefix(false).chars().count(), 2);
        assert!(selection_prefix(true).starts_with(SELECTION_CARET));
        assert!(
            loading_line(0, &Theme::dark())
                .spans
                .iter()
                .any(|s| s.content.contains("loading"))
        );
        assert_eq!(EMPTY_LIST, "No items");
    }

    #[test]
    fn spinner_cycles() {
        assert_eq!(spinner_glyph(0), SPINNER_FRAMES[0]);
        assert_eq!(spinner_glyph(SPINNER_FRAMES.len()), SPINNER_FRAMES[0]);
    }

    #[test]
    fn empty_copy_depends_on_filter() {
        let theme = Theme::dark();
        let empty = empty_line(false, &theme);
        let no_match = empty_line(true, &theme);
        assert!(empty.spans[0].content.contains("No items"));
        assert!(no_match.spans[0].content.contains("No matches"));
    }

    #[test]
    fn focused_border_uses_border_not_accent() {
        let theme = Theme::dark();
        assert_ne!(theme.border, theme.accent);
        assert_eq!(frame_border_style(true, &theme).fg, Some(theme.border));
        assert_eq!(
            frame_border_style(false, &theme).fg,
            Some(theme.border_muted)
        );
    }
}
