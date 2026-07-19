//! Chrome OverlayHost: stack, surface chrome (no product Transient bodies).
//!
//! Command Palette lives under `crate::features::palette`; HostPrompt bodies
//! under `crate::features::prompts`.

mod host;
mod kinds;
mod prompt_front;
mod surface;

pub use host::{EscapeOutcome, OverlayHost};
pub use kinds::{LocalConfirmKind, OverlayLayer, TransientKind};
pub use prompt_front::{PromptFront, PromptKind};
pub use surface::{OverlayPanelSpec, OverlayPanelStyle, render_overlay_layer};
