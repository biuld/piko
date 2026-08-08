use super::*;

impl AppState {
    pub(super) fn apply_turn_lifecycle(&mut self, event: piko_protocol::TurnEvent) -> Vec<Effect> {
        let effects = Vec::new();
        match event {
            piko_protocol::TurnEvent::Queued {
                session_id,
                turn_id,
                agent_instance_id,
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                self.session
                    .pending
                    .clear_kind(crate::app::pending::PendingCommandKind::ChatSubmit);
                self.session.active_turns.insert(
                    agent_instance_id.clone(),
                    crate::app::ActiveTurnUi {
                        turn_id: turn_id.clone(),
                        status: piko_protocol::TurnStatus::Queued,
                    },
                );
                self.status = format!("turn {turn_id} queued ({agent_instance_id})");
            }
            piko_protocol::TurnEvent::Started {
                session_id,
                turn_id,
                agent_instance_id,
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                self.session
                    .pending
                    .clear_kind(crate::app::pending::PendingCommandKind::ChatSubmit);
                self.session.active_turns.insert(
                    agent_instance_id.clone(),
                    crate::app::ActiveTurnUi {
                        turn_id: turn_id.clone(),
                        status: piko_protocol::TurnStatus::Running,
                    },
                );
                self.status = format!("turn {turn_id} running ({agent_instance_id})");
            }
            piko_protocol::TurnEvent::Completed {
                session_id,
                turn_id,
                agent_instance_id,
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                if self
                    .session
                    .active_turns
                    .get(&agent_instance_id)
                    .is_some_and(|active| active.turn_id == turn_id)
                {
                    self.session.active_turns.remove(&agent_instance_id);
                }
                // Usage chrome is host-authoritative via Event::Usage only.
                self.last_turn_id = Some(turn_id.clone());
                self.status = format!("turn {turn_id} completed");
            }
            piko_protocol::TurnEvent::Failed {
                session_id,
                turn_id,
                agent_instance_id,
                error,
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                if self
                    .session
                    .active_turns
                    .get(&agent_instance_id)
                    .is_some_and(|active| active.turn_id == turn_id)
                {
                    self.session.active_turns.remove(&agent_instance_id);
                }
                self.last_turn_id = Some(turn_id.clone());
                self.status = format!("turn {turn_id} failed");
                self.push_error(error);
            }
            piko_protocol::TurnEvent::Cancelled {
                session_id,
                turn_id,
                agent_instance_id,
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                if self
                    .session
                    .active_turns
                    .get(&agent_instance_id)
                    .is_some_and(|active| active.turn_id == turn_id)
                {
                    self.session.active_turns.remove(&agent_instance_id);
                }
                self.last_turn_id = Some(turn_id.clone());
                self.status = format!("turn {turn_id} cancelled");
            }
        }
        effects
    }

    pub(super) fn apply_approval(&mut self, event: piko_protocol::ApprovalEvent) -> Vec<Effect> {
        let effects = Vec::new();
        match event {
            piko_protocol::ApprovalEvent::Requested {
                session_id,
                agent_instance_id,
                approval_id,
                tool_name,
                tool_args,
                prompt,
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                self.approvals.push(PendingApproval {
                    id: approval_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    tool_name: tool_name.clone(),
                    args: tool_args,
                    prompt,
                });
                if let Some(agent) = self
                    .agent_panel
                    .list
                    .items
                    .iter_mut()
                    .find(|a| a.agent_instance_id == agent_instance_id)
                {
                    agent.activity = piko_protocol::AgentActivity::WaitingForApproval;
                }
                self.status = format!("approval requested for {tool_name}");
                self.notify(
                    NotificationLevel::Warning,
                    format!("approval requested for {tool_name}"),
                );
                if self.focus_manager.active_mode() != AppMode::Surface(SurfaceId::Approval) {
                    self.push_surface(SurfaceId::Approval);
                }
            }
            piko_protocol::ApprovalEvent::Resolved {
                session_id,
                approval_id,
                decision,
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                let agent_instance_id = self
                    .approvals
                    .pending
                    .iter()
                    .find(|a| a.id == approval_id)
                    .map(|a| a.agent_instance_id.clone());
                self.approvals.resolve(&approval_id);
                if let Some(agent_id) = agent_instance_id {
                    let still_blocked = self
                        .approvals
                        .pending
                        .iter()
                        .any(|a| a.agent_instance_id == agent_id);
                    if !still_blocked
                        && let Some(agent) = self
                            .agent_panel
                            .list
                            .items
                            .iter_mut()
                            .find(|a| a.agent_instance_id == agent_id)
                    {
                        agent.activity = if self.session.active_turns.contains_key(&agent_id) {
                            piko_protocol::AgentActivity::Running
                        } else {
                            piko_protocol::AgentActivity::Idle
                        };
                    }
                }
                self.status = format!("approval {approval_id} resolved: {decision:?}");
                if self.approvals.is_empty()
                    && self.focus_manager.active_mode() == AppMode::Surface(SurfaceId::Approval)
                {
                    self.pop_focus();
                }
                if self.approvals.is_empty()
                    && !self.interactions.is_empty()
                    && self.focus_manager.active_mode()
                        != AppMode::Surface(SurfaceId::ToolInteraction)
                {
                    self.push_surface(SurfaceId::ToolInteraction);
                }
            }
        }
        effects
    }

