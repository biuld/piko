//! Chrome semantic theme tokens and palette snapshots.
//!
//! **Chrome core** owns canvas/surface/fg/border/ring/accent and status colors.
//! Product domain role colors (chat authors, tool classes) live in the consuming
//! app — not on [`ChromeTokens`].

use std::cell::Cell;

use gpui::{Hsla, Rgba, rgb};

/// Named palette variants for multi-pane chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChromePalette {
    #[default]
    Dark,
    Light,
}

impl ChromePalette {
    const fn as_u8(self) -> u8 {
        match self {
            Self::Dark => 0,
            Self::Light => 1,
        }
    }

    const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Light,
            _ => Self::Dark,
        }
    }

    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }
}

/// Immutable theme handle: palette identity + resolved token table.
#[derive(Debug, Clone, Copy)]
pub struct ThemeSnapshot {
    pub palette: ChromePalette,
    pub tokens: ChromeTokens,
}

impl ThemeSnapshot {
    pub const fn for_palette(palette: ChromePalette) -> Self {
        Self {
            palette,
            tokens: ChromeTokens::for_palette(palette),
        }
    }

    pub const fn dark() -> Self {
        Self::for_palette(ChromePalette::Dark)
    }

    pub const fn light() -> Self {
        Self::for_palette(ChromePalette::Light)
    }
}

/// Semantic palette for multi-pane chrome (editor-style density).
///
/// Domain role accents (user/assistant/tool/…) are **not** fields here.
#[derive(Debug, Clone, Copy)]
pub struct ChromeTokens {
    pub canvas: u32,
    pub surface: u32,
    pub elevated: u32,
    pub chrome: u32,
    pub fg: u32,
    pub muted_fg: u32,
    pub border: u32,
    pub ring: u32,
    pub accent: u32,
    pub success: u32,
    pub warning: u32,
    pub danger: u32,
    pub info: u32,
}

impl ChromeTokens {
    pub const fn for_palette(palette: ChromePalette) -> Self {
        match palette {
            ChromePalette::Dark => Self::dark(),
            ChromePalette::Light => Self::light(),
        }
    }

    pub const fn dark() -> Self {
        Self {
            canvas: 0x090909,
            surface: 0x18191b,
            elevated: 0x252629,
            chrome: 0x090909,
            fg: 0xe0e1e4,
            muted_fg: 0x898e94,
            border: 0x3e4147,
            ring: 0x2a7deb,
            accent: 0x4b8dec,
            success: 0x169068,
            warning: 0xb07203,
            danger: 0xe1465e,
            info: 0x4b8dec,
        }
    }

    /// Fleet Light semantic mapping.
    ///
    /// The source theme has alpha-bearing border and selection tokens. Chrome's
    /// compact token table stores opaque RGB, so `border` is the source border
    /// composited over the white island surface. Interactive hover surfaces use
    /// Fleet's elevated surface color.
    pub const fn light() -> Self {
        Self {
            canvas: 0xeeeff0,
            surface: 0xffffff,
            elevated: 0xf8f8f9,
            chrome: 0xeeeff0,
            fg: 0x090909,
            muted_fg: 0x6e747b,
            border: 0xd1d1d2,
            ring: 0x2a7deb,
            accent: 0x1d61ba,
            success: 0x169068,
            warning: 0xb07203,
            danger: 0xe1465e,
            info: 0x4b8dec,
        }
    }

    pub fn rgba(hex: u32) -> Rgba {
        rgb(hex)
    }

    pub fn hsla(hex: u32) -> Hsla {
        Hsla::from(rgb(hex))
    }

    pub fn canvas_rgba(self) -> Rgba {
        Self::rgba(self.canvas)
    }

    pub fn surface_rgba(self) -> Rgba {
        Self::rgba(self.surface)
    }

    pub fn elevated_rgba(self) -> Rgba {
        Self::rgba(self.elevated)
    }

    pub fn chrome_rgba(self) -> Rgba {
        Self::rgba(self.chrome)
    }

    pub fn fg_rgba(self) -> Rgba {
        Self::rgba(self.fg)
    }

    pub fn muted_fg_rgba(self) -> Rgba {
        Self::rgba(self.muted_fg)
    }

    pub fn border_rgba(self) -> Rgba {
        Self::rgba(self.border)
    }

    pub fn ring_rgba(self) -> Rgba {
        Self::rgba(self.ring)
    }

    pub fn accent_rgba(self) -> Rgba {
        Self::rgba(self.accent)
    }

    pub fn role_accent(self, role: RoleAccent) -> Rgba {
        Self::rgba(self.role_hex(role))
    }

    pub fn role_accent_hsla(self, role: RoleAccent) -> Hsla {
        Self::hsla(self.role_hex(role))
    }

