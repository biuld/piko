//! Overlay panel chrome (surface geometry + focus session contract).
//!
//! Stack policy, product kinds (palette, host prompt, …), and Escape routing
//! stay in the app. This module paints backdrop + elevated panels and documents
//! modal focus open/close.

mod envelope;
mod focus;
mod surface;

pub use envelope::{OverlayEnvelope, overlay_envelope};
pub use focus::OverlayFocusSession;
pub use surface::{OverlayPanelSpec, OverlayPanelStyle, render_overlay_layer};
