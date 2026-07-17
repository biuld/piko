use piko_protocol::{ApprovalDecision, Command};

use crate::{
    app::{AppState, command_id, effect::Effect},
    features::notifications::NotificationLevel,
};

impl AppState {
    pub(crate) fn submit(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        let submitted_draft = self.editor.text();
        let Some(text) = self.editor.take_trimmed() else {
            return effects;
        };
        self.refresh_suggestions();
        if text.starts_with('/') {
            if let Some(slash_effects) = self.try_slash_command(&text) {
                return slash_effects;
            } else {
                self.editor.restore_text(&submitted_draft);
                self.status = format!("Unknown slash command: {}", text);
                self.notify(
                    NotificationLevel::Error,
                    format!("Unknown slash command: {}", text),
                );
                return effects;
            }
        }
        let Some(session_id) = self.session.id.clone() else {
            self.session.pending_turn_text = Some(text);

            if !self.session.initializing {
                self.begin_session_hydration(None);
                let create_id = command_id();
                self.session.pending.track(
                    create_id.clone(),
                    super::pending::PendingCommandKind::SessionCreate,
                );
                effects.push(Effect::send(Command::SessionCreate {
                    command_id: create_id,
                    cwd: self.cwd.to_string_lossy().into_owned(),
                }));
                self.status = "creating session...".to_string();
            } else {
                self.status = "waiting for session...".to_string();
            }
            return effects;
        };
        let submit_command_id = command_id();
        let Some(target_agent_instance_id) = self.agent_panel.active_agent_instance_id.clone()
        else {
            self.editor.restore_text(&submitted_draft);
            self.status = "no agent selected".to_string();
            self.notify(NotificationLevel::Error, "no agent selected");
            return effects;
        };
        let target_name = self
            .agent_panel
            .agents
            .iter()
            .find(|agent| agent.agent_instance_id == target_agent_instance_id)
            .map(|agent| agent.name.clone())
            .unwrap_or_else(|| target_agent_instance_id.clone());
        self.session.pending.track(
            submit_command_id.clone(),
            super::pending::PendingCommandKind::ChatSubmit,
        );
        effects.push(Effect::send(Command::ChatSubmit {
            command_id: submit_command_id,
            session_id,
            target_agent_instance_id,
            text,
        }));
        self.status = format!("submitted to {target_name}");
        self.notify(
            NotificationLevel::Info,
            format!("submitted to {target_name}"),
        );
        effects
    }

    pub(crate) fn cancel(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        let (Some(session_id), Some(turn_id)) = (
            self.session.id.clone(),
            self.active_turn_id().map(str::to_string),
        ) else {
            self.editor.restore_text("");
            self.status = "editor cleared".to_string();
            return effects;
        };
        effects.push(Effect::send(Command::TurnCancel {
            command_id: command_id(),
            session_id,
            turn_id,
        }));
        self.status = "cancel requested".to_string();
        effects
    }

    pub(crate) fn respond_approval(&mut self, decision: ApprovalDecision) -> Vec<Effect> {
        let mut effects = Vec::new();
        let Some(session_id) = self.session.id.clone() else {
            return effects;
        };
        let Some(approval) = self.approvals.front() else {
            self.status = "no pending approval".to_string();
            return effects;
        };
        let decision_label = format!("{decision:?}");
        let approval_id = approval.id.clone();
        effects.push(Effect::send(Command::ApprovalRespond {
            command_id: command_id(),
            session_id,
            approval_id,
            decision,
            note: None,
        }));
        self.status = format!("approval response sent: {decision_label}");
        self.notify(
            NotificationLevel::Info,
            format!("approval response sent: {decision_label}"),
        );
        effects
    }

