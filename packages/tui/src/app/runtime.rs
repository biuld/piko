use piko_protocol::Command;

use crate::{
    features::{notifications::NotificationLevel, timeline::TimelineEntry},
    host::HostLine,
};

use super::{AppState, effect, pending};

impl AppState {
    pub fn update(&mut self, msg: effect::Msg) -> Vec<effect::Effect> {
        match msg {
            effect::Msg::Action(action) => self.dispatch(action),
            effect::Msg::HostLine(line) => self.handle_host_line(line),
            effect::Msg::Tick => {
                self.last_tick = std::time::Instant::now();
                self.spinner_frame = self.spinner_frame.wrapping_add(1);
                self.timeline.viewport.apply_metrics();
                Vec::new()
            }
        }
    }

    pub fn handle_host_line(&mut self, line: HostLine) -> Vec<effect::Effect> {
        match line {
            HostLine::Message(message) => match *message {
                piko_protocol::ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(piko_protocol::CommandResult::Empty),
                } => {
                    let _ = self.session.pending.take(&command_id);
                    self.status = format!("done {command_id}");
                    Vec::new()
                }
                piko_protocol::ServerMessage::CommandResponse {
                    command_id,
                    result: Err(reason),
                } => self.handle_command_error(command_id, reason),
                message => self.apply_event(message),
            },
            HostLine::DecodeError(error) => {
                self.notify(NotificationLevel::Error, error.clone());
                self.push(TimelineEntry::Error(error));
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
            | Some(pending::PendingCommandKind::BootstrapCatalog) => {
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
                    self.clear_session_view();
                    if let Some(text) = self.session.pending_turn_text.take()
                        && self.editor.is_empty()
                    {
                        self.editor.insert_paste(&text, &self.tui_config.editor);
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
                // Turn lifecycle is authoritative. A rejected submit never
                // creates an active Turn, so there is no optimistic run state
                // to clear here.
            }
            Some(pending::PendingCommandKind::SessionDelete) => {
                self.session.pending.delete_session_id = None;
                self.status = format!("delete failed: {reason}");
            }
            None => {}
        }
        self.notify(
            NotificationLevel::Error,
            format!("rejected {command_id}: {reason}"),
        );
        self.push(TimelineEntry::Error(reason));
        effects
    }

    pub fn push(&mut self, entry: TimelineEntry) {
        self.timeline.push(entry);
    }

    pub fn push_error(&mut self, message: String) {
        self.notify(NotificationLevel::Error, message.clone());
        self.push(TimelineEntry::Error(message));
    }

    pub fn notify(&mut self, level: NotificationLevel, message: impl Into<String>) {
        self.notifications.push(level, message);
    }
}
