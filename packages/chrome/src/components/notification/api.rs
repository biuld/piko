//! Stateless integration facade for application notification hosts.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::notification::{Notification, NotificationType};
use gpui_component::{Root, Sizable, WindowExt};

use super::{
    NotificationSeverity, render_notification_panel_anchor, render_notification_toast_layer,
};
use crate::theme::{ChromeIcon, ChromeTokens, IconSize, icon, tokens};

pub struct NotificationBellSpec {
    pub id: ElementId,
    pub active: bool,
    pub unread: bool,
    pub open_tooltip: SharedString,
    pub close_tooltip: SharedString,
}

impl NotificationBellSpec {
    pub fn new(
        id: impl Into<ElementId>,
        active: bool,
        unread: bool,
        open_tooltip: impl Into<SharedString>,
        close_tooltip: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            active,
            unread,
            open_tooltip: open_tooltip.into(),
            close_tooltip: close_tooltip.into(),
        }
    }
}

pub struct NotificationToastSpec {
    pub severity: NotificationSeverity,
    pub title: SharedString,
    pub message: SharedString,
    pub autohide: bool,
}

impl NotificationToastSpec {
    pub fn new(
        severity: NotificationSeverity,
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
    ) -> Self {
        Self {
            severity,
            title: title.into(),
            message: message.into(),
            autohide: true,
        }
    }

    pub fn autohide(mut self, autohide: bool) -> Self {
        self.autohide = autohide;
        self
    }
}

/// Render the standard title-bar bell. Product code supplies copy and the
/// action callback but no visual or badge geometry.
pub fn render_notification_bell(
    spec: NotificationBellSpec,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let t = tokens();
    let color = if spec.active {
        ChromeTokens::hsla(t.fg)
    } else {
        ChromeTokens::hsla(t.muted_fg)
    };
    let tooltip = if spec.active {
        spec.close_tooltip
    } else {
        spec.open_tooltip
    };

    div()
        .relative()
        .child(
            Button::new(spec.id)
                .icon(icon(ChromeIcon::Bell, IconSize::Label, color))
                .tooltip(tooltip)
                .ghost()
                .small()
                .compact()
                .on_click(on_click),
        )
        .when(spec.unread, |button| {
            button.child(
                div()
                    .absolute()
                    .top(px(3.))
                    .right(px(3.))
                    .size(px(6.))
                    .rounded_full()
                    .bg(t.accent_rgba()),
            )
        })
        .into_any_element()
}

/// Push one Chrome-styled semantic toast through GPUI Component's lifecycle.
pub fn push_notification_toast(window: &mut Window, spec: NotificationToastSpec, cx: &mut App) {
    window.push_notification(
        Notification::new()
            .title(spec.title)
            .message(spec.message)
            .with_type(notification_type(spec.severity))
            .autohide(spec.autohide),
        cx,
    );
}

/// Clear only currently visible/queued toast entities. History remains an app
/// concern.
pub fn clear_notification_toasts(window: &mut Window, cx: &mut App) {
    window.clear_notifications(cx);
}

/// Read GPUI Component's installed Root and mount its toast list at Chrome's
/// stable notification anchor.
pub fn render_notification_toasts(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let layer = Root::render_notification_layer(window, cx)?;
    Some(render_notification_toast_layer(layer))
}

/// Full-window click-away layer plus the standard notification panel anchor.
/// The panel itself occludes pointer propagation.
pub fn render_notification_center_layer(
    panel: impl IntoElement,
    on_click_away: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id("chrome-notification-center-layer")
        .absolute()
        .inset_0()
        .on_mouse_down(MouseButton::Left, on_click_away)
        .child(render_notification_panel_anchor(panel))
        .into_any_element()
}

fn notification_type(severity: NotificationSeverity) -> NotificationType {
    match severity {
        NotificationSeverity::Info => NotificationType::Info,
        NotificationSeverity::Success => NotificationType::Success,
        NotificationSeverity::Warning => NotificationType::Warning,
        NotificationSeverity::Error => NotificationType::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::{NotificationBellSpec, NotificationSeverity, NotificationToastSpec};

    #[test]
    fn toast_defaults_to_autohide_and_can_be_overridden() {
        let default = NotificationToastSpec::new(NotificationSeverity::Info, "title", "message");
        assert!(default.autohide);

        let persistent =
            NotificationToastSpec::new(NotificationSeverity::Warning, "title", "message")
                .autohide(false);
        assert!(!persistent.autohide);
    }

    #[test]
    fn bell_spec_keeps_app_copy_and_state() {
        let spec = NotificationBellSpec::new("bell", true, true, "Open", "Close");
        assert!(spec.active);
        assert!(spec.unread);
        assert_eq!(spec.open_tooltip.as_ref(), "Open");
        assert_eq!(spec.close_tooltip.as_ref(), "Close");
    }
}
