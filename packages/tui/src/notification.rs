use std::collections::VecDeque;

#[derive(Clone, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone)]
pub struct Notification {
    pub level: NotificationLevel,
    pub message: String,
}

#[derive(Default)]
pub struct NotificationCenter {
    items: VecDeque<Notification>,
}

impl NotificationCenter {
    pub fn push(&mut self, level: NotificationLevel, message: impl Into<String>) {
        self.items.push_back(Notification {
            level,
            message: message.into(),
        });
        while self.items.len() > 5 {
            self.items.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn items(&self) -> &VecDeque<Notification> {
        &self.items
    }

    /// Whether any notification should occupy screen space.
    /// Only warning and error levels are considered visible; info messages
    /// are transient status updates that don't need a dedicated row.
    pub fn has_visible(&self) -> bool {
        self.items.iter().any(|n| n.level != NotificationLevel::Info)
    }

    /// The most recent visible notification, if any.
    pub fn visible(&self) -> Option<&Notification> {
        self.items.iter().rev().find(|n| n.level != NotificationLevel::Info)
    }
}