    fn role_hex(self, role: RoleAccent) -> u32 {
        match role {
            RoleAccent::Success => self.success,
            RoleAccent::Warning => self.warning,
            RoleAccent::Danger => self.danger,
            RoleAccent::Info => self.info,
            RoleAccent::Accent => self.accent,
        }
    }
}

/// Chrome-core semantic accents (status + primary accent).
///
/// Product domain roles (user/assistant/thinking/tool/system) live in the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleAccent {
    Success,
    Warning,
    Danger,
    Info,
    Accent,
}

// Application UI-thread palette mirror (set via `set_chrome_palette` / theme apply).
//
// GPUI Component 0.5 exposes one application-global Theme, so every window in
// an application intentionally shares this palette. GPUI applications render
// on their UI thread; a thread-local mirror lets context-free paint helpers
// follow that Theme without leaking palette state across application threads
// or parallel tests. Per-window themes are not represented by this API.
thread_local! {
    static CURRENT_PALETTE: Cell<u8> = const { Cell::new(0) };
}

/// Active palette identity.
pub fn chrome_palette() -> ChromePalette {
    CURRENT_PALETTE.with(|palette| ChromePalette::from_u8(palette.get()))
}

/// Install the application-global palette used by [`tokens`] / [`theme_snapshot`].
pub fn set_chrome_palette(palette: ChromePalette) {
    CURRENT_PALETTE.with(|current| current.set(palette.as_u8()));
}

/// Immutable snapshot of the active chrome theme.
pub fn theme_snapshot() -> ThemeSnapshot {
    ThemeSnapshot::for_palette(chrome_palette())
}

/// Token table for the active application-global snapshot.
pub fn tokens() -> ChromeTokens {
    theme_snapshot().tokens
}

/// Token table for an explicit snapshot handle (tests / multi-theme callers).
pub fn tokens_from(snapshot: &ThemeSnapshot) -> ChromeTokens {
    snapshot.tokens
}

#[cfg(test)]
mod tests {
    use super::{
        ChromePalette, ChromeTokens, RoleAccent, ThemeSnapshot, chrome_palette, set_chrome_palette,
        theme_snapshot, tokens, tokens_from,
    };

    #[test]
    fn dark_and_light_differ_on_surface_and_fg() {
        let dark = ChromeTokens::dark();
        let light = ChromeTokens::light();
        assert_ne!(dark.canvas, light.canvas);
        assert_ne!(dark.surface, light.surface);
        assert_ne!(dark.fg, light.fg);
        assert_ne!(dark.elevated, light.elevated);
    }

    #[test]
    fn snapshot_for_palette_matches_token_tables() {
        let dark = ThemeSnapshot::dark();
        assert_eq!(dark.palette, ChromePalette::Dark);
        assert_eq!(dark.tokens.canvas, ChromeTokens::dark().canvas);

        let light = ThemeSnapshot::light();
        assert_eq!(light.palette, ChromePalette::Light);
        assert_eq!(light.tokens.fg, ChromeTokens::light().fg);
    }

    #[test]
    fn tokens_from_explicit_snapshot_ignores_thread_palette() {
        let prev = chrome_palette();
        set_chrome_palette(ChromePalette::Dark);
        let light = ThemeSnapshot::light();
        assert_eq!(tokens_from(&light).canvas, ChromeTokens::light().canvas);
        assert_eq!(tokens().canvas, ChromeTokens::dark().canvas);
        set_chrome_palette(prev);
    }

    #[test]
    fn set_palette_updates_thread_snapshot() {
        let prev = chrome_palette();
        set_chrome_palette(ChromePalette::Light);
        assert_eq!(chrome_palette(), ChromePalette::Light);
        assert_eq!(theme_snapshot().tokens.fg, ChromeTokens::light().fg);
        assert_eq!(tokens().muted_fg, ChromeTokens::light().muted_fg);
        set_chrome_palette(ChromePalette::Dark);
        assert_eq!(tokens().canvas, ChromeTokens::dark().canvas);
        set_chrome_palette(prev);
    }

    #[test]
    fn semantic_role_accents_resolve() {
        let t = ChromeTokens::dark();
        assert_eq!(
            t.role_accent(RoleAccent::Accent),
            ChromeTokens::rgba(t.accent)
        );
        assert_eq!(
            t.role_accent(RoleAccent::Danger),
            ChromeTokens::rgba(t.danger)
        );
        assert_eq!(t.accent_rgba(), ChromeTokens::rgba(t.accent));
    }

    #[test]
    fn palette_is_dark_flag() {
        assert!(ChromePalette::Dark.is_dark());
        assert!(!ChromePalette::Light.is_dark());
    }
}
