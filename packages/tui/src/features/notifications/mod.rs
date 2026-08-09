use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use crate::{
    app::{HitId, command::NotificationAction},
    ui::interaction::{ComponentHit, PointerComponent, PointerGesture},
};

mod panel;

pub use panel::NotificationPanelCtx;

const INFO_TTL: Duration = Duration::from_secs(3);
const MAX_TRANSIENT: usize = 5;
const MAX_ATTENTION: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoticeScope {
    Global,
    Session(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoticeSubject {
    Approval(String),
    Interaction(String),
    Auth(String),
}

#[derive(Clone, Debug)]
pub enum NoticeLifetime {
    Transient { expires_at: Instant },
    Dismissible,
    UntilResolved(NoticeSubject),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotificationViewScope {
    #[default]
    Current,
    All,
}

#[derive(Clone, Debug)]
pub struct Notification {
    pub id: u64,
    pub level: NotificationLevel,
    pub scope: NoticeScope,
    pub lifetime: NoticeLifetime,
    pub message: String,
}

#[derive(Default)]
pub struct NotificationCenter {
    items: VecDeque<Notification>,
    next_id: u64,
    view_scope: NotificationViewScope,
    scroll: usize,
}

impl NotificationCenter {
    pub fn push(&mut self, level: NotificationLevel, message: impl Into<String>) -> u64 {
        let lifetime = if level == NotificationLevel::Info {
            NoticeLifetime::Transient {
                expires_at: Instant::now() + INFO_TTL,
            }
        } else {
            NoticeLifetime::Dismissible
        };
        self.push_with(NoticeScope::Global, level, lifetime, message)
    }

    pub fn push_with(
        &mut self,
        scope: NoticeScope,
        level: NotificationLevel,
        lifetime: NoticeLifetime,
        message: impl Into<String>,
    ) -> u64 {
        if let NoticeLifetime::UntilResolved(subject) = &lifetime {
            self.resolve(subject);
        }
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;
        self.items.push_back(Notification {
            id,
            level,
            scope,
            lifetime,
            message: message.into(),
        });
        self.trim();
        id
    }

    pub fn resolve(&mut self, subject: &NoticeSubject) {
        self.items.retain(|notice| {
            !matches!(&notice.lifetime, NoticeLifetime::UntilResolved(candidate) if candidate == subject)
        });
        self.clamp_scroll();
    }

    pub fn clear_state_derived_for_session(&mut self, session_id: &str) {
        self.items.retain(|notice| {
            let same_session = matches!(
                &notice.scope,
                NoticeScope::Session(expected) if expected == session_id
            );
            let state_derived = matches!(
                &notice.lifetime,
                NoticeLifetime::UntilResolved(NoticeSubject::Approval(_))
                    | NoticeLifetime::UntilResolved(NoticeSubject::Interaction(_))
            );
            !(same_session && state_derived)
        });
        self.clamp_scroll();
    }

    pub fn expire(&mut self, now: Instant) {
        self.items.retain(|notice| {
            !matches!(notice.lifetime, NoticeLifetime::Transient { expires_at } if expires_at <= now)
        });
        self.clamp_scroll();
    }

    pub fn dismiss_visible(&mut self, session_id: Option<&str>, agent_instance_id: Option<&str>) {
        let visible_id = self
            .visible_for(session_id, agent_instance_id)
            .map(|notice| notice.id);
        if let Some(id) = visible_id {
            self.items.retain(|notice| notice.id != id);
            self.clamp_scroll();
        }
    }

    pub fn open_modal(&mut self) {
        self.view_scope = NotificationViewScope::Current;
        self.scroll = 0;
    }

    pub fn set_view_scope(&mut self, scope: NotificationViewScope) {
        self.view_scope = scope;
        self.scroll = 0;
    }

    pub fn toggle_view_scope(&mut self) {
        self.set_view_scope(match self.view_scope {
            NotificationViewScope::Current => NotificationViewScope::All,
            NotificationViewScope::All => NotificationViewScope::Current,
        });
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll = self
            .scroll
            .saturating_add(amount)
            .min(self.modal_len().saturating_sub(1));
    }

    pub fn count_for(&self, session_id: Option<&str>, agent_instance_id: Option<&str>) -> usize {
        self.items
            .iter()
            .filter(|notice| notice.scope.applies_to(session_id, agent_instance_id))
            .count()
    }

    #[cfg(test)]
    pub fn items(&self) -> &VecDeque<Notification> {
        &self.items
    }

    pub fn modal_len(&self) -> usize {
        self.items.len()
    }

    pub fn has_visible_for(
        &self,
        session_id: Option<&str>,
        agent_instance_id: Option<&str>,
    ) -> bool {
        self.visible_for(session_id, agent_instance_id).is_some()
    }

    pub fn visible_for(
        &self,
        session_id: Option<&str>,
        agent_instance_id: Option<&str>,
    ) -> Option<&Notification> {
        let applies =
            |notice: &&Notification| notice.scope.applies_to(session_id, agent_instance_id);
        self.items
            .iter()
            .rev()
            .filter(applies)
            .find(|notice| !matches!(notice.lifetime, NoticeLifetime::Transient { .. }))
            .or_else(|| self.items.iter().rev().find(applies))
    }

    fn trim(&mut self) {
        trim_kind(&mut self.items, true, MAX_TRANSIENT);
        trim_kind(&mut self.items, false, MAX_ATTENTION);
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.min(self.modal_len().saturating_sub(1));
    }

    fn modal_items(&self, session_id: Option<&str>) -> Vec<&Notification> {
        self.items
            .iter()
            .rev()
            .filter(|notice| {
                self.view_scope == NotificationViewScope::All
                    || notice.scope.applies_to(session_id, None)
            })
            .collect()
    }
}

impl NoticeScope {
    fn applies_to(&self, session_id: Option<&str>, _agent_instance_id: Option<&str>) -> bool {
        match self {
            Self::Global => true,
            Self::Session(expected) => session_id == Some(expected.as_str()),
        }
    }
}

fn trim_kind(items: &mut VecDeque<Notification>, transient: bool, limit: usize) {
    while items
        .iter()
        .filter(|notice| matches!(notice.lifetime, NoticeLifetime::Transient { .. }) == transient)
        .count()
        > limit
    {
        if let Some(index) = items.iter().position(|notice| {
            matches!(notice.lifetime, NoticeLifetime::Transient { .. }) == transient
        }) {
            items.remove(index);
        }
    }
}

impl PointerComponent<HitId> for NotificationCenter {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        match (gesture, hit.element) {
            (PointerGesture::Activate, Some(HitId::Mode(index))) => {
                self.set_view_scope(if index == 0 {
                    NotificationViewScope::Current
                } else {
                    NotificationViewScope::All
                });
                Vec::new()
            }
            (PointerGesture::ScrollUp, Some(HitId::Content)) => {
                self.scroll_up(3);
                Vec::new()
            }
            (PointerGesture::ScrollDown, Some(HitId::Content)) => {
                self.scroll_down(3);
                Vec::new()
            }
            (PointerGesture::Activate, Some(HitId::Notice)) => {
                vec![NotificationAction::DismissVisible.into()]
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_info_never_evicts_attention_notice() {
        let mut center = NotificationCenter::default();
        center.push(NotificationLevel::Warning, "keep me");
        for index in 0..20 {
            center.push(NotificationLevel::Info, format!("info {index}"));
        }

        assert_eq!(center.items.len(), MAX_TRANSIENT + 1);
        assert_eq!(center.visible_for(None, None).unwrap().message, "keep me");
    }

    #[test]
    fn subject_resolution_and_scoping_are_stable() {
        let mut center = NotificationCenter::default();
        let subject = NoticeSubject::Approval("approval-1".into());
        center.push_with(
            NoticeScope::Session("session-1".into()),
            NotificationLevel::Warning,
            NoticeLifetime::UntilResolved(subject.clone()),
            "approve",
        );

        assert!(center.visible_for(Some("session-2"), None).is_none());
        assert!(center.visible_for(Some("session-1"), None).is_some());
        center.resolve(&subject);
        assert!(center.visible_for(Some("session-1"), None).is_none());
    }

    #[test]
    fn modal_defaults_to_current_and_can_show_all_sessions() {
        let mut center = NotificationCenter::default();
        center.push(NotificationLevel::Info, "global");
        center.push_with(
            NoticeScope::Session("session-1".into()),
            NotificationLevel::Warning,
            NoticeLifetime::Dismissible,
            "current",
        );
        center.push_with(
            NoticeScope::Session("session-2".into()),
            NotificationLevel::Error,
            NoticeLifetime::Dismissible,
            "other",
        );

        center.open_modal();
        assert_eq!(center.modal_items(Some("session-1")).len(), 2);
        center.set_view_scope(NotificationViewScope::All);
        assert_eq!(center.modal_items(Some("session-1")).len(), 3);
    }

    #[test]
    fn mode_affix_pointer_switches_to_all_sessions() {
        let mut center = NotificationCenter::default();
        center.push_with(
            NoticeScope::Session("session-2".into()),
            NotificationLevel::Warning,
            NoticeLifetime::Dismissible,
            "other",
        );
        center.open_modal();

        let actions = center.pointer_event(
            ComponentHit {
                element: Some(HitId::Mode(1)),
                rect: ratatui::layout::Rect::new(0, 0, 10, 1),
                x: 1,
                y: 0,
            },
            PointerGesture::Activate,
        );

        assert!(actions.is_empty());
        assert_eq!(center.view_scope, NotificationViewScope::All);
        assert_eq!(center.modal_items(Some("session-1")).len(), 1);
    }

    #[test]
    fn dismiss_removes_only_the_visible_notice() {
        let mut center = NotificationCenter::default();
        center.push(NotificationLevel::Warning, "first");
        center.push(NotificationLevel::Error, "second");

        center.dismiss_visible(None, None);

        assert_eq!(center.items.len(), 1);
        assert_eq!(center.visible_for(None, None).unwrap().message, "first");
    }

    #[test]
    fn expired_transient_notice_is_removed() {
        let mut center = NotificationCenter::default();
        center.push_with(
            NoticeScope::Global,
            NotificationLevel::Info,
            NoticeLifetime::Transient {
                expires_at: Instant::now(),
            },
            "done",
        );

        center.expire(Instant::now());

        assert!(center.items.is_empty());
    }
}
