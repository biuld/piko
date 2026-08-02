//! Window-local notification history and floating panel body.

mod model;
mod render;

pub use model::{
    AppNotification, AppNotificationCenter, NotificationSeverity, notice_id, push_bounded,
};
pub use render::render_notification_center;
