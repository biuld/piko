//! Settings Archipelago shell — TitleBar + body slots (no product forms).

mod body_slots;
mod frame;
mod title_bar;

pub use body_slots::body_slots;
pub use frame::mount_frame;
pub use title_bar::render_title_bar;
