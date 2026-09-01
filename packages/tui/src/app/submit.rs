use piko_protocol::Command;

use crate::{
    app::{AppState, QueueStatus, command_id, effect::Effect},
    features::guidance_row::binding_hint,
    features::notifications::NotificationLevel,
    input::command::CommandId,
};

#[derive(Clone, Copy)]
enum Delivery {
    /// FollowUp: start when idle, queue when busy.
    FollowUp,
    /// Steer: inject into the active root only.
    Steer,
}

impl AppState {
    pub(crate) fn submit(&mut self) -> Vec<Effect> {
        if self.viewed_agent_is_busy() {
            self.submit_delivery(Delivery::Steer)
        } else {
            self.submit_delivery(Delivery::FollowUp)
        }
    }

    pub(crate) fn submit_follow_up(&mut self) -> Vec<Effect> {
        self.submit_delivery(Delivery::FollowUp)
    }

    pub(crate) fn submit_steer(&mut self) -> Vec<Effect> {
        self.submit_delivery(Delivery::Steer)
    }

    pub(crate) fn dequeue_follow_up(&mut self) -> Vec<Effect> {
        let Some(agent_instance_id) = self.agent_panel.active_agent_instance_id.clone() else {
            self.status = "no agent selected".to_string();
            return Vec::new();
        };
        let Some(input) = self
            .session
            .agent_work
            .get(&agent_instance_id)
            .and_then(|work| work.queued_inputs.last())
            .cloned()
        else {
            self.status = "no queued follow-up".to_string();
            return Vec::new();
        };
        if !self.editor.is_empty() {
            self.status = "composer is not empty".to_string();
            self.notify(NotificationLevel::Warning, "composer is not empty");
            return Vec::new();
        }

        self.editor
            .restore_content(&piko_protocol::MessageContent::String(input.preview));
        self.refresh_suggestions();
        if let Some(session_id) = self.session.id.clone() {
            self.status = "follow-up restored".to_string();
            return vec![Effect::send(Command::AgentInputCancel {
                command_id: command_id(),
                session_id,
                agent_instance_id,
                input_id: input.input_id,
            })];
        }
        Vec::new()
    }

    pub fn viewed_agent_is_busy(&self) -> bool {
        let Some(agent_instance_id) = self.agent_panel.active_agent_instance_id.as_deref() else {
            return false;
        };
        self.session
            .agent_work
            .get(agent_instance_id)
            .is_some_and(|work| work.active_work.is_some())
    }

    pub fn viewed_agent_is_running(&self) -> bool {
        let Some(agent_instance_id) = self.agent_panel.active_agent_instance_id.as_deref() else {
            return false;
        };
        self.session
            .agent_work
            .get(agent_instance_id)
            .is_some_and(|work| work.foreground.is_busy())
    }

    pub fn queue_summary(&self) -> QueueStatus {
        self.queue_status.clone()
    }

    fn submit_delivery(&mut self, delivery: Delivery) -> Vec<Effect> {
        let submitted_draft = self.editor.snapshot_draft();
        let Some(submission) = self.editor.take_submission() else {
            return Vec::new();
        };
        self.refresh_suggestions();
        if let piko_protocol::MessageContent::String(text) = &submission.content
            && text.starts_with('/')
        {
            return self.intercept_slash(text.clone(), submitted_draft);
        }
        self.dispatch_content_or_restore(
            submission.content,
            submission.display_text,
            submitted_draft,
            delivery,
        )
    }

    fn intercept_slash(
        &mut self,
        text: String,
        submitted_draft: crate::features::editor::state::EditorDraft,
    ) -> Vec<Effect> {
        if let Some(slash_effects) = self.try_slash_command(&text) {
            return slash_effects;
        }
        self.editor.restore_draft(submitted_draft);
        self.status = format!("Unknown slash command: {text}");
        self.notify(
            NotificationLevel::Error,
            format!("Unknown slash command: {text}"),
        );
        Vec::new()
    }

    fn dispatch_content_or_restore(
        &mut self,
        content: piko_protocol::MessageContent,
        display_text: String,
        submitted_draft: crate::features::editor::state::EditorDraft,
        delivery: Delivery,
    ) -> Vec<Effect> {
        if self.session.id.is_none() && matches!(delivery, Delivery::Steer) {
            self.editor.restore_draft(submitted_draft);
            return self.reject_steer_idle();
        }
        if self.session.id.is_none() {
            return self.queue_until_session(content, submitted_draft);
        }
        if self.agent_panel.active_agent_instance_id.is_none() {
            self.editor.restore_draft(submitted_draft);
            self.status = "no agent selected".to_string();
            self.notify(NotificationLevel::Error, "no agent selected");
            return Vec::new();
        }
        self.dispatch_content(content, display_text, submitted_draft, delivery)
    }

