//! Apply kit tokens onto GPUI Component `Theme`.

use gpui::{App, hsla};
use gpui_component::theme::{Theme, ThemeColor, ThemeMode};

use super::tokens::{ChromePalette, ChromeTokens, set_chrome_palette};

/// Apply a chrome palette: UI-thread snapshot + GPUI component theme overlay.
pub fn apply_chrome_theme(cx: &mut App, palette: ChromePalette) {
    set_chrome_palette(palette);
    let mode = match palette {
        ChromePalette::Dark => ThemeMode::Dark,
        ChromePalette::Light => ThemeMode::Light,
    };
    Theme::change(mode, None, cx);
    let t = ChromeTokens::for_palette(palette);
    let theme = Theme::global_mut(cx);
    overlay_chrome_colors(&mut theme.colors, t);
    theme.mode = mode;
    // Overflow-gated thumbs; follow OS auto-hide vs hover preference.
    Theme::sync_scrollbar_appearance(cx);
}

/// Force dark mode and overlay key ThemeColor fields with kit tokens.
///
/// Prefer [`apply_chrome_theme`] when the caller already knows the palette.
pub fn apply_chrome_dark_theme(cx: &mut App) {
    apply_chrome_theme(cx, ChromePalette::Dark);
}

fn overlay_chrome_colors(c: &mut ThemeColor, t: ChromeTokens) {
    let h = ChromeTokens::hsla;
    c.background = h(t.canvas);
    c.foreground = h(t.fg);
    c.popover = h(t.elevated);
    c.popover_foreground = h(t.fg);
    c.group_box = h(t.elevated);
    c.group_box_foreground = h(t.fg);
    c.primary = h(t.accent);
    c.primary_foreground = h(t.fg);
    c.primary_hover = h(t.accent);
    c.primary_active = h(t.accent);
    c.secondary = h(t.elevated);
    c.secondary_foreground = h(t.fg);
    c.secondary_hover = h(t.border);
    c.secondary_active = h(t.border);
    c.muted = h(t.elevated);
    c.muted_foreground = h(t.muted_fg);
    c.accent = h(t.border);
    c.accent_foreground = h(t.fg);
    c.border = hsla(0.0, 0.0, 0.0, 0.0);
    c.input = h(t.border);
    c.ring = h(t.ring);
    c.drag_border = h(t.ring);
    c.danger = h(t.danger);
    c.danger_foreground = h(t.fg);
    c.danger_hover = h(t.danger);
    c.danger_active = h(t.danger);
    c.warning = h(t.warning);
    c.warning_foreground = h(t.fg);
    c.warning_hover = h(t.warning);
    c.warning_active = h(t.warning);
    c.success = h(t.success);
    c.success_foreground = h(t.fg);
    c.success_hover = h(t.success);
    c.success_active = h(t.success);
    c.info = h(t.info);
    c.info_foreground = h(t.fg);
    c.info_hover = h(t.info);
    c.info_active = h(t.info);
    c.sidebar = h(t.surface);
    c.sidebar_foreground = h(t.fg);
    c.sidebar_border = h(t.border);
    c.sidebar_accent = h(t.border);
    c.sidebar_accent_foreground = h(t.fg);
    c.sidebar_primary = h(t.accent);
    c.sidebar_primary_foreground = h(t.fg);
    c.title_bar = h(t.chrome);
    c.title_bar_border = h(t.chrome);
    c.list = h(t.surface);
    c.list_even = h(t.surface);
    c.list_active = h(t.elevated);
    c.list_active_border = h(t.elevated);
    c.list_hover = h(t.elevated);
    c.list_head = h(t.surface);
    c.scrollbar = h(t.surface);
    c.scrollbar_thumb = h(t.border);
    c.scrollbar_thumb_hover = h(t.muted_fg);
    c.selection = hsla(0.59, 0.62, 0.35, 0.55);
    c.link = h(t.accent);
    c.link_hover = h(t.info);
    c.link_active = h(t.accent);
    c.overlay = hsla(0.0, 0.0, 0.0, chrome_overlay_alpha(t));
    c.window_border = h(t.border);
    c.red = h(t.danger);
    c.green = h(t.success);
    c.blue = h(t.accent);
    c.yellow = h(t.warning);
    c.cyan = h(t.info);
    c.magenta = h(t.muted_fg);
    c.skeleton = h(t.border);
    c.tab_bar = h(t.chrome);
    c.tab = h(t.chrome);
    c.tab_active = h(t.surface);
    c.tab_active_foreground = h(t.fg);
    c.tab_foreground = h(t.muted_fg);
    c.tab_bar_segmented = h(t.elevated);
    c.tiles = h(t.surface);
    c.caret = h(t.fg);
}

fn chrome_overlay_alpha(t: ChromeTokens) -> f32 {
    // Dark canvas → heavier dim; light canvas → lighter scrim.
    if t.canvas < 0x808080 { 0.45 } else { 0.35 }
}

#[cfg(test)]
mod tests {
    use super::super::tokens::{ChromePalette, ChromeTokens};
    use super::chrome_overlay_alpha;

    #[test]
    fn dark_tokens_are_stable() {
        let t = ChromeTokens::dark();
        assert_eq!(t.canvas, 0x090909);
        assert_eq!(t.fg, 0xe0e1e4);
        assert_eq!(t.ring, 0x2a7deb);
    }

    #[test]
    fn light_tokens_are_stable() {
        let t = ChromeTokens::light();
        assert_eq!(t.canvas, 0xeeeff0);
        assert_eq!(t.chrome, 0xeeeff0);
        assert_eq!(t.surface, 0xffffff);
        assert_eq!(t.elevated, 0xf8f8f9);
        assert_eq!(t.fg, 0x090909);
        assert_eq!(t.muted_fg, 0x6e747b);
        assert_eq!(t.border, 0xd1d1d2);
        assert_eq!(t.ring, 0x2a7deb);
        assert_eq!(t.accent, 0x1d61ba);
        assert_eq!(t.success, 0x169068);
        assert_eq!(t.warning, 0xb07203);
        assert_eq!(t.danger, 0xe1465e);
        assert_eq!(t.info, 0x4b8dec);
    }

    #[test]
    fn for_palette_selects_tables() {
        assert_eq!(
            ChromeTokens::for_palette(ChromePalette::Dark).canvas,
            ChromeTokens::dark().canvas
        );
        assert_eq!(
            ChromeTokens::for_palette(ChromePalette::Light).fg,
            ChromeTokens::light().fg
        );
    }

    #[test]
    fn overlay_alpha_heavier_on_dark_canvas() {
        assert!(
            chrome_overlay_alpha(ChromeTokens::dark())
                > chrome_overlay_alpha(ChromeTokens::light())
        );
    }
}
