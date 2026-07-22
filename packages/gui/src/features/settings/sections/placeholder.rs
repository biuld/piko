//! Placeholder sections deferred beyond Phase 3.

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::theme::metrics;

use super::super::SettingsSection;
use super::super::widgets::{section_lede, setting_group, text_button};

pub fn render_placeholder(_section: SettingsSection) -> impl IntoElement {
    let m = metrics();
    div()
        .w_full()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(m.space_md)
        .child(setting_group(section_lede(crate::t!(
            "settings.placeholder"
        ))))
}

pub fn render_advanced(entity: WeakEntity<DesktopApp>) -> impl IntoElement {
    let m = metrics();
    div()
        .w_full()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(m.space_md)
        .child(section_lede(crate::t!("settings.advanced.lede")))
        .child(setting_group(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(m.space_md)
                .child({
                    let entity = entity.clone();
                    text_button(
                        "settings-advanced-open-config",
                        crate::t!("settings.advanced.open_config"),
                        move |_, _, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, _| {
                                    this.settings_open_config_dir();
                                });
                            }
                        },
                    )
                })
                .child(section_lede(crate::t!("settings.advanced.deferred"))),
        ))
}
