//! Workbench TitleBar — dock toggles, brand, Settings gear (toggle).

use gpui::*;
use gpui_component::Sizable;
use gpui_component::TitleBar;
use gpui_component::button::{Button, ButtonVariants};
use piko_chrome::components::notification::{NotificationBellSpec, render_notification_bell};

use crate::app::desktop_app::{
    DesktopApp, OpenSettings, ToggleNotificationCenter, ToggleRightColumn, ToggleSessions,
};
use crate::theme::{
    ChromeIcon, ChromeTokens, IconSize, PanelSide, icon, label_text, metrics, panel_toggle_icon,
    tokens,
};

pub fn render_title_bar(
    sessions_docked: bool,
    right_docked: bool,
    settings_active: bool,
    notifications_open: bool,
    notifications_unread: bool,
    entity: WeakEntity<DesktopApp>,
) -> impl IntoElement {
    let m = metrics();
    let t = tokens();

    let sessions_toggle = {
        let entity = entity.clone();
        panel_toggle(
            "title-toggle-sessions",
            PanelSide::Left,
            sessions_docked,
            crate::t!("chrome.toggle.sessions"),
            move |_, window, cx| {
                if let Some(view) = entity.upgrade() {
                    view.update(cx, |this, cx| {
                        this.action_toggle_sessions(&ToggleSessions, window, cx);
                    });
                }
            },
        )
    };

    let right_toggle = {
        let entity = entity.clone();
        panel_toggle(
            "title-toggle-right-column",
            PanelSide::Right,
            right_docked,
            crate::t!("chrome.toggle.right_column"),
            move |_, window, cx| {
                if let Some(view) = entity.upgrade() {
                    view.update(cx, |this, cx| {
                        this.action_toggle_right_column(&ToggleRightColumn, window, cx);
                    });
                }
            },
        )
    };

    TitleBar::new().h(m.title_bar_height).child(
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .gap(m.space_xs)
                    .child(sessions_toggle)
                    .child(right_toggle),
            )
            .child(
                div()
                    .absolute()
                    .right(m.island_gutter)
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .gap(m.space_xs)
                    .child(notification_bell(
                        notifications_open,
                        notifications_unread,
                        entity.clone(),
                    ))
                    .child(settings_gear(settings_active, entity)),
            )
            .child(
                label_text(false)
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.fg_rgba())
                    .child("piko"),
            ),
    )
}

pub(crate) fn notification_bell(
    active: bool,
    unread: bool,
    entity: WeakEntity<DesktopApp>,
) -> AnyElement {
    render_notification_bell(
        NotificationBellSpec::new(
            "title-toggle-notifications",
            active,
            unread,
            crate::t!("chrome.action.notifications"),
            crate::t!("chrome.action.notifications.close"),
        ),
        move |_, window, cx| {
            if let Some(view) = entity.upgrade() {
                view.update(cx, |this, cx| {
                    this.action_toggle_notification_center(&ToggleNotificationCenter, window, cx);
                });
            }
        },
    )
}

pub(crate) fn settings_gear(active: bool, entity: WeakEntity<DesktopApp>) -> Button {
    let t = tokens();
    let color = if active {
        ChromeTokens::hsla(t.fg)
    } else {
        ChromeTokens::hsla(t.muted_fg)
    };
    let tooltip = if active {
        crate::t!("chrome.action.settings.close")
    } else {
        crate::t!("chrome.action.settings")
    };
    Button::new("title-toggle-settings")
        .icon(icon(ChromeIcon::Settings, IconSize::Label, color))
        .tooltip(tooltip)
        .ghost()
        .small()
        .compact()
        .on_click(move |_, window, cx| {
            if let Some(view) = entity.upgrade() {
                view.update(cx, |this, cx| {
                    this.action_open_settings(&OpenSettings, window, cx);
                });
            }
        })
}

/// Ghost icon button; open/closed is carried only by hollow vs hatched SVG.
fn panel_toggle(
    id: impl Into<ElementId>,
    side: PanelSide,
    docked: bool,
    tooltip: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    let color = ChromeTokens::hsla(tokens().muted_fg);
    Button::new(id)
        .icon(panel_toggle_icon(side, docked, color))
        .tooltip(tooltip)
        .ghost()
        .small()
        .compact()
        .on_click(on_click)
}
