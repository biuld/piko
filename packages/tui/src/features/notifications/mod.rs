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
pub enum NoticePolicy {
    Transient { visible_until: Instant },
    Dismissible,
    UntilResolved(NoticeSubject),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NoticeStatus {
    #[default]
    Active,
    Dismissed,
    Resolved,
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
    pub policy: NoticePolicy,
    pub status: NoticeStatus,
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
        let policy = if level == NotificationLevel::Info {
            NoticePolicy::Transient {
                visible_until: Instant::now() + INFO_TTL,
            }
        } else {
            NoticePolicy::Dismissible
        };
        self.push_with(NoticeScope::Global, level, policy, message)
    }

    pub fn push_with(
        &mut self,
        scope: NoticeScope,
        level: NotificationLevel,
        policy: NoticePolicy,
        message: impl Into<String>,
    ) -> u64 {
        let message = message.into();
        if let NoticePolicy::UntilResolved(subject) = &policy
            && let Some(existing) = self.items.iter_mut().rev().find(|notice| {
                notice.status == NoticeStatus::Active
                    && matches!(&notice.policy, NoticePolicy::UntilResolved(candidate) if candidate == subject)
            })
        {
            existing.scope = scope;
            existing.level = level;
            existing.message = message;
            return existing.id;
        }
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;
        self.items.push_back(Notification {
            id,
            level,
            scope,
            policy,
            status: NoticeStatus::Active,
            message,
        });
        id
    }

    /// Restore an authoritative pending notice without duplicating its local
    /// history record during snapshot reconciliation.
    pub fn restore_with(
        &mut self,
        scope: NoticeScope,
        level: NotificationLevel,
        policy: NoticePolicy,
        message: impl Into<String>,
    ) -> u64 {
        let message = message.into();
        if let NoticePolicy::UntilResolved(subject) = &policy
            && let Some(existing) = self.items.iter_mut().rev().find(|notice| {
                matches!(&notice.policy, NoticePolicy::UntilResolved(candidate) if candidate == subject)
            })
        {
            existing.scope = scope;
            existing.level = level;
            existing.policy = policy;
            existing.status = NoticeStatus::Active;
            existing.message = message;
            return existing.id;
        }
        self.push_with(scope, level, policy, message)
    }

    pub fn resolve(&mut self, subject: &NoticeSubject) {
        for notice in &mut self.items {
            if notice.status != NoticeStatus::Resolved
                && matches!(&notice.policy, NoticePolicy::UntilResolved(candidate) if candidate == subject)
            {
                notice.status = NoticeStatus::Resolved;
            }
        }
    }

    pub fn clear_state_derived_for_session(&mut self, session_id: &str) {
        for notice in &mut self.items {
            let same_session = matches!(
                &notice.scope,
                NoticeScope::Session(expected) if expected == session_id
            );
            let state_derived = matches!(
                &notice.policy,
                NoticePolicy::UntilResolved(NoticeSubject::Approval(_))
                    | NoticePolicy::UntilResolved(NoticeSubject::Interaction(_))
            );
            if notice.status == NoticeStatus::Active && same_session && state_derived {
                notice.status = NoticeStatus::Resolved;
            }
        }
    }

