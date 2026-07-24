//! Assemble the Workbench Archipelago frame (TitleBar + body + StatusBar).

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::projections::derive_status_bar;
use crate::shell::workbench::{render_status_bar, render_title_bar};
use crate::theme::metrics;

use super::IslandId;

pub fn mount_frame(
    root: Stateful<Div>,
    app: &DesktopApp,
    window: &mut Window,
    cx: &mut Context<DesktopApp>,
) -> Stateful<Div> {
    let live = app.bridge_state().is_live();
    let show_session = app.layout.is_docked_visible(IslandId::Sessions, live);
    let show_right = app.layout.any_right_column_docked(live);
    let status_vm = derive_status_bar(app.bridge_state(), show_session);
    let allow_motion = app.ux_prefs.allow_motion();
    let entity = cx.entity().downgrade();
    let m = metrics();

    root.child(render_title_bar(
        show_session,
        show_right,
        false,
        app.notifications.is_open(),
        app.notifications.has_unread(),
        entity,
    ))
    .child(
        div()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .p(m.island_gutter)
            .child(app.render_workbench_row(window, cx)),
    )
    .child(render_status_bar(&status_vm, allow_motion))
}
