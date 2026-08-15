use super::*;

impl AppState {
    pub(super) fn apply_transcript_committed(
        &mut self,
        committed: piko_protocol::TranscriptCommittedEvent,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();
        if !self.accepts_session(&committed.session_id) {
            return effects;
        }
        let agent_instance_id = committed.agent_instance_id.clone();
        let mut consistent = true;
        self.with_agent_timeline(&agent_instance_id, |timeline| {
            consistent = timeline.apply_committed(committed);
        });
        if !consistent && let Some(session_id) = self.session.id.clone() {
            effects.push(Effect::send(Command::StateSnapshot {
                command_id: command_id(),
                session_id,
            }));
        }
        effects
    }

    pub(super) fn apply_stream_item(
        &mut self,
        patch: piko_protocol::StreamItemPatch,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();
        let Some(session_id) = patch.session_id.as_deref() else {
            return effects;
        };
        if !self.accepts_session(session_id) {
            return effects;
        }
        let Some(agent_instance_id) = patch.agent_instance_id.clone() else {
            return effects;
        };
        let mut outcome = piko_client_core::ApplyOutcome::Ignored;
        self.with_agent_timeline(&agent_instance_id, |timeline| {
            outcome = timeline.apply_stream_item(&patch);
        });
        if outcome == piko_client_core::ApplyOutcome::Inconsistent
            && let Some(session_id) = self.session.id.clone()
        {
            effects.push(Effect::send(Command::StateSnapshot {
                command_id: command_id(),
                session_id,
            }));
        }
        effects
    }

    pub(super) fn apply_session_entry_committed(
        &mut self,
        committed: piko_protocol::SessionEntryCommittedEvent,
    ) -> Vec<Effect> {
        if !self.accepts_session(&committed.session_id) {
            return Vec::new();
        }
        let order = self.timeline().components.len() as u64;
        let entry = committed.entry;
        let outcome = self.timelines.apply_session_entry(entry, order);
        if outcome == piko_client_core::ApplyOutcome::Inconsistent {
            vec![Effect::send(Command::StateSnapshot {
                command_id: command_id(),
                session_id: committed.session_id,
            })]
        } else {
            Vec::new()
        }
    }

    pub(super) fn apply_session_reconciled(
        &mut self,
        reconciled: piko_protocol::SessionReconciledEvent,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();
        if self.accepts_reconcile(&reconciled.session_id) {
            let selected_agent_instance_id = reconciled.snapshot.selected_agent_instance_id.clone();
            self.session.id = Some(reconciled.session_id.clone());
            self.session.opening_id = None;
            self.session.previous_live_id = None;
            self.session.initializing = false;
            self.apply_snapshot(reconciled.snapshot);
            self.agent_panel.list.clear();
            for agent in reconciled.agents {
                self.agent_panel
                    .upsert_agent(crate::features::agent_status::AgentEntry {
                        agent_id: agent.agent_id,
                        agent_instance_id: agent.agent_instance_id,
                        name: agent.name,
                        parent_agent_instance_id: agent.parent_agent_instance_id,
                        lifecycle: agent.lifecycle,
                        activity: agent.activity,
                        unread_report_count: agent.unread_report_count,
                        status: agent.status,
                    });
            }
            self.agent_panel.mark_hydrated();
            let active_agent_instance_id = selected_agent_instance_id
                .filter(|selected| {
                    self.agent_panel
                        .agents()
                        .iter()
                        .any(|agent| &agent.agent_instance_id == selected)
                })
                .or_else(|| {
                    self.agent_panel
                        .agents()
                        .iter()
                        .find(|agent| agent.parent_agent_instance_id.is_none())
                        .or_else(|| self.agent_panel.agents().first())
                        .map(|agent| agent.agent_instance_id.clone())
                });
            if let Some(active_agent_instance_id) = active_agent_instance_id {
                self.select_agent_timeline(&active_agent_instance_id);
            }
            if let Some(name) = self.initial_options.session_name.take() {
                effects.push(Effect::send(Command::SessionRename {
                    command_id: command_id(),
                    session_id: reconciled.session_id.clone(),
                    name,
                }));
            }
            if let Some(text) = self.session.pending_turn_text.take() {
                if let Some(target_agent_instance_id) =
                    self.agent_panel.active_agent_instance_id.clone()
                {
                    let submit_command_id = command_id();
                    self.session.pending.track(
                        submit_command_id.clone(),
                        crate::app::pending::PendingCommandKind::ChatSubmit,
                    );
                    effects.push(Effect::send(Command::ChatSubmit {
                        command_id: submit_command_id,
                        session_id: reconciled.session_id,
                        target_agent_instance_id,
                        text,
                    }));
                    self.status = "submitted first message".to_string();
                } else {
                    self.session.pending_turn_text = Some(text);
                    self.status = "waiting for root agent".to_string();
                }
            }
        }
        effects
    }

