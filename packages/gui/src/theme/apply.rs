//! Apply piko tokens onto GPUI Component `Theme`.

use gpui::{App, hsla};
use gpui_component::theme::{Theme, ThemeColor, ThemeMode};

use super::tokens::PikoTokens;

/// Force dark mode and overlay key ThemeColor fields with piko tokens.
pub fn apply_piko_dark_theme(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    let t = PikoTokens::dark();
    let theme = Theme::global_mut(cx);
    overlay_piko_colors(&mut theme.colors, t);
    theme.mode = ThemeMode::Dark;
}

fn overlay_piko_colors(c: &mut ThemeColor, t: PikoTokens) {
    let h = PikoTokens::hsla;
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
    c.overlay = hsla(0.0, 0.0, 0.0, 0.45);
    c.window_border = h(t.border);
    c.red = h(t.danger);
    c.green = h(t.success);
    c.blue = h(t.accent);
    c.yellow = h(t.warning);
    c.cyan = h(t.info);
    c.magenta = h(t.thinking);
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

#[cfg(test)]
mod tests {
    use super::super::tokens::PikoTokens;

    #[test]
    fn dark_tokens_are_stable() {
        let t = PikoTokens::dark();
        assert_eq!(t.canvas, 0x090909);
        assert_eq!(t.fg, 0xe0e1e4);
        assert_eq!(t.ring, 0x2a7deb);
    }
}
