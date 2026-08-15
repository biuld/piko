use super::*;

impl Timeline {
    pub fn new() -> Self {
        Self {
            components: VecDeque::new(),
            viewport: ScrollViewport::default(),
            thinking_visible: true,
            tool_calls: Vec::new(),
            projection: piko_client_core::AgentTimeline::new(),
            next_local_id: 1,
            line_cache: std::cell::RefCell::new(super::line_cache::LineCache::default()),
            projection_dirty: false,
            defer_projection_sync: false,
        }
    }

    pub fn begin_projection_batch(&mut self) {
        self.defer_projection_sync = true;
    }

    pub fn end_projection_batch(&mut self) {
        self.defer_projection_sync = false;
        self.flush_projection();
    }

    fn mark_projection_applied(&mut self) {
        if self.defer_projection_sync {
            self.projection_dirty = true;
        } else {
            self.sync_projection();
        }
    }

    fn flush_projection(&mut self) {
        if self.projection_dirty {
            self.projection_dirty = false;
            self.sync_projection();
        }
    }

    pub fn push(&mut self, entry: TimelineEntry) {
        match entry {
            TimelineEntry::Tool(tool) => {
                let updated = self.upsert_tool(tool.clone());
                if !updated {
                    self.push_component(TimelineComponent::Tool(tool));
                }
            }
            TimelineEntry::Error(text) => self.push_error(text),
        }
    }

    pub fn apply_stream_item(
        &mut self,
        patch: &piko_protocol::StreamItemPatch,
    ) -> piko_client_core::ApplyOutcome {
        let outcome = self.projection.apply_stream_item(patch);
        if outcome == piko_client_core::ApplyOutcome::Applied {
            self.mark_projection_applied();
        }
        outcome
    }

    pub fn apply_committed(&mut self, event: TranscriptCommittedEvent) -> bool {
        let outcome = self.projection.apply_committed_checked(
            event.message_id,
            event.transcript_seq,
            event.message,
            event.source_turn_id,
        );
        if outcome == piko_client_core::ApplyOutcome::Applied {
            self.mark_projection_applied();
        }
        outcome != piko_client_core::ApplyOutcome::Inconsistent
    }

    pub fn apply_session_entry(
        &mut self,
        entry: piko_protocol::SessionTreeEntry,
        branch_order: u64,
    ) -> piko_client_core::ApplyOutcome {
        let outcome = self.projection.apply_session_entry(entry, branch_order);
        if outcome == piko_client_core::ApplyOutcome::Applied {
            self.mark_projection_applied();
        }
        outcome
    }

    pub fn project_tool_started(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        parent_message_id: Option<String>,
    ) {
        self.projection
            .apply_tool_started(tool_call_id, tool_name, args, parent_message_id);
        self.mark_projection_applied();
    }

    pub fn finish_turn(&mut self, turn_id: &str, status: crate::app::ToolStatus) {
        let status = match status {
            crate::app::ToolStatus::Failed => piko_client_core::ToolStatus::Failed,
            crate::app::ToolStatus::Cancelled => piko_client_core::ToolStatus::Cancelled,
            _ => return,
        };
        self.projection.finish_turn(turn_id, status);
        self.mark_projection_applied();
    }

    pub fn push_error(&mut self, text: String) {
        self.push_anchored_error(text, None);
    }

    pub fn push_turn_error(&mut self, turn_id: &str, text: String) {
        self.push_anchored_error(text, Some(turn_id.to_string()));
    }

