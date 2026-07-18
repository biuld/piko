//! Reusable surface primitives for the islands layout.

use gpui::*;

use super::{metrics, tokens};

/// A first-class task surface placed on the window canvas.
pub fn island() -> Div {
    let m = metrics();
    let t = tokens();
    div()
        .overflow_hidden()
        .rounded(m.island_radius)
        .bg(t.surface_rgba())
}
