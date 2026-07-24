//! Assemble the Settings Archipelago frame (TitleBar + body, no StatusBar v1).

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::shell::settings::{body_slots, render_title_bar};

pub fn mount_frame<Id>(
    root: Stateful<Div>,
    entity: WeakEntity<DesktopApp>,
    tree: &piko_chrome::runtime::layout::IslandNode<Id>,
    nav_id: Id,
    panel_id: Id,
    nav: impl IntoElement,
    panel: impl IntoElement,
) -> Stateful<Div>
where
    Id: Copy + Eq + std::hash::Hash,
{
    root.child(render_title_bar(entity))
        .child(body_slots(tree, nav_id, panel_id, nav, panel))
}