    pub(crate) fn submit_tool_interaction(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        let Some(session_id) = self.session.id.clone() else {
            return effects;
        };
        let Some(interaction) = self.interactions.front_mut() else {
            self.status = "no pending interaction".to_string();
            return effects;
        };
        let workflow = &mut interaction.workflow;
        if workflow.input_active() {
            workflow.set_input_active(false);
            if !workflow.require_confirm && workflow.questions.len() == 1 {
                // fall through to submit below
            } else {
                return effects;
            }
        } else if !workflow.confirm_focused {
            let question = &workflow.questions[workflow.active_question_idx];
            if question
                .choices
                .get(question.selected_idx)
                .is_some_and(|choice| choice.has_input)
            {
                workflow.set_input_active(true);
                return effects;
            }
            if workflow.require_confirm
                || workflow.active_question_idx + 1 < workflow.questions.len()
            {
                workflow.next_step();
                return effects;
            }
        }

        if let Some((interaction_id, response)) = self.interactions.submit_response() {
            effects.push(Effect::send(Command::UserInteractionRespond {
                command_id: command_id(),
                session_id,
                interaction_id,
                response,
            }));
            self.status = "interaction response sent".to_string();
        }
        effects
    }

    pub(crate) fn cancel_tool_interaction(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        let Some(session_id) = self.session.id.clone() else {
            return effects;
        };
        if let Some(interaction) = self.interactions.front_mut()
            && interaction.workflow.input_active()
        {
            interaction.workflow.set_input_active(false);
            return effects;
        }
        let Some((interaction_id, response)) = self.interactions.cancel_response() else {
            self.status = "no pending interaction".to_string();
            return effects;
        };
        effects.push(Effect::send(Command::UserInteractionRespond {
            command_id: command_id(),
            session_id,
            interaction_id,
            response,
        }));
        self.status = "interaction cancelled".to_string();
        effects
    }

    pub fn refresh_suggestions(&mut self) {
        let text = self.editor.text();
        self.editor.auto_complete.update(
            &self.cwd,
            &self.command_catalog,
            &text,
            self.editor.cursor(),
        );
    }

    pub(crate) fn select_suggestion_next(&mut self) {
        self.editor.auto_complete.select_next();
        self.fill_editor_with_selected_suggestion();
    }

    pub(crate) fn select_suggestion_prev(&mut self) {
        self.editor.auto_complete.select_prev();
        self.fill_editor_with_selected_suggestion();
    }

    pub(crate) fn accept_suggestion(&mut self) {
        if let Some(completion) = self.editor.auto_complete.accept() {
            let cursor = self.editor.cursor();
            let is_file = completion.replacement.starts_with('@');
            if is_file {
                self.editor.replace_range(completion.start, cursor, "");
                let trimmed = completion.replacement.trim_end().to_string();
                let placeholder = format!("[{trimmed}]");
                self.editor.insert_reference_block(placeholder, trimmed);
                if completion.replacement.ends_with(' ') {
                    self.editor.insert_char(' ');
                }
            } else {
                self.editor
                    .replace_range(completion.start, cursor, &completion.replacement);
            }
            self.refresh_suggestions();
        }
    }

    fn fill_editor_with_selected_suggestion(&mut self) {
        let selected_idx = self.editor.auto_complete.selected;
        if let Some(item) = self.editor.auto_complete.items.get(selected_idx).cloned() {
            let cursor = self.editor.cursor();
            let is_file = item.replacement.starts_with('@');
            if is_file {
                self.editor.replace_range(item.start, cursor, "");
                let trimmed = item.replacement.trim_end().to_string();
                let placeholder = format!("[{trimmed}]");
                self.editor.insert_reference_block(placeholder, trimmed);
                if item.replacement.ends_with(' ') {
                    self.editor.insert_char(' ');
                }
            } else {
                self.editor
                    .replace_range(item.start, cursor, &item.replacement);
            }
        }
    }

    pub(crate) fn history_prev(&mut self) {
        self.editor.history_prev();
        self.refresh_suggestions();
    }

    pub(crate) fn history_next(&mut self) {
        self.editor.history_next();
        self.refresh_suggestions();
    }
}
