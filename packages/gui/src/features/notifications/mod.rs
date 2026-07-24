//! Window-local notification history and floating panel body.

mod model;
mod render;

pub use model::{NotificationCenterState, NotificationId, NotificationSeverity};
pub use render::render_notification_center;
