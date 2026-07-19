//! Assemble the Settings Primary Surface frame (TitleBar + body, no StatusBar v1).

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::shell::settings::{body_slots, render_title_bar};

pub fn mount_frame(
    root: Stateful<Div>,
    entity: WeakEntity<DesktopApp>,
    nav: impl IntoElement,
    panel: impl IntoElement,
) -> Stateful<Div> {
    root.child(render_title_bar(entity))
        .child(body_slots(nav, panel))
}