    fn push_anchored_error(&mut self, text: String, after_turn_id: Option<String>) {
        let anchored = after_turn_id.is_some();
        let id = self.local_id();
        self.push_component(TimelineComponent::Error(ErrorComponent {
            id,
            text,
            after_turn_id,
        }));
        if anchored {
            self.mark_projection_applied();
        }
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.viewport.scroll_up(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.viewport.scroll_down(amount);
    }

    pub fn jump_latest(&mut self) {
        self.viewport.jump_latest();
    }

    pub fn toggle_tool(&mut self, source_index: usize) {
        let Some(TimelineComponent::Tool(tool)) = self.components.get_mut(source_index) else {
            return;
        };
        tool.expanded = !tool.expanded;
        let id = tool.id.clone();
        let expanded = tool.expanded;
        if let Some(registry) = self.tool_calls.iter_mut().find(|tool| tool.id == id) {
            registry.expanded = expanded;
        }
    }

    pub fn clear(&mut self) {
        self.components.clear();
        self.tool_calls.clear();
        self.viewport.jump_latest();
        self.projection.clear();
        self.line_cache.borrow_mut().clear();
        self.projection_dirty = false;
        self.defer_projection_sync = false;
    }

    /// Update or insert a presentation-only tool. Host-authored tools enter
    /// through the canonical client-core projection above.
    pub fn upsert_tool(&mut self, mut tool: ToolEntry) -> bool {
        tool.component_id = ComponentId::ToolCallId(tool.id.clone());
        if let Some(existing) = self.tool_calls.iter_mut().find(|t| t.id == tool.id) {
            tool.expanded = existing.expanded;
            *existing = tool.clone();
        } else {
            self.tool_calls.push(tool.clone());
        }
        for component in self.components.iter_mut().rev() {
            if let TimelineComponent::Tool(existing) = component
                && existing.id == tool.id
            {
                *existing = tool;
                return true;
            }
        }
        false
    }

    fn sync_projection(&mut self) {
        use piko_client_core::TimelineItem as CoreItem;

        let expanded: HashMap<String, bool> = self
            .tool_calls
            .iter()
            .map(|tool| (tool.id.clone(), tool.expanded))
            .collect();
        let local_errors: Vec<TimelineComponent> = self
            .components
            .iter()
            .filter(|component| matches!(component, TimelineComponent::Error(_)))
            .cloned()
            .collect();
        let was_at_latest = self.viewport.is_at_latest();
        self.components.clear();
        self.tool_calls.clear();

        let mut last_component_by_turn = HashMap::new();
        for item in self.projection.items() {
            let source_turn_id = match item {
                CoreItem::Committed(committed) => Some(committed.source_turn_id.clone()),
                CoreItem::Tool(tool) => tool.source_turn_id.clone(),
                CoreItem::RealtimeDraft(_) | CoreItem::SessionEntry(_) => None,
            };
            let component = match item {
                CoreItem::Committed(committed) => {
                    component_from_message(committed.message_id.clone(), &committed.message)
                }
                CoreItem::RealtimeDraft(draft) => {
                    let blocks = draft
                        .content_segments
                        .iter()
                        .filter(|segment| !segment.text.is_empty())
                        .map(|segment| match segment.kind {
                            piko_client_core::RealtimeContentKind::Text => {
                                ContentBlock::Text(segment.text.clone())
                            }
                            piko_client_core::RealtimeContentKind::Thinking => {
                                ContentBlock::Thinking(segment.text.clone())
                            }
                        })
                        .collect();
                    Some(TimelineComponent::Assistant(AssistantMessageComponent {
                        id: ComponentId::MessageId(draft.message_id.clone()),
                        blocks,
                        stop_reason: None,
                        error_message: None,
                        // Streaming draft has no committed timestamp yet.
                        timestamp: None,
                    }))
                }
                CoreItem::Tool(tool) => {
                    let result = if !tool.result_content.is_empty() {
                        Some(protocol_blocks_to_text(&tool.result_content))
                    } else {
                        tool.result.as_ref().map(super::tool_format::json_for_entry)
                    };
                    let args = tool
                        .partial_json
                        .clone()
                        .unwrap_or_else(|| super::tool_format::json_for_entry(&tool.args));
                    let mut projected = ToolEntry::new(
                        tool.tool_call_id.clone(),
                        tool.tool_name.clone(),
                        map_tool_status(tool.status),
                        args,
                        result,
                        tool.parent_message_id.clone(),
                    );
                    projected.result_details = tool
                        .result_details
                        .as_ref()
                        .map(super::tool_format::json_for_entry);
                    projected.expanded = expanded.get(&projected.id).copied().unwrap_or(false);
                    self.tool_calls.push(projected.clone());
                    Some(TimelineComponent::Tool(projected))
                }
                CoreItem::SessionEntry(entry) => component_from_session_entry(&entry.entry),
            };
            if let Some(component) = component {
                self.components.push_back(component);
                if let Some(turn_id) = source_turn_id {
                    last_component_by_turn.insert(turn_id, self.components.len() - 1);
                }
            }
        }
        for error in local_errors {
            let insertion_index = match &error {
                TimelineComponent::Error(error) => error
                    .after_turn_id
                    .as_ref()
                    .and_then(|turn_id| last_component_by_turn.get(turn_id))
                    .map(|index| index + 1),
                _ => None,
            };
            if let Some(index) = insertion_index {
                self.components.insert(index, error);
                for last_index in last_component_by_turn.values_mut() {
                    if *last_index >= index {
                        *last_index += 1;
                    }
                }
            } else {
                self.components.push_back(error);
            }
        }
        while self.components.len() > MAX_COMPONENTS {
            self.components.pop_front();
        }
        if was_at_latest {
            self.viewport.jump_latest();
        } else {
            self.viewport.mark_appended();
        }
    }

    #[cfg(test)]
    pub fn push_session_fact(&mut self, entry_id: String, label: &'static str, text: String) {
        self.push_component(TimelineComponent::SessionFact(SessionFactComponent {
            id: ComponentId::EntryId(entry_id),
            label,
            text,
        }));
    }

    #[cfg(test)]
    pub fn tool_call_count(&self) -> usize {
        self.components
            .iter()
            .filter(|component| matches!(component, TimelineComponent::Tool(_)))
            .count()
    }

    #[cfg(test)]
    pub fn component_kinds(&self) -> Vec<TimelineKind> {
        self.components
            .iter()
            .map(TimelineComponent::kind)
            .collect()
    }

    #[cfg(test)]
    pub fn message_ids(&self) -> Vec<String> {
        self.components
            .iter()
            .filter_map(|component| match component.id() {
                ComponentId::MessageId(id) => Some(id.clone()),
                _ => None,
            })
            .collect()
    }

    #[cfg(test)]
    pub fn assistant_text(&self, message_id: &str) -> Option<String> {
        self.components.iter().find_map(|component| {
            let TimelineComponent::Assistant(assistant) = component else {
                return None;
            };
            if assistant.id != ComponentId::MessageId(message_id.to_string()) {
                return None;
            }
            Some(
                assistant
                    .blocks
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text(text) => Some(text.as_str()),
                        _ => None,
                    })
                    .collect(),
            )
        })
    }
}

