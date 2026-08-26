use piko_protocol::{Command, TurnStatus};

use crate::{
    app::{AppState, FollowUpUi, QueueStatus, command_id, effect::Effect},
    features::notifications::NotificationLevel,
};

#[derive(Clone, Copy)]
enum Delivery {
    /// `ChatSubmit` / FollowUp: start when idle, queue when busy.
    FollowUp,
    /// `QueueSteer`: inject into the running turn only.
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
        let Some(index) = self
            .session
            .follow_ups
            .iter()
            .rposition(|item| item.agent_instance_id == agent_instance_id)
        else {
            self.status = "no queued follow-up".to_string();
            return Vec::new();
        };
        if !self.editor.is_empty() {
            self.status = "composer is not empty".to_string();
            self.notify(NotificationLevel::Warning, "composer is not empty");
            return Vec::new();
        }

        let content = self.session.follow_ups[index].content.clone();
        let turn_id = self.session.follow_ups[index].turn_id.clone();
        self.editor.restore_content(&content);
        self.refresh_suggestions();
        if let (Some(session_id), Some(turn_id)) = (self.session.id.clone(), turn_id) {
            self.session.follow_ups.remove(index);
            self.status = "follow-up restored".to_string();
            return vec![Effect::send(Command::TurnCancel {
                command_id: command_id(),
                session_id,
                turn_id,
            })];
        }
        self.session.follow_ups[index].cancel_when_queued = true;
        self.status = "follow-up restored".to_string();
        Vec::new()
    }

    pub fn viewed_agent_is_busy(&self) -> bool {
        let Some(agent_instance_id) = self.agent_panel.active_agent_instance_id.as_deref() else {
            return false;
        };
        matches!(
            self.session
                .active_turns
                .get(agent_instance_id)
                .map(|turn| turn.status),
            Some(TurnStatus::Running | TurnStatus::WaitingForApproval | TurnStatus::Cancelling)
        )
    }

    pub fn queue_summary(&self) -> QueueStatus {
        let mut summary = self.queue_status.clone();
        let local = self.session.follow_ups.len() as u32;
        if local > summary.follow_up_count {
            summary.follow_up_count = local;
        }
        summary
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
        display_text: String,
        submitted_draft: crate::features::editor::state::EditorDraft,
        record_follow_up: bool,
    ) -> Vec<Effect> {
        let target_name = self.agent_label(&agent_instance_id);
        let submit_command_id = command_id();
        if record_follow_up {
            self.session.follow_ups.push(FollowUpUi {
                command_id: Some(submit_command_id.clone()),
                agent_instance_id: agent_instance_id.clone(),
                text: display_text,
                content: content.clone(),
                turn_id: None,
                cancel_when_queued: false,
            });
        }
        self.session.pending.track(
            submit_command_id.clone(),
            super::pending::PendingCommandKind::ChatSubmit,
        );
        self.session.pending_submissions.insert(
            submit_command_id.clone(),
            super::pending::PendingSubmissionUi {
                draft: submitted_draft,
                optimistic_follow_up: record_follow_up,
            },
        );
        let status = if record_follow_up {
            format!("queued for {target_name}")
        } else {
            format!("submitted to {target_name}")
        };
        self.status = status;
        let command = match content {
            piko_protocol::MessageContent::String(text) => Command::ChatSubmit {
                command_id: submit_command_id,
                session_id,
                target_agent_instance_id: agent_instance_id,
                text,
            },
            content => Command::ChatSubmitMessage {
                command_id: submit_command_id,
                session_id,
                target_agent_instance_id: agent_instance_id,
                content,
            },
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
            super::pending::PendingCommandKind::ChatSubmit,
        );
        self.session.pending_submissions.insert(
            submit_command_id.clone(),
            super::pending::PendingSubmissionUi {
                draft: submitted_draft,
                optimistic_follow_up: false,
            },
        );
        let command = match content {
            piko_protocol::MessageContent::String(message) => Command::QueueSteer {
                command_id: submit_command_id,
                session_id,
                agent_instance_id,
                message,
            },
            content => Command::QueueSteerMessage {
                command_id: submit_command_id,
                session_id,
                agent_instance_id,
                content,
            },
        };
        vec![Effect::send(command)]
    }

    fn queue_until_session(
        &mut self,
        content: piko_protocol::MessageContent,
        draft: crate::features::editor::state::EditorDraft,
    ) -> Vec<Effect> {
        self.session.pending_turn_content = Some(content);
        self.session.pending_turn_draft = Some(draft);
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
        self.notify(
            NotificationLevel::Error,
            "agent is not running; use Alt+Enter to queue",
        );
        Vec::new()
    }

    pub(crate) fn bind_follow_up_queued(
        &mut self,
        session_id: &str,
        agent_instance_id: &str,
        turn_id: &str,
    ) -> Option<Effect> {
        let item =
            self.session.follow_ups.iter_mut().find(|item| {
                item.agent_instance_id == agent_instance_id && item.turn_id.is_none()
            })?;
        item.turn_id = Some(turn_id.to_string());
        if !item.cancel_when_queued {
            return None;
        }
        self.session
            .follow_ups
            .retain(|item| item.turn_id.as_deref() != Some(turn_id));
        Some(Effect::send(Command::TurnCancel {
            command_id: command_id(),
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
        }))
    }

    pub(crate) fn drop_follow_up_turn(&mut self, turn_id: &str) {
        self.session
            .follow_ups
            .retain(|item| item.turn_id.as_deref() != Some(turn_id));
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