    pub(super) fn apply_session_cleared(
        &mut self,
        cleared: piko_protocol::SessionClearedEvent,
    ) -> Vec<Effect> {
        let effects = Vec::new();
        if self.session.id.as_deref() == Some(&cleared.previous_session_id) {
            self.clear_session_view();
            self.session
                .pending
                .clear_kind(crate::app::pending::PendingCommandKind::SessionDelete);
            self.session.pending.delete_session_id = None;
            self.status = "session deleted".to_string();
            self.notify(NotificationLevel::Warning, "session deleted");
            self.clear_focus();
        }
        effects
    }

    pub(super) fn apply_agent_changed(&mut self, agent: piko_protocol::AgentInfo) -> Vec<Effect> {
        let effects = Vec::new();
        if !self.accepts_session(&agent.session_id) {
            return effects;
        }
        self.agent_panel
            .upsert_agent(crate::features::agent_status::AgentEntry {
                agent_id: agent.agent_id,
                agent_instance_id: agent.agent_instance_id,
                name: agent.name,
                parent_agent_instance_id: agent.parent_agent_instance_id,
                lifecycle: agent.lifecycle,
                activity: agent.activity,
                unread_report_count: agent.unread_report_count,
                status: agent.status,
            });
        effects
    }

    pub(super) fn apply_interaction(
        &mut self,
        event: piko_protocol::InteractionEvent,
    ) -> Vec<Effect> {
        let effects = Vec::new();
        match event {
            piko_protocol::InteractionEvent::Requested {
                session_id,
                agent_instance_id,
                interaction_id,
                title,
                questions,
                require_confirm,
                auto_resolution_ms,
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                if auto_resolution_ms.is_none() {
                    self.notifications.push_with(
                        NoticeScope::Session(session_id.clone()),
                        NotificationLevel::Warning,
                        NoticePolicy::UntilResolved(NoticeSubject::Interaction(
                            interaction_id.clone(),
                        )),
                        title
                            .as_deref()
                            .unwrap_or("tool input requested")
                            .to_string(),
                    );
                }
                self.interactions.push(
                    interaction_id,
                    agent_instance_id,
                    title,
                    questions,
                    require_confirm,
                    auto_resolution_ms.is_none(),
                );
                if auto_resolution_ms.is_none() {
                    self.push_surface(SurfaceId::ToolInteraction);
                }
            }
            piko_protocol::InteractionEvent::Resolved {
                session_id,
                interaction_id,
                status: _,
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                self.notifications
                    .resolve(&NoticeSubject::Interaction(interaction_id.clone()));
                self.interactions.resolve(&interaction_id);
                if self.interactions.is_empty()
                    && self.focus_manager.active_mode()
                        == AppMode::Surface(SurfaceId::ToolInteraction)
                {
                    self.pop_focus();
                }
            }
        }
        effects
    }

    pub(super) fn apply_todo_list_updated(
        &mut self,
        updated: piko_protocol::TodoListUpdated,
    ) -> Vec<Effect> {
        // Live projection: no session id on the event body; accept when we have a live session.
        if self.session.id.is_none() || self.session.initializing {
            return Vec::new();
        }
        self.todo_lists.upsert(updated.todo_list);
        Vec::new()
    }

    pub(super) fn apply_turn_diff(&mut self, diff: piko_protocol::TurnDiffEvent) -> Vec<Effect> {
        let effects = Vec::new();
        if !self.accepts_session(&diff.session_id) {
            return effects;
        }
        self.last_turn_id = Some(diff.turn_id.clone());
        self.last_turn_diff = Some(diff.clone());
        self.status = format!(
            "turn {} changed {} file{}",
            diff.turn_id,
            diff.files.len(),
            if diff.files.len() == 1 { "" } else { "s" }
        );
        effects
    }
}
