//! Typed semantic color catalog for a piko TUI theme.
//!
//! Modelled after Grok's `Theme` (xai-grok-pager-render): every render path
//! reads named slots; no ad-hoc RGB. Custom TOML themes fill the same slots.
//!
//! # Catalog size
//!
//! [`Theme::SLOT_COUNT`] is **96** color slots (plus `name`).

use ratatui::style::Color;

use crate::terminal::ColorLevel;

/// Fully resolved theme: every semantic color is a typed field.
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,

    // ── Surfaces (8) ─────────────────────────────────────────────────────────
    /// Default viewport / island body background.
    pub bg_base: Color,
    /// Raised surface (composer input, elevated panels).
    pub bg_elevated: Color,
    /// Recessed surface (code blocks, nested panes).
    pub bg_sunken: Color,
    /// Soft highlight fill.
    pub bg_highlight: Color,
    /// Mouse / hover row fill.
    pub bg_hover: Color,
    /// Selected list/tree row background.
    pub bg_selected: Color,
    /// Terminal/emulator background when painted explicitly.
    pub bg_terminal: Color,
    /// Text selection / visual mode background.
    pub bg_visual: Color,

    // ── Role accents (9) — vertical marks / author labels ─────────────────────
    pub accent_user: Color,
    pub accent_assistant: Color,
    pub accent_thinking: Color,
    pub accent_tool: Color,
    pub accent_system: Color,
    pub accent_error: Color,
    pub accent_success: Color,
    pub accent_running: Color,
    pub accent_skill: Color,

    // ── UI / mode accents (5) ────────────────────────────────────────────────
    /// Selection highlight, active marks (never panel chrome borders).
    pub accent: Color,
    /// Secondary accent (session labels, alternate marks).
    pub accent_alt: Color,
    pub accent_plan: Color,
    pub accent_model: Color,
    pub accent_remember: Color,

    // ── Text hierarchy (5) ───────────────────────────────────────────────────
    pub text: Color,
    pub text_secondary: Color,
    /// Tertiary — placeholders, separators (`·`), meta punctuation.
    pub dim: Color,
    /// Secondary — descriptions, collapsed meta.
    pub muted: Color,
    /// Bright gray — secondary labels, tool accents.
    pub gray_bright: Color,

    // ── Status (4) ───────────────────────────────────────────────────────────
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub info: Color,

    // ── Borders / chrome (6) ─────────────────────────────────────────────────
    pub border: Color,
    pub border_muted: Color,
    pub prompt_border: Color,
    pub prompt_border_active: Color,
    pub selection_border: Color,
    pub hover_border: Color,

    // ── Content semantic (3) ─────────────────────────────────────────────────
    pub command: Color,
    pub path: Color,
    pub running: Color,

    // ── Scrollbar (2) ────────────────────────────────────────────────────────
    pub scrollbar_bg: Color,
    pub scrollbar_fg: Color,

    // ── Diff (6) ─────────────────────────────────────────────────────────────
    pub diff_delete_bg: Color,
    pub diff_delete_fg: Color,
    pub diff_insert_bg: Color,
    pub diff_insert_fg: Color,
    pub diff_equal_fg: Color,
    pub diff_gutter_fg: Color,

    // ── Transcript blocks (11) ───────────────────────────────────────────────
    pub user_message_bg: Color,
    pub user_message_text: Color,
    pub tool_pending_bg: Color,
    pub tool_success_bg: Color,
    pub tool_error_bg: Color,
    pub tool_title: Color,
    pub tool_output: Color,
    pub custom_message_bg: Color,
    pub custom_message_text: Color,
    pub custom_message_label: Color,
    pub thinking_text: Color,

    // ── Markdown (18) ────────────────────────────────────────────────────────
    pub md_heading_h1: Color,
    pub md_heading_h2: Color,
    pub md_heading_h3: Color,
    pub md_heading_h4: Color,
    pub md_heading_h5: Color,
    pub md_heading_h6: Color,
    pub md_code: Color,
    pub md_code_bg: Color,
    pub md_text: Color,
    pub md_muted: Color,
    pub md_link: Color,
    pub md_link_url: Color,
    pub md_quote: Color,
    pub md_quote_border: Color,
    pub md_hr: Color,
    pub md_list_bullet: Color,
    pub md_task_checked: Color,
    pub md_task_unchecked: Color,

    // ── Syntax (9) ───────────────────────────────────────────────────────────
    pub syntax_comment: Color,
    pub syntax_keyword: Color,
    pub syntax_function: Color,
    pub syntax_variable: Color,
    pub syntax_string: Color,
    pub syntax_number: Color,
    pub syntax_type: Color,
    pub syntax_operator: Color,
    pub syntax_punctuation: Color,

    // ── Thinking level picker (6) ────────────────────────────────────────────
    pub thinking_off: Color,
    pub thinking_minimal: Color,
    pub thinking_low: Color,
    pub thinking_medium: Color,
    pub thinking_high: Color,
    pub thinking_xhigh: Color,

    // ── Misc (4) ─────────────────────────────────────────────────────────────
    pub bash_mode: Color,
    pub paste_bg: Color,
    pub paste_fg: Color,
    pub paste_dim: Color,
}

