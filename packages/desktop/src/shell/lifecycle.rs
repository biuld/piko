//! Host hydration and presentation-hint reconciliation.

use super::*;

impl Shell {
    pub(super) fn open_session(
        &mut self,
        session_id: String,
        session_path: Option<String>,
    ) -> Vec<ClientIntent> {
        self.subscribed_agent = None;
        self.prefs.last_session_id = Some(session_id.clone());
        let _ = self.prefs.save(&self.prefs_path);
        vec![ClientIntent::OpenSession {
            session_id,
            session_path,
        }]
    }

    pub(super) fn maybe_warm_reopen(&mut self) {
        if self.warm_reopen_attempted
            || self
                .state
                .core
                .pending_commands
                .values()
                .any(|operation| matches!(operation, PendingOp::Discover))
        {
            return;
        }
        self.warm_reopen_attempted = true;
        let Some(session_id) = self.prefs.last_session_id.clone() else {
            return;
        };
        let Some(summary) = self
            .state
            .core
            .session_list
            .sessions
            .iter()
            .find(|summary| summary.session_id == session_id)
            .cloned()
        else {
            self.prefs.last_session_id = None;
            let _ = self.prefs.save(&self.prefs_path);
            return;
        };
        let intents = self.open_session(summary.session_id, summary.session_path);
        for intent in intents {
            let commands = reduce(
                &mut self.state,
                &mut self.command_ids,
                ClientMsg::Intent(intent),
            );
            self.send_commands(commands);
        }
    }

    pub(super) fn finish_hydration_if_ready(&mut self) {
        if self.state.connection != DesktopConnection::Hydrating {
            return;
        }
        let bootstrap_pending = self.state.core.pending_commands.values().any(|operation| {
            matches!(
                operation,
                PendingOp::Discover | PendingOp::ListModels | PendingOp::Open { .. }
            )
        });
        if !bootstrap_pending && self.warm_reopen_attempted {
            self.state.on_hydrated();
        }
    }

    pub(super) fn remember_live_session(&mut self) {
        let session_id = self
            .state
            .core
            .live_session
            .as_ref()
            .map(|session| session.session_id.clone());
        if session_id.is_some() && self.prefs.last_session_id != session_id {
            self.prefs.last_session_id = session_id;
            let _ = self.prefs.save(&self.prefs_path);
        }
    }
}
