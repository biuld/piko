//! Notification history, floating panel, and the unified toast entry point.

use gpui::*;
use piko_chrome::components::notification::{
    NotificationToastSpec, clear_notification_toasts, push_notification_toast,
    render_notification_center_layer as render_chrome_notification_center_layer,
    render_notification_toasts,
};
use piko_client_core::state::ConnectionState;

use crate::features::{NotificationId, NotificationSeverity, render_notification_center};

use super::desktop_app::{DesktopApp, ToggleNotificationCenter};

impl DesktopApp {
    pub(crate) fn push_app_notification(
        &mut self,
        severity: NotificationSeverity,
        title: impl Into<String>,
        message: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = title.into();
        let message = message.into();
        self.notifications
            .push(severity, title.clone(), message.clone());
        if !self.notifications.is_open() {
            push_notification_toast(
                window,
                NotificationToastSpec::new(severity, title, message),
                cx,
            );
        }
        cx.notify();
    }

    /// Push toasts for new errors / disconnect; fingerprint to avoid spam.
    pub(crate) fn sync_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let err = self.bridge_state().last_error.clone();
        if let Some(msg) = err
            && self.last_notified_error.as_ref() != Some(&msg)
        {
            self.last_notified_error = Some(msg.clone());
            let truncated = truncate_msg(&msg, 160);
            self.push_app_notification(
                NotificationSeverity::Error,
                crate::t!("notifications.toast.error.title"),
                truncated,
                window,
                cx,
            );
        }

        let connected = matches!(
            self.bridge_state().shell.connection,
            ConnectionState::Connected
        );
        if self.last_connection_connected && !connected {
            self.push_app_notification(
                NotificationSeverity::Warning,
                crate::t!("notifications.toast.disconnected.title"),
                crate::t!("notifications.toast.disconnected.message"),
                window,
                cx,
            );
        }
        self.last_connection_connected = connected;
    }

    pub(crate) fn action_toggle_notification_center(
        &mut self,
        _: &ToggleNotificationCenter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.notifications.toggle();
        if self.notifications.is_open() {
            clear_notification_toasts(window, cx);
        }
        cx.notify();
    }

    pub(crate) fn close_notification_center(&mut self, cx: &mut Context<Self>) -> bool {
        if self.notifications.close() {
            cx.notify();
            true
        } else {
            false
        }
    }

    pub(crate) fn clear_notification_center(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.notifications.clear();
        clear_notification_toasts(window, cx);
        cx.notify();
    }

    fn remove_notification_record(&mut self, id: NotificationId, cx: &mut Context<Self>) {
        if self.notifications.remove(id) {
            cx.notify();
        }
    }

    pub(crate) fn render_notification_center_layer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.notifications.is_open() {
            return None;
        }

        let viewport = window.viewport_size();
        let entity = cx.entity().downgrade();
        let remove_entity = entity.clone();
        let clear_entity = entity.clone();
        let panel = render_notification_center(
            &self.notifications,
            viewport,
            move |id, _, _, cx| {
                if let Some(view) = remove_entity.upgrade() {
                    view.update(cx, |this, cx| this.remove_notification_record(id, cx));
                }
            },
            move |_, window, cx| {
                if let Some(view) = clear_entity.upgrade() {
                    view.update(cx, |this, cx| this.clear_notification_center(window, cx));
                }
            },
        );

        Some(render_chrome_notification_center_layer(
            panel,
            move |_, _, cx| {
                if let Some(view) = entity.upgrade() {
                    view.update(cx, |this, cx| {
                        this.close_notification_center(cx);
                    });
                }
            },
        ))
    }

    pub(crate) fn render_toast_layer(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.notifications.is_open() {
            return None;
        }
        render_notification_toasts(window, cx)
    }
}

fn truncate_msg(msg: &str, max: usize) -> String {
    if msg.chars().count() <= max {
        msg.to_string()
    } else {
        let take: String = msg.chars().take(max.saturating_sub(1)).collect();
        format!("{take}…")
    }
}
