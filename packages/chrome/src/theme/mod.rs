//! Visual theme for Islands chrome (tokens, density, type roles, icons).
//!
//! Submodules own one concern each; numbers and colors live here, not in islands:
//!
//! | Module | Owns |
//! |---|---|
//! | [`metrics`] | Density + type-scale numbers (spacing, sizes, layout constants) |
//! | [`tokens`] | Semantic color palettes + `ThemeSnapshot` handle |
//! | [`typography`] | Applying the type scale (`TextRole`, markdown Body) |
//! | [`icons`] | Vendored icon set sized against the type scale |
//! | [`surfaces`] | Island surface helpers |
//! | [`apply`] | Mapping tokens onto GPUI Component `Theme` |
//!
//! Active palette follows GPUI Component's application-global Theme
//! (`set_chrome_palette` / `apply_chrome_theme`). Domain role colors stay in
//! the consuming app. Context-free helpers read a UI-thread-local mirror;
//! per-window palette variation is not supported by the current GPUI Component
//! theme API.
//!
//! Icon SVG bytes live in `packages/chrome/assets/icons/` and are served by
//! [`crate::ChromeAssets`] (`icons/*.svg` paths match [`ChromeIcon`]).

mod apply;
mod icons;
mod metrics;
mod surfaces;
mod tokens;
mod typography;

pub use apply::{apply_chrome_dark_theme, apply_chrome_theme};
pub use icons::{
    ChromeIcon, IconSize, PanelSide, disclosure, icon, panel_toggle_icon, placeholder_icon,
    rotating_gear, row_leading,
};
pub use metrics::metrics;
pub use surfaces::island;
pub use tokens::{
    ChromePalette, ChromeTokens, RoleAccent, ThemeSnapshot, chrome_palette, set_chrome_palette,
    theme_snapshot, tokens, tokens_from,
};
pub use typography::{TextRole, body_markdown, label_text, text};
