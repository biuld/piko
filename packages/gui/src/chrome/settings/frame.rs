//! Assemble the Settings Primary Surface frame (TitleBar + body, no StatusBar v1).

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::chrome::primary_surface::SettingsSection;
use crate::chrome::settings::{render_body, render_title_bar};

pub fn mount_frame(
    root: Stateful<Div>,
    app: &DesktopApp,
    section: SettingsSection,
    cx: &mut Context<DesktopApp>,
) -> Stateful<Div> {
    let entity = cx.entity().downgrade();
    root.child(render_title_bar(entity.clone()))
        .child(render_body(section, app, entity))
}