    pub(super) fn apply_queue(&mut self, event: piko_protocol::QueueEvent) -> Vec<Effect> {
        let effects = Vec::new();
        match event {
            piko_protocol::QueueEvent::Updated {
                session_id,
                steer_count,
                follow_up_count,
                next_turn_count,
                steer_preview,
                follow_up_preview,
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                self.queue_status = QueueStatus {
                    steer_count,
                    follow_up_count,
                    next_turn_count,
                    steer_preview,
                    follow_up_preview,
                };
                self.status = format!(
                    "queue steer={steer_count} follow_up={follow_up_count} next_turn={next_turn_count}"
                );
            }
        }
        effects
    }

    pub(super) fn apply_auth(&mut self, event: piko_protocol::AuthEvent) -> Vec<Effect> {
        let effects = Vec::new();
        match event {
            piko_protocol::AuthEvent::LoginDeviceCode {
                provider,
                user_code,
                verification_uri,
            } => self.push(TimelineEntry::System(format!(
                "{provider} login: open {verification_uri} and enter {user_code}"
            ))),
            piko_protocol::AuthEvent::LoginSuccess { provider } => {
                self.push(TimelineEntry::System(format!("{provider} login succeeded")));
            }
            piko_protocol::AuthEvent::LoginFailed { provider, error } => {
                self.push_error(format!("{provider} login failed: {error}"));
            }
            piko_protocol::AuthEvent::LoggedOut { provider } => {
                self.push(TimelineEntry::System(format!("{provider} logged out")));
            }
        }
        effects
    }

    pub(super) fn apply_model(&mut self, event: piko_protocol::ModelEvent) -> Vec<Effect> {
        let effects = Vec::new();
        match event {
            piko_protocol::ModelEvent::ConfigChanged {
                model_id,
                provider,
                thinking_level,
                context_window,
                ..
            } => {
                self.model.active_model_id = if model_id.is_empty() {
                    None
                } else {
                    Some(model_id.clone())
                };
                self.model.active_provider = if provider.is_empty() {
                    None
                } else {
                    Some(provider.clone())
                };
                if let Some(level) = thinking_level {
                    self.model.active_thinking_level = Some(level.as_str().to_string());
                } else {
                    self.model.active_thinking_level = Some("off".to_string());
                }
                self.model.host_context_window = context_window.filter(|w| *w > 0);
                if self.model.active_model_id.is_some() && self.model.active_provider.is_some() {
                    self.status = format!("model {provider}/{model_id}");
                } else {
                    self.status = "no model active".to_string();
                }
            }
        }
        effects
    }

    pub(super) fn apply_usage(&mut self, event: piko_protocol::UsageEvent) -> Vec<Effect> {
        let effects = Vec::new();
        match event {
            piko_protocol::UsageEvent::Updated {
                session_id,
                used,
                size,
                cumulative,
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                if used > 0 {
                    self.session.last_context_tokens = Some(used);
                }
                if let Some(window) = size.filter(|w| *w > 0) {
                    self.model.host_context_window = Some(window);
                }
                if let Some(cumulative) = cumulative {
                    // Host ledger is authoritative when present.
                    self.session.cumulative_usage = Some(cumulative);
                }
            }
        }
        effects
    }
}