/// Macro: canonical snake_case slot list. Order is stable for docs / counts.
macro_rules! theme_slots {
    ($m:ident) => {
        $m! {
            // Surfaces (8)
            bg_base, bg_elevated, bg_sunken, bg_highlight, bg_hover, bg_selected,
            bg_terminal, bg_visual,
            // Role accents (9)
            accent_user, accent_assistant, accent_thinking, accent_tool, accent_system,
            accent_error, accent_success, accent_running, accent_skill,
            // UI / mode (5)
            accent, accent_alt, accent_plan, accent_model, accent_remember,
            // Text (5)
            text, text_secondary, dim, muted, gray_bright,
            // Status (4)
            success, error, warning, info,
            // Borders (6)
            border, border_muted, prompt_border, prompt_border_active,
            selection_border, hover_border,
            // Content semantic (3)
            command, path, running,
            // Scrollbar (2)
            scrollbar_bg, scrollbar_fg,
            // Diff (6)
            diff_delete_bg, diff_delete_fg, diff_insert_bg, diff_insert_fg,
            diff_equal_fg, diff_gutter_fg,
            // Transcript (11)
            user_message_bg, user_message_text, tool_pending_bg, tool_success_bg,
            tool_error_bg, tool_title, tool_output, custom_message_bg,
            custom_message_text, custom_message_label, thinking_text,
            // Markdown (18)
            md_heading_h1, md_heading_h2, md_heading_h3, md_heading_h4,
            md_heading_h5, md_heading_h6, md_code, md_code_bg, md_text, md_muted,
            md_link, md_link_url, md_quote, md_quote_border, md_hr, md_list_bullet,
            md_task_checked, md_task_unchecked,
            // Syntax (9)
            syntax_comment, syntax_keyword, syntax_function, syntax_variable,
            syntax_string, syntax_number, syntax_type, syntax_operator,
            syntax_punctuation,
            // Thinking levels (6)
            thinking_off, thinking_minimal, thinking_low, thinking_medium,
            thinking_high, thinking_xhigh,
            // Misc (4)
            bash_mode, paste_bg, paste_fg, paste_dim,
        }
    };
}

impl Theme {
    /// Number of color slots (excludes `name`).
    pub const SLOT_COUNT: usize = 96;

    /// Canonical snake_case slot names, in definition order.
    pub const SLOT_NAMES: &'static [&'static str] = {
        macro_rules! names {
            ($($f:ident),* $(,)?) => {
                &[$(stringify!($f),)*]
            };
        }
        theme_slots!(names)
    };

    /// Look up a slot by canonical snake_case name.
    pub fn get(&self, key: &str) -> Color {
        macro_rules! match_fields {
            ($($f:ident),* $(,)?) => {
                match key {
                    $(stringify!($f) => self.$f,)*
                    _ => Color::Reset,
                }
            };
        }
        theme_slots!(match_fields)
    }

    /// Apply a resolved color to a known snake_case slot (unknown keys ignored).
    pub(crate) fn apply_slot(&mut self, key: &str, color: Color) {
        macro_rules! assign {
            ($($f:ident),* $(,)?) => {
                match key {
                    $(stringify!($f) => self.$f = color,)*
                    _ => {}
                }
            };
        }
        theme_slots!(assign);
    }

    /// Overlay non-empty map entries onto a clone of `base`.
    pub(crate) fn with_overrides(
        base: &Self,
        name: String,
        colors: &std::collections::HashMap<String, Color>,
    ) -> Self {
        let mut theme = base.clone();
        theme.name = name;
        for (key, color) in colors {
            theme.apply_slot(key, *color);
        }
        theme
    }

    /// Heading color for markdown levels 1–6 (clamped).
    pub fn md_heading(&self, level: u8) -> Color {
        match level.clamp(1, 6) {
            1 => self.md_heading_h1,
            2 => self.md_heading_h2,
            3 => self.md_heading_h3,
            4 => self.md_heading_h4,
            5 => self.md_heading_h5,
            _ => self.md_heading_h6,
        }
    }

    /// Resolve every semantic slot to the color depth selected by the
    /// process-local terminal profile. Components keep consuming slots and
    /// never need to know how a terminal represents colors.
    pub fn for_color_level(&self, level: ColorLevel) -> Self {
        let mut theme = self.clone();
        macro_rules! quantize {
            ($($field:ident),* $(,)?) => {
                $(theme.$field = quantize_color(theme.$field, level);)*
            };
        }
        theme_slots!(quantize);
        theme
    }
}

