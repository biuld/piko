//! Settings left nav — section list inside the nav island.

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::chrome::primary_surface::SettingsSection;
use crate::theme::{RoleAccent, metrics, tokens};

pub fn render_nav(active: SettingsSection, entity: WeakEntity<DesktopApp>) -> impl IntoElement {
    let t = tokens();
    let m = metrics();

    div()
        .id("settings-nav")
        .size_full()
        .flex()
        .flex_col()
        .gap(px(2.))
        .py(m.space_sm)
        .px(m.space_xs)
        .overflow_y_scroll()
        .children(SettingsSection::ALL.into_iter().map(|section| {
            let selected = section == active;
            let entity = entity.clone();
            let label = crate::t!(section.i18n_key());
            let label_color = if selected {
                t.role_accent(RoleAccent::Accent)
            } else {
                t.fg_rgba()
            };
            div()
                .id(nav_row_id(section))
                .h(px(32.))
                .px(m.tool_row_inset)
                .rounded_sm()
                .cursor_pointer()
                .flex()
                .items_center()
                .hover(|style| style.bg(t.elevated_rgba()))
                .when(selected, |d| d.bg(t.elevated_rgba()))
                .on_click(move |_, _, cx| {
                    if let Some(view) = entity.upgrade() {
                        view.update(cx, |this, cx| {
                            this.select_settings_section(section, cx);
                        });
                    }
                })
                .child(
                    crate::theme::label_text(selected)
                        .text_color(label_color)
                        .child(label),
                )
        }))
}

fn nav_row_id(section: SettingsSection) -> SharedString {
    match section {
        SettingsSection::General => "settings-nav-general".into(),
        SettingsSection::Account => "settings-nav-account".into(),
        SettingsSection::AgentTools => "settings-nav-agent-tools".into(),
        SettingsSection::ContextReliability => "settings-nav-context-reliability".into(),
        SettingsSection::Appearance => "settings-nav-appearance".into(),
        SettingsSection::Keyboard => "settings-nav-keyboard".into(),
        SettingsSection::Advanced => "settings-nav-advanced".into(),
    }
}
