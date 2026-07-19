//! Agent & Tools — sandbox and active-tools host settings.

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::chrome::settings::widgets::{bool_switch, section_lede, setting_row};
use crate::theme::metrics;

pub fn render_agent_tools(app: &DesktopApp, entity: WeakEntity<DesktopApp>) -> impl IntoElement {
    let m = metrics();
    let runtime = &app.host_runtime;
    div()
        .flex()
        .flex_col()
        .gap(m.space_lg)
        .child(section_lede(crate::t!("settings.agent_tools.lede")))
        .child(setting_row(
            "settings-agent-tools-sandbox",
            crate::t!("settings.agent_tools.sandbox.label"),
            Some(crate::t!("settings.agent_tools.sandbox.detail").into()),
            {
                let checked = runtime.sandbox.enabled;
                let entity = entity.clone();
                bool_switch(
                    "settings-agent-tools-sandbox-switch",
                    checked,
                    move |checked, _, cx| {
                        if let Some(view) = entity.upgrade() {
                            view.update(cx, |this, cx| {
                                this.settings_set_sandbox_enabled(checked, cx);
                            });
                        }
                    },
                )
            },
        ))
        .child(setting_row(
            "settings-agent-tools-restrict",
            crate::t!("settings.agent_tools.tools_restricted.label"),
            Some(crate::t!("settings.agent_tools.tools_restricted.detail").into()),
            {
                let checked = runtime.tools_restricted();
                let entity = entity.clone();
                bool_switch(
                    "settings-agent-tools-restrict-switch",
                    checked,
                    move |checked, _, cx| {
                        if let Some(view) = entity.upgrade() {
                            view.update(cx, |this, cx| {
                                this.settings_set_tools_restricted(checked, cx);
                            });
                        }
                    },
                )
            },
        ))
        .child(section_lede(crate::t!("settings.agent_tools.mcp_hint")))
}
