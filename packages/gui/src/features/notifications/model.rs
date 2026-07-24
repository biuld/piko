use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub use piko_chrome::components::notification::NotificationSeverity;

const HISTORY_LIMIT: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NotificationId(u64);

impl NotificationId {
    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct NotificationRecord {
    pub id: NotificationId,
    pub severity: NotificationSeverity,
    pub title: String,
    pub message: String,
    pub created_at: Instant,
}

#[derive(Debug, Default)]
pub struct NotificationCenterState {
    records: VecDeque<NotificationRecord>,
    next_id: u64,
    open: bool,
    unread: bool,
}

impl NotificationCenterState {
    pub fn push(
        &mut self,
        severity: NotificationSeverity,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> NotificationId {
        let id = NotificationId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.records.push_front(NotificationRecord {
            id,
            severity,
            title: title.into(),
            message: message.into(),
            created_at: Instant::now(),
        });
        self.records.truncate(HISTORY_LIMIT);
        if !self.open {
            self.unread = true;
        }
        id
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        if self.open {
            self.unread = false;
        }
    }

    pub fn close(&mut self) -> bool {
        let was_open = self.open;
        self.open = false;
        was_open
    }

    pub fn remove(&mut self, id: NotificationId) -> bool {
        let before = self.records.len();
        self.records.retain(|record| record.id != id);
        before != self.records.len()
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.unread = false;
    }

    pub fn records(&self) -> impl Iterator<Item = &NotificationRecord> {
        self.records.iter()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn has_unread(&self) -> bool {
        self.unread
    }
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

    #[test]
    fn push_is_newest_first_and_bounded() {
        let mut state = NotificationCenterState::default();
        for i in 0..105 {
            state.push(NotificationSeverity::Info, format!("n{i}"), "message");
        }
        let records: Vec<_> = state.records().collect();
        assert_eq!(records.len(), HISTORY_LIMIT);
        assert_eq!(records[0].title, "n104");
        assert_eq!(records[99].title, "n5");
    }

    #[test]
    fn open_marks_existing_and_new_records_read() {
        let mut state = NotificationCenterState::default();
        state.push(NotificationSeverity::Warning, "first", "message");
        assert!(state.has_unread());
        state.toggle();
        assert!(state.is_open());
        assert!(!state.has_unread());
        state.push(NotificationSeverity::Error, "second", "message");
        assert!(!state.has_unread());
        assert!(state.close());
    }

    #[test]
    fn removing_and_clearing_records_updates_store() {
        let mut state = NotificationCenterState::default();
        let first = state.push(NotificationSeverity::Info, "first", "message");
        state.push(NotificationSeverity::Success, "second", "message");
        assert!(state.remove(first));
        assert_eq!(state.records().count(), 1);
        state.clear();
        assert_eq!(state.records().count(), 0);
        assert!(!state.has_unread());
    }
}