    fn dispatch_content(
        &mut self,
        content: piko_protocol::MessageContent,
        display_text: String,
        submitted_draft: crate::features::editor::state::EditorDraft,
        delivery: Delivery,
    ) -> Vec<Effect> {
        let Some(session_id) = self.session.id.clone() else {
            return self.queue_until_session(content, submitted_draft);
        };
        let Some(agent_instance_id) = self.agent_panel.active_agent_instance_id.clone() else {
            self.editor.restore_content(&content);
            self.status = "no agent selected".to_string();
            self.notify(NotificationLevel::Error, "no agent selected");
            return Vec::new();
        };
        match delivery {
            Delivery::Steer if !self.viewed_agent_is_busy() => {
                self.editor.restore_draft(submitted_draft);
                self.reject_steer_idle()
            }
            Delivery::Steer => {
                self.send_steer(session_id, agent_instance_id, content, submitted_draft)
            }
            Delivery::FollowUp => {
                let record = self.viewed_agent_is_busy();
                self.send_chat(
                    session_id,
                    agent_instance_id,
                    content,
                    display_text,
                    submitted_draft,
                    record,
                )
            }
        }
    }

    fn send_chat(
        &mut self,
        session_id: String,
        agent_instance_id: String,
        content: piko_protocol::MessageContent,
        _display_text: String,
        submitted_draft: crate::features::editor::state::EditorDraft,
        record_follow_up: bool,
    ) -> Vec<Effect> {
        let target_name = self.agent_label(&agent_instance_id);
        let submit_command_id = command_id();
        self.session.pending.track(
            submit_command_id.clone(),
            super::pending::PendingCommandKind::AgentInputSubmit,
        );
        self.session.pending_submissions.insert(
            submit_command_id.clone(),
            super::pending::PendingSubmissionUi {
                draft: submitted_draft,
            },
        );
        let status = if record_follow_up {
            format!("queued for {target_name}")
        } else {
            format!("submitted to {target_name}")
        };
        self.status = status;
        let command = Command::AgentInputSubmit {
            input: user_agent_input(
                &submit_command_id,
                session_id,
                agent_instance_id,
                piko_protocol::AgentInputDelivery::FollowUp,
                content,
            ),
            command_id: submit_command_id,
        };
        vec![Effect::send(command)]
    }

    fn send_steer(
        &mut self,
        session_id: String,
        agent_instance_id: String,
        content: piko_protocol::MessageContent,
        submitted_draft: crate::features::editor::state::EditorDraft,
    ) -> Vec<Effect> {
        let target_name = self.agent_label(&agent_instance_id);
        self.status = format!("steered {target_name}");
        let submit_command_id = command_id();
        self.session.pending.track(
            submit_command_id.clone(),
            super::pending::PendingCommandKind::AgentInputSubmit,
        );
        self.session.pending_submissions.insert(
            submit_command_id.clone(),
            super::pending::PendingSubmissionUi {
                draft: submitted_draft,
            },
        );
        let command = Command::AgentInputSubmit {
            input: user_agent_input(
                &submit_command_id,
                session_id,
                agent_instance_id,
                piko_protocol::AgentInputDelivery::SteerActive,
                content,
            ),
            command_id: submit_command_id,
        };
        vec![Effect::send(command)]
    }

    fn queue_until_session(
        &mut self,
        content: piko_protocol::MessageContent,
        draft: crate::features::editor::state::EditorDraft,
    ) -> Vec<Effect> {
        self.session.pending_submit_content = Some(content);
        self.session.pending_submit_draft = Some(draft);
        if !self.session.initializing {
            self.begin_session_hydration(None);
            let create_id = command_id();
            self.session.pending.track(
                create_id.clone(),
                super::pending::PendingCommandKind::SessionCreate,
            );
            self.status = "creating session...".to_string();
            vec![Effect::send(Command::SessionCreate {
                command_id: create_id,
                cwd: self.cwd.to_string_lossy().into_owned(),
            })]
        } else {
            self.status = "waiting for session...".to_string();
            Vec::new()
        }
    }

    fn reject_steer_idle(&mut self) -> Vec<Effect> {
        self.status = "agent is not running".to_string();
        let message = binding_hint(self, CommandId::EditorFollowUp).map_or_else(
            || "agent is not running".to_string(),
            |key| format!("agent is not running; use {key} to queue"),
        );
        self.notify(NotificationLevel::Error, message);
        Vec::new()
    }

    fn agent_label(&self, agent_instance_id: &str) -> String {
        self.agent_panel
            .agents()
            .iter()
            .find(|agent| agent.agent_instance_id == agent_instance_id)
            .map(|agent| agent.name.clone())
            .unwrap_or_else(|| agent_instance_id.to_string())
    }
}

pub(super) fn user_agent_input(
    request_id: &str,
    session_id: String,
    agent_instance_id: String,
    delivery: piko_protocol::AgentInputDelivery,
    content: piko_protocol::MessageContent,
) -> piko_protocol::AgentInput {
    piko_protocol::AgentInput {
        input_id: format!("input_{}", uuid::Uuid::new_v4()),
        request_id: request_id.to_string(),
        session_id,
        agent_instance_id,
        origin: piko_protocol::AgentInputOrigin::User,
        delivery,
        content,
        submitted_at: chrono::Utc::now().timestamp_millis(),
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    }
}
