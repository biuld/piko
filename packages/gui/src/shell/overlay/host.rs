//! OverlayHost state: priority stack, fingerprint, overlay focus session.
//!
//! Focus open/close uses chrome [`OverlayFocusSession`] (roadmap E4) so
//! product wiring does not invent a parallel bool stack.

use super::OverlayFocusSession;
use super::kinds::{LocalConfirmKind, OverlayLayer, TransientKind};
use super::prompt_front::{PromptFront, PromptKind, prompt_fingerprint};

/// Chrome-owned overlay presentation state (not Client Core authority).
#[derive(Debug, Default)]
pub struct OverlayHost {
    pub host_prompt: Option<PromptFront>,
    pub open_prompt_fp: Option<String>,
    pub open_prompt_flight: Option<bool>,
    pub local_confirm: Option<LocalConfirmKind>,
    pub transient: Option<TransientKind>,
    /// Chrome overlay focus episode (begin on first open, end on last close).
    pub focus_session: OverlayFocusSession,
}

impl OverlayHost {
    pub fn visible_layer(&self) -> Option<OverlayLayer> {
        if self.host_prompt.is_some() {
            return Some(OverlayLayer::HostPrompt);
        }
        if let Some(kind) = &self.local_confirm {
            return Some(OverlayLayer::LocalConfirm(kind.clone()));
        }
        if let Some(kind) = &self.transient {
            return Some(OverlayLayer::Transient(kind.clone()));
        }
        None
    }

    pub fn is_command_palette_open(&self) -> bool {
        matches!(self.transient, Some(TransientKind::CommandPalette))
            && self.host_prompt.is_none()
            && self.local_confirm.is_none()
    }

    pub fn try_open_session_delete_confirm(
        &mut self,
        session_id: String,
        display_name: String,
    ) -> bool {
        if self.host_prompt.is_some() {
            return false;
        }
        self.transient = None;
        self.local_confirm = Some(LocalConfirmKind::DeleteSession {
            session_id,
            display_name,
        });
        true
    }

    pub fn try_open_session_rename(&mut self, session_id: String, initial_name: String) -> bool {
        if self.host_prompt.is_some() {
            return false;
        }
        self.local_confirm = None;
        self.transient = Some(TransientKind::SessionRename {
            session_id,
            initial_name,
        });
        true
    }

    /// Sync HostPrompt from Core front; returns true when presentation changed.
    pub fn sync_host_prompt(&mut self, front: Option<PromptFront>) -> bool {
        let fp = prompt_fingerprint(front.as_ref());
        let flight = front.as_ref().map(|f| f.response_in_flight);
        if fp == self.open_prompt_fp && flight == self.open_prompt_flight {
            return false;
        }
        self.open_prompt_fp = fp;
        self.open_prompt_flight = flight;
        self.host_prompt = front;
        // HostPrompt outranks local confirm / transient.
        if self.host_prompt.is_some() {
            self.local_confirm = None;
            self.transient = None;
        }
        true
    }

    /// Open busy-quit confirm unless a HostPrompt is active.
    pub fn try_open_quit_confirm(&mut self) -> bool {
        if self.host_prompt.is_some() {
            return false;
        }
        self.transient = None;
        self.local_confirm = Some(LocalConfirmKind::QuitBusy);
        true
    }

    pub fn close_local_confirm(&mut self) {
        self.local_confirm = None;
    }

    /// Open Command Palette unless a HostPrompt is active.
    pub fn try_open_command_palette(&mut self) -> bool {
        if self.host_prompt.is_some() {
            return false;
        }
        self.local_confirm = None;
        self.transient = Some(TransientKind::CommandPalette);
        true
    }

    pub fn close_transient(&mut self) {
        self.transient = None;
    }

    /// Start an overlay focus episode. Returns `true` when the caller should
    /// save outer island focus now (fresh open only).
    pub fn begin_focus_session(&mut self) -> bool {
        self.focus_session.begin()
    }

