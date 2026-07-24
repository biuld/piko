//! Appearance settings — `[gui]` presentation prefs.

use gpui::*;
use piko_chrome::theme::ChromePalette;

use crate::app::desktop_app::DesktopApp;
use crate::theme::metrics;

use super::super::widgets::{bool_switch, section_lede, setting_group, setting_row};

pub fn render_appearance(app: &DesktopApp, entity: WeakEntity<DesktopApp>) -> impl IntoElement {
    let m = metrics();
    div()
        .w_full()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(m.space_md)
        .child(section_lede(crate::t!("settings.appearance.lede")))
        .child(setting_group(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(m.space_md)
                .child(setting_row(
                    "settings-appearance-light-theme",
                    crate::t!("settings.appearance.light_theme.label"),
                    Some(crate::t!("settings.appearance.light_theme.detail").into()),
                    {
                        let checked = app.ux_prefs.chrome_palette == ChromePalette::Light;
                        let entity = entity.clone();
                        bool_switch(
                            "settings-appearance-light-theme-switch",
                            checked,
                            move |checked, window, cx| {
                                if let Some(view) = entity.upgrade() {
                                    view.update(cx, |this, cx| {
                                        this.settings_set_chrome_palette(checked, window, cx);
                                    });
                                }
                            },
                        )
                    },
                ))
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
                )),
        ))
}
