use super::*;

impl AppState {
    pub(super) fn apply_snapshot(&mut self, snapshot: SessionSnapshot) {
        let snapshot_session_id = snapshot.session_id.clone();
        self.notifications
            .clear_state_derived_for_session(&snapshot_session_id);
        self.timelines.clear();
        self.agent_panel.active_agent_instance_id = None;
        self.tree.set_agent_filter(None);
        self.queue_status = QueueStatus::default();
        self.agent_usage = snapshot.agent_usage.clone();
        // Host-projected todo lists replace any prior session map.
        self.todo_lists.replace_all(snapshot.todo_lists.clone());
        // Authoritative session ledger replaces any local roll-up.
        self.session.cumulative_usage = snapshot.cumulative_usage.clone();
        self.session.last_context_tokens = snapshot
            .active_turns
            .iter()
            .rev()
            .find_map(|turn| {
                turn.usage
                    .as_ref()
                    .map(crate::features::bottom_bar::context_tokens_from_usage)
            })
            .filter(|&tokens| tokens > 0)
            .or_else(|| {
                last_context_tokens_from_entries(
                    &snapshot.entries,
                    snapshot.current_leaf_id.as_deref(),
                )
            });
        self.tree
            .load(&snapshot.entries, snapshot.current_leaf_id.as_deref());

        let active_entries =
            get_active_branch_entries(&snapshot.entries, snapshot.current_leaf_id.as_deref());

        let mut timeline_agent_ids = Vec::new();
        for entry in &active_entries {
            let agent_instance_id = match entry {
                SessionTreeEntry::Message(message) => Some(message.agent_instance_id.as_str()),
                SessionTreeEntry::ToolCall(tool) => tool.agent_instance_id.as_deref(),
                _ => None,
            };
            if let Some(agent_instance_id) = agent_instance_id
                && !timeline_agent_ids.iter().any(|id| id == agent_instance_id)
            {
                timeline_agent_ids.push(agent_instance_id.to_string());
            }
        }
        let selected_timeline = snapshot
            .selected_agent_instance_id
            .clone()
            .filter(|selected| timeline_agent_ids.contains(selected))
            .or_else(|| timeline_agent_ids.first().cloned());
        self.agent_panel.active_agent_instance_id = selected_timeline.clone();
        for agent_instance_id in timeline_agent_ids {
            if Some(&agent_instance_id) != selected_timeline.as_ref() {
                self.timelines.ensure_inactive(agent_instance_id);
            }
        }

        self.timelines.begin_projection_batch();
        for (order, entry) in active_entries.into_iter().enumerate() {
            match entry {
                SessionTreeEntry::Message(message_entry) => {
                    let agent_instance_id = message_entry.agent_instance_id.clone();
                    let committed = piko_protocol::TranscriptCommittedEvent {
                        session_id: snapshot.session_id.clone(),
                        agent_instance_id: agent_instance_id.clone(),
                        agent_id: message_entry.agent_id,
                        source_turn_id: message_entry.source_turn_id,
                        message_id: message_entry.id,
                        transcript_seq: message_entry.transcript_seq,
                        message: message_entry.message,
                    };
                    self.with_agent_timeline(&agent_instance_id, |timeline| {
                        timeline.apply_committed(committed);
                    });
                }
                SessionTreeEntry::ToolCall(tool_call) => {
                    let agent_instance_id = tool_call.agent_instance_id.clone();
                    let tool_call_id = tool_call.tool_call_id;
                    let tool_name = tool_call.tool_name;
                    let arguments = tool_call.arguments;
                    let parent_message_id = tool_call.parent_message_id;
                    if let Some(agent_instance_id) = agent_instance_id {
                        self.with_agent_timeline(&agent_instance_id, |timeline| {
                            timeline.project_tool_started(
                                tool_call_id,
                                tool_name,
                                arguments,
                                parent_message_id,
                            );
                        });
                    } else {
                        self.timeline_mut().project_tool_started(
                            tool_call_id,
                            tool_name,
                            arguments,
                            parent_message_id,
                        );
                    }
                }
                entry => {
                    if let SessionTreeEntry::ModelChange(change) = &entry {
                        self.model.active_model_id = Some(change.model_id.clone());
                        self.model.active_provider = Some(change.provider.clone());
                    } else if let SessionTreeEntry::ThinkingLevelChange(change) = &entry {
                        self.model.active_thinking_level = Some(change.thinking_level.clone());
                    }
                    self.timelines.apply_session_entry(entry, order as u64);
                }
            }
        }
        for boundary in snapshot.model_steps.iter().cloned() {
            let agent_instance_id = boundary.agent_instance_id.clone();
            let mut outcome = piko_client_core::ApplyOutcome::Ignored;
            self.with_agent_timeline(&agent_instance_id, |timeline| {
                outcome = timeline.apply_model_step_committed(boundary.clone());
            });
            if outcome != piko_client_core::ApplyOutcome::Inconsistent {
                self.timelines.remember_model_step(boundary);
            }
        }
        self.timelines.end_projection_batch();

        self.session.active_turns = snapshot
            .active_turns
            .into_iter()
            .map(|turn| {
                (
                    turn.agent_instance_id,
                    crate::app::ActiveTurnUi {
                        turn_id: turn.turn_id,
                        status: turn.status,
                    },
                )
            })
            .collect();

        self.approvals.clear();
        for approval in snapshot.pending_approvals {
            let tool_name = if approval.tool_name.is_empty() {
                approval
                    .request
                    .get("toolName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool")
                    .to_string()
            } else {
                approval.tool_name
            };
            let approval_id = approval.approval_id;
            self.notifications.restore_with(
                NoticeScope::Session(snapshot_session_id.clone()),
                NotificationLevel::Warning,
                NoticePolicy::UntilResolved(NoticeSubject::Approval(approval_id.clone())),
                format!("approval requested for {tool_name}"),
            );
            self.approvals.push(PendingApproval {
                id: approval_id,
                agent_instance_id: approval.agent_instance_id,
                tool_name,
                args: approval.request,
                prompt: approval.prompt,
                selected_idx: 0,
            });
        }
        self.interactions.clear();
        for interaction in snapshot.pending_interactions {
            let interaction_id = interaction.interaction_id;
            let title = interaction.title;
            if interaction.auto_resolution_ms.is_none() {
                self.notifications.restore_with(
                    NoticeScope::Session(snapshot_session_id.clone()),
                    NotificationLevel::Warning,
                    NoticePolicy::UntilResolved(NoticeSubject::Interaction(interaction_id.clone())),
                    title
                        .as_deref()
                        .unwrap_or("tool input requested")
                        .to_string(),
                );
            }
            self.interactions.push(
                interaction_id,
                interaction.agent_instance_id,
                title,
                interaction.questions,
                interaction.require_confirm,
                true,
            );
        }
        if !self.approvals.is_empty()
            && self.focus_manager.active_mode() != AppMode::Surface(SurfaceId::Approval)
        {
            self.push_surface(SurfaceId::Approval);
        } else if !self.interactions.is_empty()
            && self.focus_manager.active_mode() != AppMode::Surface(SurfaceId::ToolInteraction)
        {
            self.push_surface(SurfaceId::ToolInteraction);
        }
    }
}
