use super::*;

impl AppState {
    pub(super) fn apply_snapshot(&mut self, snapshot: SessionSnapshot) {
        self.timeline.clear();
        self.agent_timelines.clear();
        self.agent_panel.active_agent_instance_id = None;
        self.queue_status = QueueStatus::default();
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

        for entry in active_entries {
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
                    let tool = ToolEntry::new(
                        tool_call.tool_call_id,
                        tool_call.tool_name,
                        ToolStatus::Running,
                        compact_json(&tool_call.arguments),
                        None,
                        tool_call.parent_message_id,
                    );
                    if let Some(agent_instance_id) = agent_instance_id {
                        self.with_agent_timeline(&agent_instance_id, |timeline| {
                            if !timeline.upsert_tool(tool.clone()) {
                                timeline.push(TimelineEntry::Tool(tool));
                            }
                        });
                    } else if !self.timeline.upsert_tool(tool.clone()) {
                        self.push(TimelineEntry::Tool(tool));
                    }
                }
                SessionTreeEntry::ModelChange(change) => {
                    self.model.active_model_id = Some(change.model_id.clone());
                    self.model.active_provider = Some(change.provider.clone());
                    if let Some(text) =
                        session_entry_timeline_text(&SessionTreeEntry::ModelChange(change))
                    {
                        self.push(TimelineEntry::Session(text));
                    }
                }
                SessionTreeEntry::ThinkingLevelChange(change) => {
                    self.model.active_thinking_level = Some(change.thinking_level.clone());
                    if let Some(text) =
                        session_entry_timeline_text(&SessionTreeEntry::ThinkingLevelChange(change))
                    {
                        self.push(TimelineEntry::Session(text));
                    }
                }
                other => {
                    if let Some(text) = session_entry_timeline_text(&other) {
                        self.push(TimelineEntry::Session(text));
                    }
                }
            }
        }

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
            self.approvals.push(PendingApproval {
                id: approval.approval_id,
                agent_instance_id: approval.agent_instance_id,
                tool_name,
                args: approval.request,
                prompt: approval.prompt,
            });
        }
        self.interactions.clear();
        for interaction in snapshot.pending_interactions {
            self.interactions.push(
                interaction.interaction_id,
                interaction.agent_instance_id,
                interaction.title,
                interaction.questions,
                interaction.require_confirm,
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