fn quantize_color(color: Color, level: ColorLevel) -> Color {
    if matches!(color, Color::Reset) {
        return Color::Reset;
    }
    match level {
        ColorLevel::TrueColor => color,
        ColorLevel::TerminalDefault => match color {
            Color::Black
            | Color::Red
            | Color::Green
            | Color::Yellow
            | Color::Blue
            | Color::Magenta
            | Color::Cyan
            | Color::Gray
            | Color::DarkGray
            | Color::LightRed
            | Color::LightGreen
            | Color::LightYellow
            | Color::LightBlue
            | Color::LightMagenta
            | Color::LightCyan
            | Color::White => color,
            Color::Indexed(index) if index < 16 => Color::Indexed(index),
            _ => Color::Reset,
        },
        ColorLevel::Ansi16 => Color::Indexed(nearest_ansi16(rgb(color))),
        ColorLevel::Ansi256 => Color::Indexed(nearest_ansi256(rgb(color))),
    }
}

fn rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(index) => ansi256_rgb(index),
        Color::Black => (0, 0, 0),
        Color::Red => (205, 0, 0),
        Color::Green => (0, 205, 0),
        Color::Yellow => (205, 205, 0),
        Color::Blue => (0, 0, 238),
        Color::Magenta => (205, 0, 205),
        Color::Cyan => (0, 205, 205),
        Color::Gray => (229, 229, 229),
        Color::DarkGray => (127, 127, 127),
        Color::LightRed => (255, 0, 0),
        Color::LightGreen => (0, 255, 0),
        Color::LightYellow => (255, 255, 0),
        Color::LightBlue => (92, 92, 255),
        Color::LightMagenta => (255, 0, 255),
        Color::LightCyan => (0, 255, 255),
        Color::White => (255, 255, 255),
        Color::Reset => (128, 128, 128),
    }
}

fn ansi256_rgb(index: u8) -> (u8, u8, u8) {
    match index {
        0..=15 => [
            (0, 0, 0),
            (205, 0, 0),
            (0, 205, 0),
            (205, 205, 0),
            (0, 0, 238),
            (205, 0, 205),
            (0, 205, 205),
            (229, 229, 229),
            (127, 127, 127),
            (255, 0, 0),
            (0, 255, 0),
            (255, 255, 0),
            (92, 92, 255),
            (255, 0, 255),
            (0, 255, 255),
            (255, 255, 255),
        ][index as usize],
        16..=231 => {
            let value = index - 16;
            let r = value / 36;
            let g = (value % 36) / 6;
            let b = value % 6;
            let scale = |channel: u8| if channel == 0 { 0 } else { 55 + channel * 40 };
            (scale(r), scale(g), scale(b))
        }
        232..=255 => {
            let value = 8 + (index - 232) * 10;
            (value, value, value)
        }
    }
}

fn nearest_ansi16((r, g, b): (u8, u8, u8)) -> u8 {
    nearest((r, g, b), (0..16).map(ansi256_rgb))
}

fn nearest_ansi256((r, g, b): (u8, u8, u8)) -> u8 {
    nearest((r, g, b), (0..=255).map(ansi256_rgb))
}

fn nearest<I>(target: (u8, u8, u8), colors: I) -> u8
where
    I: Iterator<Item = (u8, u8, u8)>,
{
    colors
        .enumerate()
        .min_by_key(|(_, (r, g, b))| {
            let dr = i32::from(*r) - i32::from(target.0);
            let dg = i32::from(*g) - i32::from(target.1);
            let db = i32::from(*b) - i32::from(target.2);
            dr * dr + dg * dg + db * db
        })
        .map_or(0, |(index, _)| index as u8)
}

#[cfg(test)]
mod slot_tests {
    use super::*;

    #[test]
    fn slot_count_matches_names() {
        assert_eq!(Theme::SLOT_NAMES.len(), Theme::SLOT_COUNT);
        assert_eq!(Theme::SLOT_COUNT, 96);
    }

    #[test]
    fn slot_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for name in Theme::SLOT_NAMES {
            assert!(seen.insert(*name), "duplicate slot {name}");
        }
    }

    #[test]
    fn color_levels_quantize_every_semantic_slot() {
        let mut theme = Theme::dark();
        theme.accent = Color::Rgb(12, 130, 240);

        assert_eq!(
            theme.for_color_level(ColorLevel::TrueColor).accent,
            Color::Rgb(12, 130, 240)
        );
        assert!(matches!(
            theme.for_color_level(ColorLevel::Ansi256).accent,
            Color::Indexed(_)
        ));
        assert!(matches!(
            theme.for_color_level(ColorLevel::Ansi16).accent,
            Color::Indexed(index) if index < 16
        ));
        assert_eq!(
            theme.for_color_level(ColorLevel::TerminalDefault).accent,
            Color::Reset
        );
    }
}
