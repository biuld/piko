//! Settings left nav — section list inside the nav island.
//!
//! Keyboard (when the nav island owns focus) is driven by
//! [`piko_chrome::ListKeyboard`] on [`super::SettingsNavIsland`]: ↑/↓ move
//! selection, Enter/Space confirm (selects current and focuses the Panel).
//!
//! Row chrome uses chrome [`ListRowSpec`] / [`render_list`] (roadmap D3).

use gpui::*;
use piko_chrome::{ListClickHandler, ListRowSpec, render_list};

use crate::app::desktop_app::DesktopApp;
use crate::theme::{RoleAccent, tokens};

use super::SettingsSection;

actions!(
    settings_nav,
    [SelectPrevSection, SelectNextSection, ConfirmSection]
);

/// `keyboard_active`: when true, paint a focus ring on the keyboard caret row
/// (nav island owns keyboard focus).
pub fn render_nav(
    active: SettingsSection,
    keyboard_active: bool,
    entity: WeakEntity<DesktopApp>,
) -> impl IntoElement {
    let t = tokens();
    let rows = SettingsSection::ALL.into_iter().map(|section| {
        let selected = section == active;
        let entity = entity.clone();
        let label = crate::t!(section.i18n_key());
        let label_color = if selected {
            t.role_accent(RoleAccent::Accent)
        } else {
            t.fg_rgba()
        };
        let spec = ListRowSpec::new(nav_row_id(section), label)
            .selected(selected)
            .keyboard_focused(selected && keyboard_active)
            .label_color(label_color);
        let on_click: ListClickHandler = Box::new(move |_, _, cx| {
            if let Some(view) = entity.upgrade() {
                view.update(cx, |this, cx| {
                    this.select_settings_section(section, cx);
                });
            }
        });
        (spec, on_click)
    });

    div()
        .id("settings-nav")
        .size_full()
        .overflow_y_scroll()
        .child(render_list(rows))
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
