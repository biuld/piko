use piko_protocol::Command;

use crate::{
    features::{notifications::NotificationLevel, timeline::TimelineEntry},
    host::HostLine,
};

use super::{AppState, SurfaceId, effect, pending};

impl AppState {
    pub fn update(&mut self, msg: effect::Msg) -> Vec<effect::Effect> {
        let is_tick = matches!(&msg, effect::Msg::Tick);
        let effects = match msg {
            effect::Msg::Action(action) => self.dispatch(action),
            effect::Msg::HostLine(line) => self.handle_host_line(line),
            effect::Msg::Tick => {
                let now = std::time::Instant::now();
                self.last_tick = now;
                self.spinner_frame = self.spinner_frame.wrapping_add(1);
                let text = self.editor.text();
                self.editor
                    .auto_complete
                    .poll_file_results(&text, self.editor.cursor());
                self.timeline_mut().viewport.apply_metrics();
                Vec::new()
            }
        };
        if !is_tick {
            self.reconcile_thought_inspector();
        } else {
            self.advance_thought_inspector(self.last_tick);
            self.reconcile_thought_inspector();
        }
        effects
    }

    pub fn begin_host_batch(&mut self) {
        self.timelines.begin_projection_batch();
    }

    pub fn end_host_batch(&mut self) {
        self.timelines.end_projection_batch();
    }

    pub fn handle_host_line(&mut self, line: HostLine) -> Vec<effect::Effect> {
        match line {
            HostLine::Message(message) => match *message {
                piko_protocol::ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(piko_protocol::CommandResult::Empty),
                } => {
                    self.session.pending_submissions.remove(&command_id);
                    self.status = if self.session.pending.take(&command_id)
                        == Some(pending::PendingCommandKind::UsageRefresh)
                    {
                        "usage refreshed".to_string()
                    } else {
                        format!("done {command_id}")
                    };
                    Vec::new()
                }
                piko_protocol::ServerMessage::CommandResponse {
                    command_id,
                    result: Err(reason),
                } => self.handle_command_error(command_id, reason),
                message => self.apply_event(message),
            },
            HostLine::DecodeError(error) => {
                self.notify(NotificationLevel::Error, error);
                Vec::new()
            }
            HostLine::Closed => {
                self.status = "hostd closed stdout".to_string();
                self.notify(NotificationLevel::Warning, "hostd closed stdout");
                Vec::new()
            }
        }
    }

    fn handle_command_error(&mut self, command_id: String, reason: String) -> Vec<effect::Effect> {
        self.status = format!("rejected {command_id}");
        let pending = self.session.pending.take(&command_id);
        let mut effects = Vec::new();
        match pending {
            Some(pending::PendingCommandKind::BootstrapConfig)
            | Some(pending::PendingCommandKind::BootstrapCatalog)
            | Some(pending::PendingCommandKind::BootstrapModels) => {
                self.finish_bootstrap_command(&command_id);
            }
            Some(pending::PendingCommandKind::SessionCreate)
            | Some(pending::PendingCommandKind::SessionOpen) => {
                self.sessions.loading = false;
                self.sessions.error = Some(reason.clone());
                if let Some(previous) = self.session.previous_live_id.take() {
                    self.session.opening_id = Some(previous.clone());
                    self.session.initializing = true;
                    effects.push(effect::Effect::send(Command::StateSnapshot {
                        command_id: super::command_id(),
                        session_id: previous,
                    }));
                } else {
                    let pending_draft = self.session.pending_turn_draft.take();
                    let pending_content = self.session.pending_turn_content.take();
                    self.clear_session_view();
                    if let Some(draft) = pending_draft
                        && self.editor.is_empty()
                    {
                        self.editor.restore_draft(draft);
                    } else if let Some(content) = pending_content
                        && self.editor.is_empty()
                    {
                        self.editor.restore_content(&content);
                    }
                }
            }
            Some(pending::PendingCommandKind::SessionList) => {
                self.sessions.loading = false;
                self.sessions.error = Some(reason.clone());
                if self.session.continue_requested {
                    self.session.continue_requested = false;
                    self.session.initializing = false;
                    if self.session.shell_ready && self.session.id.is_none() {
                        self.agent_panel.mark_hydrated();
                    }
                }
            }
            Some(pending::PendingCommandKind::ChatSubmit) => {
                if let Some(submission) = self.session.pending_submissions.remove(&command_id) {
                    if submission.optimistic_follow_up {
                        self.session
                            .follow_ups
                            .retain(|item| item.command_id.as_deref() != Some(&command_id));
                    }
                    if self.editor.is_empty() {
                        self.editor.restore_draft(submission.draft);
                        self.refresh_suggestions();
                    }
                }
            }
            Some(pending::PendingCommandKind::ModelList) => {
                self.status = format!("model list failed: {reason}");
                if matches!(self.mode(), super::AppMode::Surface(SurfaceId::Models)) {
                    self.pop_focus();
                }
            }
            Some(pending::PendingCommandKind::SessionDelete) => {
                self.session.pending.delete_session_id = None;
                self.status = format!("delete failed: {reason}");
            }
            Some(pending::PendingCommandKind::UsageRefresh) => {
                self.status = format!("usage refresh failed: {reason}");
            }
            None => {}
        }
        self.notify(
            NotificationLevel::Error,
            format!("rejected {command_id}: {reason}"),
        );
        effects
    }

    pub fn push(&mut self, entry: TimelineEntry) {
        self.timeline_mut().push(entry);
    }

    pub fn push_error(&mut self, message: String) {
        self.notify(NotificationLevel::Error, message.clone());
        self.push(TimelineEntry::Error(message));
    }

    pub fn notify(&mut self, level: NotificationLevel, message: impl Into<String>) {
        self.notifications.push(level, message);
    }
}
