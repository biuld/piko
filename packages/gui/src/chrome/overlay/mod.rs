//! Chrome OverlayHost: stack, surface, and Transient tools (Command Palette).
//!
//! Product center modals render here — not via GPUI Component `open_dialog`.

mod host;
mod kinds;
pub mod palette;
mod surface;

pub use host::{EscapeOutcome, OverlayHost};
pub use kinds::{LocalConfirmKind, OverlayLayer, TransientKind};
pub use palette::{CommandPalette, PaletteConfirm, PaletteSelectNext, PaletteSelectPrev};
pub use surface::{OverlayPanelSpec, OverlayPanelStyle, render_overlay_layer};
