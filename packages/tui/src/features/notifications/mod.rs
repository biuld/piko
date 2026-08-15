use std::{
    cell::{Cell, RefCell},
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
const COPY_FEEDBACK_TTL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

pub(crate) fn level_glyph(level: NotificationLevel) -> &'static str {
    match level {
        NotificationLevel::Info => "ⓘ",
        NotificationLevel::Warning => "▲",
        NotificationLevel::Error => "✗",
    }
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
    max_scroll: Cell<usize>,
    selected: Cell<usize>,
    visible_count: Cell<usize>,
    viewport_height: Cell<usize>,
    item_offsets: RefCell<Vec<(u64, usize, usize)>>,
    copied: Option<(u64, Instant)>,
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
        self.max_scroll.set(0);
        self.selected.set(0);
    }

    pub fn set_view_scope(&mut self, scope: NotificationViewScope) {
        self.view_scope = scope;
        self.scroll = 0;
        self.max_scroll.set(0);
        self.selected.set(0);
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
            .min(self.max_scroll.get());
    }

    pub fn select_prev(&mut self) {
        self.selected.set(self.selected.get().saturating_sub(1));
        self.ensure_selected_visible();
    }

    pub fn select_next(&mut self) {
        let last = self.visible_count.get().saturating_sub(1);
        self.selected
            .set(self.selected.get().saturating_add(1).min(last));
        self.ensure_selected_visible();
    }

    pub fn selected_copy_payload(&self, session_id: Option<&str>) -> Option<(u64, String)> {
        self.modal_items(session_id)
            .get(self.selected.get())
            .map(|notice| (notice.id, notice.message.clone()))
    }

    pub fn message(&self, id: u64) -> Option<String> {
        self.items
            .iter()
            .find(|notice| notice.id == id)
            .map(|notice| notice.message.clone())
    }

    pub fn mark_copied(&mut self, id: u64, now: Instant) {
        self.copied = Some((id, now + COPY_FEEDBACK_TTL));
    }

    pub(super) fn is_copied(&self, id: u64, now: Instant) -> bool {
        matches!(self.copied, Some((copied_id, until)) if copied_id == id && until > now)
    }

    fn ensure_selected_visible(&mut self) {
        let selected = self.selected.get();
        let Some((_, start, end)) = self.item_offsets.borrow().get(selected).copied() else {
            return;
        };
        let height = self.viewport_height.get().max(1);
        if start < self.scroll {
            self.scroll = start;
        } else if end > self.scroll.saturating_add(height) {
            self.scroll = if end.saturating_sub(start) >= height {
                start
            } else {
                end.saturating_sub(height)
            };
        }
        self.scroll = self.scroll.min(self.max_scroll.get());
    }

    #[cfg(test)]
    pub fn items(&self) -> &VecDeque<Notification> {
        &self.items
    }

    pub fn modal_len(&self) -> usize {
        self.items.len()
    }

    #[cfg(test)]
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
            (PointerGesture::ScrollUp, Some(HitId::NotificationCopy(_))) => {
                self.scroll_up(3);
                Vec::new()
            }
            (PointerGesture::ScrollDown, Some(HitId::NotificationCopy(_))) => {
                self.scroll_down(3);
                Vec::new()
            }
            (PointerGesture::Activate, Some(HitId::NotificationCopy(id))) => {
                if let Some(index) = self
                    .item_offsets
                    .borrow()
                    .iter()
                    .position(|(item_id, _, _)| *item_id == id)
                {
                    self.selected.set(index);
                }
                vec![NotificationAction::Copy(id).into()]
            }
            (PointerGesture::Activate, Some(HitId::Notice)) => {
                vec![NotificationAction::DismissVisible.into()]
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests;
