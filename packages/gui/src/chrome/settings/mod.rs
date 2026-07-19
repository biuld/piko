//! Settings Primary Surface — own TitleBar, nav body, no StatusBar (v1).

mod body;
mod frame;
mod nav;
pub mod sections;
mod title_bar;
mod widgets;

pub use body::render_body;
pub use frame::mount_frame;
pub use nav::render_nav;
pub use title_bar::render_title_bar;
