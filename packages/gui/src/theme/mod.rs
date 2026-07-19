//! Piko visual theme for the GPUI Workbench.
//!
//! Submodules own one concern each; numbers and colors live here, not in islands:
//!
//! | Module | Owns |
//! |---|---|
//! | [`metrics`] | Density + type-scale numbers (spacing, sizes, layout constants) |
//! | [`tokens`] | Semantic color palette |
//! | [`typography`] | Applying the type scale (`TextRole`, markdown Body) |
//! | [`icons`] | Vendored icon set sized against the type scale |
//! | [`surfaces`] | Island surface helpers |
//! | [`apply`] | Mapping tokens onto GPUI Component `Theme` |

mod apply;
mod icons;
mod metrics;
mod surfaces;
mod tokens;
mod typography;

pub use apply::apply_piko_dark_theme;
pub use icons::{
    IconSize, PanelSide, PikoIcon, disclosure, icon, panel_toggle_icon, placeholder_icon,
    rotating_gear, row_leading,
};
pub use metrics::metrics;
pub use surfaces::island;
pub use tokens::{PikoTokens, RoleAccent, tokens};
pub use typography::{TextRole, body_markdown, label_text, text};
