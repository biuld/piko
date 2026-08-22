//! Composer submit / cancel, keyed by the current agent tab (F-43 PR 5).

use std::collections::HashSet;

use super::*;

impl Shell {
    pub(super) fn submit_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if island::components::menu::context_menu_is_open(window, cx) {
            return;
        }
        if self.state.connection != DesktopConnection::Live
            || self.state.core.session_phase != SessionPhase::Live
            || self.pending_agent.is_some()
            || self.pending_submission().is_some()
        {
            return;
        }
        let value = self.composer_input.read(cx).value().to_string();
        let text = value.trim().to_string();
        if text.is_empty() {
            return;
        }
        let before: HashSet<_> = self.state.core.pending_commands.keys().cloned().collect();
        let commands = reduce(
            &mut self.state,
            &mut self.command_ids,
            ClientMsg::Intent(ClientIntent::SubmitTurn { text: text.clone() }),
        );
        let command_id = self
            .state
            .core
            .pending_commands
            .iter()
            .find_map(|(id, operation)| {
                (!before.contains(id) && matches!(operation, PendingOp::Submit)).then(|| id.clone())
            });
        let Some(command_id) = command_id else {
            return;
        };
        let view = self.view_local();
        view.pending_submission = Some(composer::PendingSubmission { command_id, text });
        view.composer_error = None;
        self.send_commands(commands);
        cx.notify();
    }

    pub(super) fn reconcile_submission(&mut self) {
        let owners: Vec<(String, composer::PendingSubmission)> = self
            .views
            .iter()
            .filter_map(|(key, view)| {
                view.pending_submission
                    .clone()
                    .map(|pending| (key.clone(), pending))
            })
            .collect();
        for (key, pending) in owners {
            if self
                .state
                .core
                .pending_commands
                .contains_key(&pending.command_id)
            {
                continue;
            }
            if let Some(failure) = self
                .state
                .core
                .command_failures
                .iter()
                .rev()
                .find(|failure| failure.command_id == pending.command_id)
            {
                if let Some(view) = self.views.get_mut(&key) {
                    view.composer_error = Some(format!("Send failed: {}", failure.message));
                    view.pending_submission = None;
                }
            } else {
                if let Some(view) = self.views.get_mut(&key) {
                    view.pending_submission = None;
                }
                self.drafts.insert(key.clone(), String::new());
                if key == self.draft_key {
                    self.clear_accepted_draft = Some(pending.text);
                }
            }
        }
    }

    pub(super) fn cancel_turn(&mut self, cx: &mut Context<Self>) {
        if self.pending_agent.is_some() {
            return;
        }
        self.dispatch_intents(cx, vec![ClientIntent::CancelTurn]);
    }
}
