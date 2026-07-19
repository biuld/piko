//! Context & Reliability — compaction and retry host settings.

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::chrome::settings::widgets::{bool_switch, section_lede, setting_row};
use crate::theme::metrics;

pub fn render_context_reliability(
    app: &DesktopApp,
    entity: WeakEntity<DesktopApp>,
) -> impl IntoElement {
    let m = metrics();
    let runtime = &app.host_runtime;
    div()
        .flex()
        .flex_col()
        .gap(m.space_lg)
        .child(section_lede(crate::t!("settings.context.lede")))
        .child(setting_row(
            "settings-context-compaction",
            crate::t!("settings.context.compaction.label"),
            Some(crate::t!("settings.context.compaction.detail").into()),
            {
                let checked = runtime.compaction.enabled;
                let entity = entity.clone();
                bool_switch(
                    "settings-context-compaction-switch",
                    checked,
                    move |checked, _, cx| {
                        if let Some(view) = entity.upgrade() {
                            view.update(cx, |this, cx| {
                                this.settings_set_compaction_enabled(checked, cx);
                            });
                        }
                    },
                )
            },
        ))
        .child(setting_row(
            "settings-context-retry",
            crate::t!("settings.context.retry.label"),
            Some(crate::t!("settings.context.retry.detail").into()),
            {
                let checked = runtime.retry.enabled;
                let entity = entity.clone();
                bool_switch(
                    "settings-context-retry-switch",
                    checked,
                    move |checked, _, cx| {
                        if let Some(view) = entity.upgrade() {
                            view.update(cx, |this, cx| {
                                this.settings_set_retry_enabled(checked, cx);
                            });
                        }
                    },
                )
            },
        ))
}