    pub fn dismiss_visible(
        &mut self,
        now: Instant,
        session_id: Option<&str>,
        agent_instance_id: Option<&str>,
    ) {
        let visible_id = self
            .row_visible_for(now, session_id, agent_instance_id)
            .map(|notice| notice.id);
        if let Some(id) = visible_id
            && let Some(notice) = self.items.iter_mut().find(|notice| notice.id == id)
        {
            notice.status = NoticeStatus::Dismissed;
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

    #[cfg(test)]
    pub fn items(&self) -> &VecDeque<Notification> {
        &self.items
    }

    pub fn modal_len(&self) -> usize {
        self.items.len()
    }

    pub fn has_row_visible_for(
        &self,
        now: Instant,
        session_id: Option<&str>,
        agent_instance_id: Option<&str>,
    ) -> bool {
        self.row_visible_for(now, session_id, agent_instance_id)
            .is_some()
    }

    pub fn row_visible_for(
        &self,
        now: Instant,
        session_id: Option<&str>,
        agent_instance_id: Option<&str>,
    ) -> Option<&Notification> {
        let applies =
            |notice: &&Notification| notice.scope.applies_to(session_id, agent_instance_id);
        self.items
            .iter()
            .rev()
            .filter(applies)
            .filter(|notice| notice.status == NoticeStatus::Active)
            .find(|notice| !matches!(notice.policy, NoticePolicy::Transient { .. }))
            .or_else(|| {
                self.items
                    .iter()
                    .rev()
                    .filter(applies)
                    .filter(|notice| notice.status == NoticeStatus::Active)
                    .find(|notice| {
                        matches!(
                            notice.policy,
                            NoticePolicy::Transient { visible_until } if visible_until > now
                        )
                    })
            })
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
    fn active_attention_precedes_transient_info_in_the_row() {
        let mut center = NotificationCenter::default();
        center.push(NotificationLevel::Warning, "keep me");
        for index in 0..20 {
            center.push(NotificationLevel::Info, format!("info {index}"));
        }

        assert_eq!(center.items.len(), 21);
        assert_eq!(
            center
                .row_visible_for(Instant::now(), None, None)
                .unwrap()
                .message,
            "keep me"
        );
    }

    #[test]
    fn subject_resolution_and_scoping_are_stable() {
        let mut center = NotificationCenter::default();
        let subject = NoticeSubject::Approval("approval-1".into());
        center.push_with(
            NoticeScope::Session("session-1".into()),
            NotificationLevel::Warning,
            NoticePolicy::UntilResolved(subject.clone()),
            "approve",
        );

        assert!(
            center
                .row_visible_for(Instant::now(), Some("session-2"), None)
                .is_none()
        );
        assert!(
            center
                .row_visible_for(Instant::now(), Some("session-1"), None)
                .is_some()
        );
        center.resolve(&subject);
        assert!(
            center
                .row_visible_for(Instant::now(), Some("session-1"), None)
                .is_none()
        );
        assert_eq!(center.items.len(), 1);
        assert_eq!(center.items[0].status, NoticeStatus::Resolved);
    }

    #[test]
    fn modal_defaults_to_current_and_can_show_all_sessions() {
        let mut center = NotificationCenter::default();
        center.push(NotificationLevel::Info, "global");
        center.push_with(
            NoticeScope::Session("session-1".into()),
            NotificationLevel::Warning,
            NoticePolicy::Dismissible,
            "current",
        );
        center.push_with(
            NoticeScope::Session("session-2".into()),
            NotificationLevel::Error,
            NoticePolicy::Dismissible,
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
            NoticePolicy::Dismissible,
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
    fn dismiss_hides_only_the_visible_notice_and_keeps_history() {
        let mut center = NotificationCenter::default();
        center.push(NotificationLevel::Warning, "first");
        center.push(NotificationLevel::Error, "second");

        center.dismiss_visible(Instant::now(), None, None);

        assert_eq!(center.items.len(), 2);
        assert_eq!(center.items.back().unwrap().status, NoticeStatus::Dismissed);
        assert_eq!(
            center
                .row_visible_for(Instant::now(), None, None)
                .unwrap()
                .message,
            "first"
        );
    }

    #[test]
    fn elapsed_info_leaves_the_row_but_remains_in_the_modal_queue() {
        let mut center = NotificationCenter::default();
        let now = Instant::now();
        center.push_with(
            NoticeScope::Global,
            NotificationLevel::Info,
            NoticePolicy::Transient { visible_until: now },
            "done",
        );

        assert!(center.row_visible_for(now, None, None).is_none());
        assert_eq!(center.items.len(), 1);
        assert_eq!(center.modal_items(None)[0].message, "done");
    }

    #[test]
    fn snapshot_restore_reactivates_history_without_appending_a_duplicate() {
        let mut center = NotificationCenter::default();
        let subject = NoticeSubject::Approval("approval-1".into());
        let policy = NoticePolicy::UntilResolved(subject.clone());
        center.push_with(
            NoticeScope::Session("session-1".into()),
            NotificationLevel::Warning,
            policy.clone(),
            "approve",
        );
        center.clear_state_derived_for_session("session-1");

        center.restore_with(
            NoticeScope::Session("session-1".into()),
            NotificationLevel::Warning,
            policy,
            "approve",
        );

        assert_eq!(center.items.len(), 1);
        assert_eq!(center.items[0].status, NoticeStatus::Active);
    }

    #[test]
    fn attention_history_is_not_capacity_evicted() {
        let mut center = NotificationCenter::default();
        for index in 0..40 {
            center.push(NotificationLevel::Warning, format!("warning {index}"));
        }

        assert_eq!(center.items.len(), 40);
    }
}
