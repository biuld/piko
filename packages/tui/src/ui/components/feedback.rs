//! Shared visual & interaction feedback primitives (component-feedback PRD).
//!
//! One language for selected / active / focused, state glyphs, spinners,
//! empty/loading copy, and hint styling across base components.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme::Theme;

// ── Glyphs ───────────────────────────────────────────────────────────────────
//
// One shared glyph language for lists, agent diagrams, tree connectors, and
// disclosure. Feature modules must not invent private duplicates.

/// Keyboard selection caret (selected row).
pub const SELECTION_CARET: &str = "❯";
/// Active / live / in-force marker.
pub const ACTIVE_MARKER: &str = "●";
/// Idle / hollow / detached marker.
pub const IDLE_MARKER: &str = "○";
/// Group drill-in affordance (hierarchical menu).
pub const GROUP_DRILL: &str = "▸";
/// Collapsed disclosure (tool cards, sections). Same shape as [`GROUP_DRILL`].
pub const DISCLOSURE_COLLAPSED: &str = "▸";
/// Expanded disclosure (tool cards, sections).
pub const DISCLOSURE_EXPANDED: &str = "▾";
/// Success glyph.
pub const SUCCESS_GLYPH: &str = "✓";
/// Failure glyph.
pub const FAIL_GLYPH: &str = "✗";
/// Cancelled / aborted glyph.
pub const CANCELLED_GLYPH: &str = "⊘";
/// Running / in-progress mark on compact chrome (not an animated spinner).
pub const RUNNING_GLYPH: &str = "○";
/// Scroll hint: more content above the visible window.
pub const SCROLL_UP_GLYPH: &str = "↑";
/// Scroll hint: more content below the visible window.
pub const SCROLL_DOWN_GLYPH: &str = "↓";
/// Compact chip separator (status · duration · tokens). Surrounding spaces are
/// part of the constant so call sites can use it as a single span.
pub const CHIP_SEP: &str = " · ";

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

/// Interactive frame chrome: focused → `border`, rest → `border_muted`.
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
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    }
}

/// Secondary detail on a row (always dim hierarchy).
pub fn row_detail_style(theme: &Theme) -> Style {
    Style::default().fg(theme.dim)
}

/// Mark an in-force (active) row through its label color instead of a marker:
/// accent text when the row is active but not keyboard-selected.
pub fn with_active_text(style: Style, is_selected: bool, is_active: bool, theme: &Theme) -> Style {
    if !is_selected && is_active {
        style.fg(theme.accent)
    } else {
        style
    }
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

/// Optional hover background. `Reset` means the theme deliberately leaves
/// hover rows unpainted.
pub fn hover_bg(theme: &Theme) -> Option<Color> {
    let color = theme.bg_hover;
    (!matches!(color, Color::Reset)).then_some(color)
}

/// Apply optional selected background onto a style.
pub fn with_selected_bg(style: Style, selected: bool, theme: &Theme) -> Style {
    if selected && let Some(bg) = selected_bg(theme) {
        return style.bg(bg);
    }
    style
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn selection_prefix_width_stable() {
        assert_eq!(selection_prefix(true).chars().count(), 2);
        assert_eq!(selection_prefix(false).chars().count(), 2);
        assert!(selection_prefix(true).starts_with(SELECTION_CARET));
        let theme = Theme::dark();
        let active = with_active_text(Style::default().fg(theme.text), false, true, &theme);
        assert_eq!(active.fg, Some(theme.accent));
        let plain = with_active_text(Style::default().fg(theme.text), false, false, &theme);
        assert_eq!(plain.fg, Some(theme.text));
        assert!(
            loading_line(0, &Theme::dark())
                .spans
                .iter()
                .any(|s| s.content.contains("loading"))
        );
        assert_eq!(EMPTY_LIST, "No items");
        assert_eq!(hover_bg(&theme), Some(theme.bg_hover));
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

    #[test]
    fn shared_glyphs_are_stable() {
        assert_eq!(GROUP_DRILL, "▸");
        assert_eq!(DISCLOSURE_COLLAPSED, GROUP_DRILL);
        assert_ne!(DISCLOSURE_EXPANDED, DISCLOSURE_COLLAPSED);
        assert_eq!(ACTIVE_MARKER, "●");
        assert_eq!(IDLE_MARKER, "○");
        assert_eq!(RUNNING_GLYPH, IDLE_MARKER);
        assert_eq!(SUCCESS_GLYPH, "✓");
        assert_eq!(FAIL_GLYPH, "✗");
        assert_eq!(CANCELLED_GLYPH, "⊘");
        assert_eq!(SCROLL_UP_GLYPH, "↑");
        assert_eq!(SCROLL_DOWN_GLYPH, "↓");
        assert_eq!(CHIP_SEP, " · ");
    }
}
