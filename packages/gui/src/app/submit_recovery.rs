//! Command-correlated Composer recovery for rejected submissions.

use std::collections::{HashMap, HashSet};

use piko_client_core::ClientState;
use piko_client_core::state::PendingOp;

#[derive(Debug, Default)]
pub struct SubmitRecovery {
    pending: HashMap<String, String>,
    processed_failures: HashSet<String>,
    restore: Vec<String>,
}

#[derive(Debug, Default)]
pub struct FirstSubmitRecovery {
    pending: Option<(String, String)>,
}

impl FirstSubmitRecovery {
    pub fn track(&mut self, command_id: String, text: String) {
        self.pending = Some((command_id, text));
    }

    pub fn resolve(&mut self, state: &ClientState) -> Option<String> {
        if state.is_live() {
            return self.pending.take().map(|(_, text)| text);
        }
        if self.pending.as_ref().is_some_and(|(id, _)| {
            state.command_failures.iter().any(|failure| {
                failure.command_id == *id && matches!(failure.operation, PendingOp::Create)
            })
        }) {
            // The Composer was never cleared, so only the deferred auto-submit
            // must be dropped after a failed SessionCreate.
            self.pending = None;
        }
        None
    }
}

impl SubmitRecovery {
    pub fn track(&mut self, command_id: String, text: String) {
        self.pending.insert(command_id, text);
    }

    pub fn observe(&mut self, state: &ClientState) {
        for failure in &state.command_failures {
            if !self.processed_failures.insert(failure.command_id.clone()) {
                continue;
            }
            if matches!(failure.operation, PendingOp::Submit)
                && let Some(text) = self.pending.remove(&failure.command_id)
            {
                self.restore.push(text);
            }
        }
        self.pending
            .retain(|id, _| state.pending_commands.contains_key(id));
    }

    pub fn take_restore(&mut self) -> Vec<String> {
        std::mem::take(&mut self.restore)
    }

    pub fn defer_restore(&mut self, mut texts: Vec<String>) {
        texts.append(&mut self.restore);
        self.restore = texts;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_client_core::state::CommandFailure;

    #[test]
    fn only_correlated_submit_failure_restores_text() {
        let mut recovery = SubmitRecovery::default();
        recovery.track("submit-1".into(), "hello".into());
        let mut state = ClientState::default();
        state
            .pending_commands
            .insert("submit-1".into(), PendingOp::Submit);
        state.command_failures.push(CommandFailure {
            command_id: "other".into(),
            operation: PendingOp::Discover,
            message: "unrelated".into(),
        });
        recovery.observe(&state);
        assert!(recovery.take_restore().is_empty());

        state.pending_commands.remove("submit-1");
        state.command_failures.push(CommandFailure {
            command_id: "submit-1".into(),
            operation: PendingOp::Submit,
            message: "rejected".into(),
        });
        recovery.observe(&state);
        assert_eq!(recovery.take_restore(), ["hello"]);
    }

    #[test]
    fn successful_submit_is_forgotten() {
        let mut recovery = SubmitRecovery::default();
        recovery.track("submit-1".into(), "hello".into());
        recovery.observe(&ClientState::default());
        let mut later = ClientState::default();
        later.command_failures.push(CommandFailure {
            command_id: "submit-1".into(),
            operation: PendingOp::Submit,
            message: "late unrelated state".into(),
        });
        recovery.observe(&later);
        assert!(recovery.take_restore().is_empty());
    }

    #[test]
    fn first_submit_runs_only_after_live_and_is_dropped_on_create_failure() {
        let mut first = FirstSubmitRecovery::default();
        first.track("create-1".into(), "hello".into());
        assert_eq!(first.resolve(&ClientState::default()), None);

        let mut failed = ClientState::default();
        failed.command_failures.push(CommandFailure {
            command_id: "create-1".into(),
            operation: PendingOp::Create,
            message: "rejected".into(),
        });
        assert_eq!(first.resolve(&failed), None);

        let mut live = ClientState::default();
        live.session_phase = piko_client_core::SessionPhase::Live;
        first.track("create-2".into(), "world".into());
        assert_eq!(first.resolve(&live).as_deref(), Some("world"));
        assert_eq!(first.resolve(&live), None);
    }
}
