//! OverlayHost state: priority stack, fingerprint, focus-save flag.

use super::kinds::{LocalConfirmKind, OverlayLayer, TransientKind};
use crate::overlays::{PromptFront, PromptKind, prompt_fingerprint};

/// Chrome-owned overlay presentation state (not Client Core authority).
#[derive(Debug, Default)]
pub struct OverlayHost {
    pub host_prompt: Option<PromptFront>,
    pub open_prompt_fp: Option<String>,
    pub open_prompt_flight: Option<bool>,
    pub local_confirm: Option<LocalConfirmKind>,
    pub transient: Option<TransientKind>,
    /// True when opening an overlay saved island focus for later restore.
    pub focus_saved: bool,
}

impl OverlayHost {
    pub fn visible_layer(&self) -> Option<OverlayLayer> {
        if self.host_prompt.is_some() {
            return Some(OverlayLayer::HostPrompt);
        }
        if let Some(kind) = self.local_confirm {
            return Some(OverlayLayer::LocalConfirm(kind));
        }
        if let Some(kind) = self.transient {
            return Some(OverlayLayer::Transient(kind));
        }
        None
    }

    pub fn is_command_palette_open(&self) -> bool {
        self.transient == Some(TransientKind::CommandPalette)
            && self.host_prompt.is_none()
            && self.local_confirm.is_none()
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
    use crate::overlays::PromptKind;

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
}