fn component_from_message(
    id: String,
    message: &piko_protocol::Message,
) -> Option<TimelineComponent> {
    match message {
        piko_protocol::Message::User { timestamp, .. } => {
            Some(TimelineComponent::User(UserMessageComponent {
                id: ComponentId::MessageId(id),
                text: crate::text::message_to_text(message),
                timestamp: *timestamp,
            }))
        }
        piko_protocol::Message::Assistant {
            content,
            stop_reason,
            error_message,
            timestamp,
            ..
        } => Some(TimelineComponent::Assistant(AssistantMessageComponent {
            id: ComponentId::MessageId(id),
            blocks: content.iter().cloned().map(ContentBlock::from).collect(),
            stop_reason: stop_reason.clone(),
            error_message: error_message.clone(),
            timestamp: *timestamp,
        })),
        _ => None,
    }
}

fn component_from_session_entry(
    entry: &piko_protocol::SessionTreeEntry,
) -> Option<TimelineComponent> {
    use piko_protocol::SessionTreeEntry;
    match entry {
        SessionTreeEntry::ModelChange(change) => {
            Some(TimelineComponent::SessionFact(SessionFactComponent {
                id: ComponentId::EntryId(change.id.clone()),
                label: "model",
                text: format!("changed to {}/{}", change.provider, change.model_id),
            }))
        }
        SessionTreeEntry::ThinkingLevelChange(change) => {
            Some(TimelineComponent::SessionFact(SessionFactComponent {
                id: ComponentId::EntryId(change.id.clone()),
                label: "thinking",
                text: format!("changed to {}", change.thinking_level),
            }))
        }
        SessionTreeEntry::ActiveToolsChange(change) if !change.active_tool_names.is_empty() => {
            Some(TimelineComponent::SessionFact(SessionFactComponent {
                id: ComponentId::EntryId(change.id.clone()),
                label: "tools",
                text: change.active_tool_names.join(", "),
            }))
        }
        SessionTreeEntry::Compaction(compaction) => {
            Some(TimelineComponent::Summary(SummaryComponent {
                id: ComponentId::EntryId(compaction.id.clone()),
                kind: SummaryKind::Compaction,
                text: compaction.summary.clone(),
            }))
        }
        SessionTreeEntry::BranchSummary(summary) => {
            Some(TimelineComponent::Summary(SummaryComponent {
                id: ComponentId::EntryId(summary.id.clone()),
                kind: SummaryKind::Branch,
                text: summary.summary.clone(),
            }))
        }
        SessionTreeEntry::CustomMessage(custom) if custom.display => {
            Some(TimelineComponent::CustomMessage(CustomMessageComponent {
                id: ComponentId::EntryId(custom.id.clone()),
                custom_type: custom.custom_type.clone(),
                content: custom.content.clone(),
            }))
        }
        _ => None,
    }
}

fn protocol_blocks_to_text(blocks: &[piko_protocol::ContentBlock]) -> String {
    blocks
        .iter()
        .map(|block| match block {
            piko_protocol::ContentBlock::Text { text } => text.clone(),
            piko_protocol::ContentBlock::Thinking { thinking, .. } => thinking.clone(),
            piko_protocol::ContentBlock::Image { mime_type, .. } => format!("[image: {mime_type}]"),
            other => other.text_projection(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn map_tool_status(status: piko_client_core::ToolStatus) -> crate::app::ToolStatus {
    match status {
        piko_client_core::ToolStatus::Running => crate::app::ToolStatus::Running,
        piko_client_core::ToolStatus::Completed => crate::app::ToolStatus::Completed,
        piko_client_core::ToolStatus::Failed => crate::app::ToolStatus::Failed,
        piko_client_core::ToolStatus::Cancelled => crate::app::ToolStatus::Cancelled,
    }
}
