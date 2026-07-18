//! Bounded toast notifications for DesktopApp.

use gpui::*;
use gpui_component::WindowExt;
use gpui_component::notification::{Notification, NotificationType};
use piko_client_core::state::ConnectionState;

use super::desktop_app::DesktopApp;

impl DesktopApp {
    /// Push toasts for new errors / disconnect; fingerprint to avoid spam.
    pub(crate) fn sync_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let err = self.bridge_state().last_error.clone();
        if let Some(msg) = err
            && self.last_notified_error.as_ref() != Some(&msg)
        {
            self.last_notified_error = Some(msg.clone());
            let truncated = truncate_msg(&msg, 160);
            window.push_notification(
                Notification::new()
                    .title("Error")
                    .message(truncated)
                    .with_type(NotificationType::Error)
                    .autohide(true),
                cx,
            );
        }

        let connected = matches!(
            self.bridge_state().shell.connection,
            ConnectionState::Connected
        );
        if self.last_connection_connected && !connected {
            window.push_notification(
                Notification::new()
                    .title("Disconnected")
                    .message("Lost connection to hostd.")
                    .with_type(NotificationType::Warning)
                    .autohide(true),
                cx,
            );
        }
        self.last_connection_connected = connected;
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
