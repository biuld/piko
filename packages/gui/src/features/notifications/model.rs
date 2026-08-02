use std::time::{Duration, Instant};

pub use island::components::notification::{
    NotificationCenterStore, NotificationMessage, NotificationSeverity,
};

const HISTORY_LIMIT: usize = 100;

/// One piko notice fed to the island [`NotificationCenterStore`].
#[derive(Debug, Clone)]
pub struct AppNotification {
    pub severity: NotificationSeverity,
    pub title: String,
    pub message: String,
    pub created_at: Instant,
    notice_id: String,
}

impl AppNotification {
    pub(crate) fn new(
        notice_id: String,
        severity: NotificationSeverity,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            notice_id,
            severity,
            title: title.into(),
            message: message.into(),
            created_at: Instant::now(),
        }
    }
}

impl NotificationMessage for AppNotification {
    fn id(&self) -> &str {
        &self.notice_id
    }

    fn severity(&self) -> NotificationSeverity {
        self.severity
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn message(&self) -> &str {
        &self.message
    }

    fn ongoing(&self) -> bool {
        false
    }

    fn thread_key(&self) -> Option<&str> {
        None
    }

    fn time_label(&self) -> String {
        relative_time(self.created_at, Instant::now())
    }
}

/// Product notification center: island-owned behavior with a bounded history.
pub type AppNotificationCenter = NotificationCenterStore<AppNotification>;

/// Ingest one notice and keep the history bounded to [`HISTORY_LIMIT`] rows.
///
/// Retention is an app responsibility; island owns history ordering, unread
/// state, coalescing, and the toast / native delivery queue.
pub fn push_bounded(center: &mut AppNotificationCenter, notice: AppNotification) -> bool {
    let changed = center.ingest(notice);
    while center.records().len() > HISTORY_LIMIT {
        let Some(oldest) = center.records().last() else {
            break;
        };
        center.remove(oldest.row_id);
    }
    changed
}

/// Content-based stable notice identity. Repeated identical events coalesce
/// into one history row instead of spamming the center.
pub fn notice_id(severity: NotificationSeverity, title: &str, message: &str) -> String {
    format!("{severity:?}/{title}/{message}")
}

pub fn relative_time(created_at: Instant, now: Instant) -> String {
    let age = now.saturating_duration_since(created_at);
    if age < Duration::from_secs(60) {
        crate::t!("notifications.time.now")
    } else if age < Duration::from_secs(60 * 60) {
        crate::t!("notifications.time.minutes", count = age.as_secs() / 60)
    } else if age < Duration::from_secs(24 * 60 * 60) {
        crate::t!(
            "notifications.time.hours",
            count = age.as_secs() / (60 * 60)
        )
    } else {
        crate::t!(
            "notifications.time.days",
            count = age.as_secs() / (24 * 60 * 60)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(text: &str) -> AppNotification {
        AppNotification::new(
            notice_id(NotificationSeverity::Error, "Title", text),
            NotificationSeverity::Error,
            "Title",
            text,
        )
    }

    #[test]
    fn push_is_newest_first_and_bounded() {
        let mut center = AppNotificationCenter::default();
        for i in 0..105 {
            assert!(push_bounded(&mut center, notice(&format!("event {i}"))));
        }
        assert_eq!(center.records().len(), HISTORY_LIMIT);
        assert_eq!(
            center.records().first().unwrap().message.message(),
            "event 104"
        );
        assert_eq!(
            center.records().last().unwrap().message.message(),
            "event 5"
        );
    }

    #[test]
    fn identical_notice_coalesces_into_one_row() {
        let mut center = AppNotificationCenter::default();
        assert!(push_bounded(&mut center, notice("boom")));
        // Consecutive identical notices are deduped by the store.
        assert!(!push_bounded(&mut center, notice("boom")));
        assert_eq!(center.records().len(), 1);
    }

    #[test]
    fn open_clears_unread_and_remove_clears_one_row() {
        let mut center = AppNotificationCenter::default();
        push_bounded(&mut center, notice("a"));
        assert!(center.unread());
        center.toggle_open();
        assert!(!center.unread());
        let row_id = center.records().first().unwrap().row_id;
        center.remove(row_id);
        assert!(center.records().is_empty());
    }

    #[test]
    fn clear_resets_history_and_unread() {
        let mut center = AppNotificationCenter::default();
        push_bounded(&mut center, notice("a"));
        center.clear();
        assert!(center.records().is_empty());
        assert!(!center.unread());
    }

    #[test]
    fn relative_time_scales_with_age() {
        let now = Instant::now();
        assert_eq!(relative_time(now, now), crate::t!("notifications.time.now"));
        let past = now - Duration::from_secs(5 * 60);
        assert_eq!(
            relative_time(past, now),
            crate::t!("notifications.time.minutes", count = 5)
        );
    }
}
