//! Appearance settings — `[gui]` presentation prefs.

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::theme::metrics;

use super::super::widgets::{bool_switch, section_lede, setting_row};

pub fn render_appearance(app: &DesktopApp, entity: WeakEntity<DesktopApp>) -> impl IntoElement {
    let m = metrics();
    div()
        .flex()
        .flex_col()
        .gap(m.space_lg)
        .child(section_lede(crate::t!("settings.appearance.lede")))
        .child(setting_row(
            "settings-appearance-reduced-motion",
            crate::t!("settings.appearance.reduced_motion.label"),
            Some(crate::t!("settings.appearance.reduced_motion.detail").into()),
            {
                let checked = app.ux_prefs.prefer_reduced_motion;
                let entity = entity.clone();
                bool_switch(
                    "settings-appearance-reduced-motion-switch",
                    checked,
                    move |checked, _, cx| {
                        if let Some(view) = entity.upgrade() {
                            view.update(cx, |this, cx| {
                                this.settings_set_reduced_motion(checked, cx);
                            });
                        }
                    },
                )
            },
        ))
}
