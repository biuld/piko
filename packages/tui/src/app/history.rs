use piko_protocol::Command;

use super::{AppState, SurfaceId, command::SurfaceAction, command_id, effect::Effect};

impl AppState {
    pub(super) fn history_request(&mut self, mut command: Command) -> Vec<Effect> {
        // Request ownership survives panel teardown, including generic SessionList replies.
        match &mut command {
            Command::SessionList { command_id, .. }
            | Command::SessionHistoryOverviewGet { command_id, .. }
            | Command::SessionHistoryAgentStreamGet { command_id, .. }
            | Command::SessionHistoryLaneGet { command_id, .. }
            | Command::SessionHistoryItemGet { command_id, .. } => {
                command_id.insert_str(0, "history:")
            }
            _ => unreachable!("history query command"),
        }
        self.history
            .pending_commands
            .push(command.command_id().to_string());
        if matches!(command, Command::SessionHistoryItemGet { .. }) {
            self.history.detail_loading = true;
            self.history.detail_error = None;
        } else {
            self.history.loading = true;
            self.history.error = None;
        }
        vec![Effect::send(command)]
    }

    pub(super) fn open_history(&mut self, requested: Option<String>) -> Vec<Effect> {
        let Some(session_id) = requested.or_else(|| self.session.id.clone()) else {
            return self.choose_history_session();
        };
        self.history.begin(session_id.clone());
        self.push_surface(SurfaceId::History);
        self.status = format!("loading trajectory for {session_id}");
        self.history_request(Command::SessionHistoryOverviewGet {
            command_id: command_id(),
            session_id,
            after_cursor: None,
            limit: None,
        })
    }

    /// Fetch the selected agent's stream and lane strip at the overview's
    /// published revision.
    pub(super) fn fetch_history_agent(&mut self) -> Vec<Effect> {
        let (Some(session_id), Some(overview)) = (
            self.history.session_id.clone(),
            self.history.overview.as_ref(),
        ) else {
            return Vec::new();
        };
        let Some(agent_id) = self.history.agent_id.clone() else {
            return Vec::new();
        };
        let revision = overview.revision;
        let mut effects = self.history_request(Command::SessionHistoryAgentStreamGet {
            command_id: command_id(),
            session_id: session_id.clone(),
            agent_instance_id: agent_id.clone(),
            expected_revision: revision,
            after_cursor: None,
            limit: Some(100),
        });
        effects.extend(self.history_request(Command::SessionHistoryLaneGet {
            command_id: command_id(),
            session_id,
            agent_instance_id: agent_id,
            expected_revision: revision,
        }));
        effects
    }

    pub(super) fn select_history_agent(&mut self, index: usize) -> Vec<Effect> {
        if self.history.choosing_session {
            return Vec::new();
        }
        let Some(agent) = self
            .history
            .overview
            .as_ref()
            .and_then(|overview| overview.agents.get(index))
        else {
            return Vec::new();
        };
        self.history.agent_choosing = false;
        if self.history.agent_id.as_deref() == Some(agent.agent_instance_id.as_str()) {
            return Vec::new();
        }
        self.history.select_agent(agent.agent_instance_id.clone());
        self.fetch_history_agent()
    }

    /// `a`: rotate to the next agent stream in overview order.
    pub(super) fn cycle_history_agent(&mut self) -> Vec<Effect> {
        let Some(overview) = self.history.overview.as_ref() else {
            return Vec::new();
        };
        if overview.agents.is_empty() {
            return Vec::new();
        }
        let current = overview
            .agents
            .iter()
            .position(|agent| Some(&agent.agent_instance_id) == self.history.agent_id.as_ref());
        let next = current
            .map(|index| (index + 1) % overview.agents.len())
            .unwrap_or(0);
        self.select_history_agent(next)
    }

    pub(super) fn dispatch_history(&mut self, action: SurfaceAction) -> Vec<Effect> {
        match action {
            SurfaceAction::OpenHistory(requested) => self.open_history(requested),
            SurfaceAction::HistoryFilter => {
                self.history.filter_editing = true;
                self.history.clear_detail();
                self.history.pending_commands.clear();
                Vec::new()
            }
            SurfaceAction::HistorySelectAgent(index) => {
                if index == usize::MAX {
                    self.cycle_history_agent()
                } else {
                    self.select_history_agent(index)
                }
            }
            SurfaceAction::HistoryDetailTab(index) => {
                if index == usize::MAX {
                    self.history.cycle_detail_tab();
                } else {
                    self.history.select_detail_tab(index);
                }
                Vec::new()
            }
            SurfaceAction::HistoryRefresh => self.open_history(self.history.session_id.clone()),
            SurfaceAction::HistoryChooseSession => {
                if self.history.overview.is_some() && !self.history.choosing_session {
                    // Inside an inspected session, `s` opens the agent selector.
                    self.history.start_agent_choosing();
                    Vec::new()
                } else {
                    self.choose_history_session()
                }
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn choose_history_session(&mut self) -> Vec<Effect> {
        self.history = Default::default();
        self.history.choosing_session = true;
        self.push_surface(SurfaceId::History);
        self.history_request(Command::SessionList {
            command_id: command_id(),
            scope: piko_protocol::SessionListScope::All,
            cwd: None,
        })
    }

    pub(super) fn history_next_page(&mut self) -> Vec<Effect> {
        if self.history.loading
            || self.history.detail_loading
            || self.history.error.is_some()
            || self.history.active_pane == crate::ui::components::split_pane::PaneSide::Second
            || self.history.choosing_session
            || self.history.agent_choosing
            || self.history.shows_detail_only()
            || self.history.selected.saturating_add(3) < self.history.row_count()
        {
            return Vec::new();
        }
        let Some(overview) = &self.history.overview else {
            return Vec::new();
        };
        let Some(page) = self.history.stream.as_ref() else {
            return Vec::new();
        };
        let Some(cursor) = page.next_cursor.clone() else {
            return Vec::new();
        };
        self.history_request(Command::SessionHistoryAgentStreamGet {
            command_id: command_id(),
            session_id: page.session_id.clone(),
            agent_instance_id: page.agent_instance_id.clone(),
            expected_revision: overview.revision,
            after_cursor: Some(cursor),
            limit: Some(100),
        })
    }
}