    /// End the focus episode when no overlay layer remains. Returns `true`
    /// when the caller should restore island keyboard focus.
    pub fn end_focus_session_if_idle(&mut self) -> bool {
        if self.visible_layer().is_some() {
            return false;
        }
        self.focus_session.end()
    }

    /// Apply Escape policy for overlay layers (Sheet is handled by the caller).
    pub fn handle_escape(&mut self) -> EscapeOutcome {
        match self.visible_layer() {
            Some(OverlayLayer::HostPrompt) => {
                if matches!(
                    self.host_prompt.as_ref().map(|p| p.kind),
                    Some(PromptKind::Interaction)
                ) {
                    EscapeOutcome::CancelInteraction
                } else {
                    EscapeOutcome::Swallowed
                }
            }
            Some(OverlayLayer::Transient(_)) => {
                self.transient = None;
                EscapeOutcome::Closed
            }
            Some(OverlayLayer::LocalConfirm(_)) => {
                self.local_confirm = None;
                EscapeOutcome::Closed
            }
            None => EscapeOutcome::NotHandled,
        }
    }
}

/// Result of OverlayHost Escape handling (before Sheet fallback).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeOutcome {
    Swallowed,
    CancelInteraction,
    Closed,
    NotHandled,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approval_front() -> PromptFront {
        PromptFront {
            kind: PromptKind::Approval,
            id: "a1".into(),
            agent_instance_id: "root".into(),
            remaining: 1,
            response_in_flight: false,
            summary: "exec".into(),
        }
    }

    #[test]
    fn host_prompt_outranks_transient() {
        let mut host = OverlayHost::default();
        assert!(host.try_open_command_palette());
        assert!(host.sync_host_prompt(Some(approval_front())));
        assert_eq!(host.visible_layer(), Some(OverlayLayer::HostPrompt));
        assert!(host.transient.is_none());
    }

    #[test]
    fn palette_refused_while_host_prompt() {
        let mut host = OverlayHost::default();
        host.sync_host_prompt(Some(approval_front()));
        assert!(!host.try_open_command_palette());
    }

    #[test]
    fn quit_confirm_refused_while_host_prompt() {
        let mut host = OverlayHost::default();
        host.sync_host_prompt(Some(approval_front()));
        assert!(!host.try_open_quit_confirm());
    }

    #[test]
    fn escape_closes_transient_then_confirm() {
        let mut host = OverlayHost::default();
        host.try_open_command_palette();
        assert_eq!(host.handle_escape(), EscapeOutcome::Closed);
        assert!(host.transient.is_none());
        host.try_open_quit_confirm();
        assert_eq!(host.handle_escape(), EscapeOutcome::Closed);
        assert!(host.local_confirm.is_none());
    }

    #[test]
    fn escape_swallows_on_approval() {
        let mut host = OverlayHost::default();
        host.sync_host_prompt(Some(approval_front()));
        assert_eq!(host.handle_escape(), EscapeOutcome::Swallowed);
        assert!(host.host_prompt.is_some());
    }

    #[test]
    fn focus_session_begin_once_and_end_when_idle() {
        let mut host = OverlayHost::default();
        assert!(host.begin_focus_session());
        assert!(!host.begin_focus_session()); // already open
        assert!(host.focus_session.is_open());

        // Layer still open — must not end/restore yet.
        host.try_open_command_palette();
        assert!(!host.end_focus_session_if_idle());
        assert!(host.focus_session.is_open());

        host.close_transient();
        assert!(host.end_focus_session_if_idle());
        assert!(!host.focus_session.is_open());
        assert!(!host.end_focus_session_if_idle());
    }

    #[test]
    fn focus_session_survives_host_prompt_replacing_palette() {
        let mut host = OverlayHost::default();
        assert!(host.try_open_command_palette());
        assert!(host.begin_focus_session());
        assert!(host.sync_host_prompt(Some(approval_front())));
        assert!(host.transient.is_none());
        // Still have a layer — session stays open.
        assert!(!host.end_focus_session_if_idle());
        assert!(host.focus_session.is_open());
        host.sync_host_prompt(None);
        assert!(host.end_focus_session_if_idle());
    }
}
