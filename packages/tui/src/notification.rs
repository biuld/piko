use std::collections::VecDeque;

#[derive(Clone)]
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
}
