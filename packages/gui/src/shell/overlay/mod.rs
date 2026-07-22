//! Chrome OverlayHost: stack + product kinds (no Transient/Prompt bodies).
//!
//! Panel surface geometry comes from [`piko_chrome::overlay`]. Focus open/close
//! uses chrome [`OverlayFocusSession`] on [`OverlayHost`] (E4). Command Palette
//! lives under `crate::features::palette`; HostPrompt bodies under
//! `crate::features::prompts`.

mod host;
mod kinds;
mod prompt_front;

pub use host::{EscapeOutcome, OverlayHost};
pub use kinds::{LocalConfirmKind, OverlayLayer, TransientKind};
pub use piko_chrome::overlay::{
    OverlayFocusSession, OverlayPanelSpec, OverlayPanelStyle, render_overlay_layer,
};
pub use prompt_front::{PromptFront, PromptKind};
