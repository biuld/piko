use super::content::{clear_kind_content, replace_kind_content, update_content_segment};
use super::*;

impl AgentTimeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn items(&self) -> &[TimelineItem] {
        &self.items
    }

    pub fn committed_count(&self) -> usize {
        self.committed_records.len()
    }

    pub fn draft_count(&self) -> usize {
        self.draft_ids.len()
    }

    pub fn apply_committed(
        &mut self,
        message_id: MessageId,
        transcript_seq: u64,
        message: Message,
        source_turn_id: String,
    ) -> bool {
        matches!(
            self.apply_committed_checked(message_id, transcript_seq, message, source_turn_id),
            ApplyOutcome::Applied
        )
    }

    pub fn apply_committed_checked(
        &mut self,
        message_id: MessageId,
        transcript_seq: u64,
        message: Message,
        source_turn_id: String,
    ) -> ApplyOutcome {
        let record = CommittedItem {
            message_id: message_id.clone(),
            transcript_seq,
            message: message.clone(),
            source_turn_id: source_turn_id.clone(),
        };
        if let Some(existing) = self.committed_records.get(&message_id) {
            return if existing.transcript_seq == transcript_seq
                && existing.message == message
                && existing.source_turn_id == source_turn_id
            {
                ApplyOutcome::Ignored
            } else {
                ApplyOutcome::Inconsistent
            };
        }

        match &message {
            Message::Context { .. } => {}
            Message::User { .. } | Message::Assistant { .. } => {
                if let Some(&draft_idx) = self.draft_ids.get(&message_id) {
                    self.items[draft_idx] = TimelineItem::Committed(Box::new(record.clone()));
                } else {
                    self.items
                        .push(TimelineItem::Committed(Box::new(record.clone())));
                }
            }
            Message::ToolCall {
                id,
                name,
                arguments,
                ..
            } => self.apply_tool_started_with_turn(
                id.clone(),
                name.clone(),
                arguments.clone(),
                None,
                Some(source_turn_id.clone()),
                Some(transcript_seq),
            ),
            Message::ToolResult {
                tool_call_id,
                tool_name,
                content,
                details,
                is_error,
                ..
            } => self.apply_tool_ended_with_turn(
                tool_call_id.clone(),
                tool_name.clone(),
                None,
                content.clone(),
                details.clone(),
                is_error.unwrap_or(false),
                Some(source_turn_id.clone()),
                Some(transcript_seq),
            ),
        }
        self.committed_records.insert(message_id, record);
        self.maintenance();
        ApplyOutcome::Applied
    }

    pub fn apply_session_entry(
        &mut self,
        entry: SessionTreeEntry,
        branch_order: u64,
    ) -> ApplyOutcome {
        if matches!(entry, SessionTreeEntry::Message(_)) || !visible_session_entry(&entry) {
            return ApplyOutcome::Ignored;
        }
        let id = entry.id().to_string();
        if let Some(&idx) = self.session_entry_ids.get(&id) {
            return match &self.items[idx] {
                TimelineItem::SessionEntry(existing)
                    if existing.entry == entry && existing.branch_order == branch_order =>
                {
                    ApplyOutcome::Ignored
                }
                _ => ApplyOutcome::Inconsistent,
            };
        }
        self.items
            .push(TimelineItem::SessionEntry(Box::new(SessionEntryItem {
                entry,
                branch_order,
            })));
        self.session_entry_ids.insert(id, self.items.len() - 1);
        self.maintenance_indexes_only();
        ApplyOutcome::Applied
    }

    pub(crate) fn apply_realtime_checked(
        &mut self,
        message_id: MessageId,
        delta_seq: u64,
        delta: &RealtimeDelta,
    ) -> ApplyOutcome {
        if self.committed_records.contains_key(&message_id) {
            return ApplyOutcome::Ignored;
        }
        let outcome = self.prepare_draft(&message_id, delta_seq);
        if outcome != ApplyOutcome::Applied {
            return outcome;
        }

        let draft_idx = self.draft_ids[&message_id];
        let tool_arg_chunk = match delta {
            RealtimeDelta::ToolCall {
                tool_call_id,
                delta,
                content_index,
            } => Some((tool_call_id.clone(), delta.clone(), *content_index)),
            _ => None,
        };
        if let TimelineItem::RealtimeDraft(draft) = &mut self.items[draft_idx] {
            match delta {
                RealtimeDelta::Text {
                    delta,
                    content_index,
                } => update_content_segment(
                    &mut draft.content_segments,
                    RealtimeContentKind::Text,
                    *content_index,
                    StreamItemOp::AppendChunk,
                    Some(delta),
                ),
                RealtimeDelta::Thinking {
                    delta,
                    content_index,
                } => update_content_segment(
                    &mut draft.content_segments,
                    RealtimeContentKind::Thinking,
                    *content_index,
                    StreamItemOp::AppendChunk,
                    Some(delta),
                ),
                _ => {}
            }
        }
        if let Some((tool_call_id, chunk, content_index)) = tool_arg_chunk {
            self.apply_tool_content(
                tool_call_id,
                StreamItemOp::AppendChunk,
                content_index,
                Some(&chunk),
                Some(message_id),
                None,
            );
        }
        ApplyOutcome::Applied
    }

    pub fn apply_stream_item(&mut self, patch: &piko_protocol::StreamItemPatch) -> ApplyOutcome {
        let parent = patch
            .fields
            .as_ref()
            .and_then(|f| f.get("parentMessageId"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| patch.item_id.clone());
        let source_turn_id = patch
            .fields
            .as_ref()
            .and_then(|f| f.get("turnId"))
            .and_then(|v| v.as_str())
            .map(str::to_string);

        if matches!(
            patch.op,
            StreamItemOp::ReplaceContent | StreamItemOp::ClearContent
        ) || (patch.op == StreamItemOp::Upsert && patch.text.is_some())
        {
            let full_upsert = patch.op == StreamItemOp::Upsert;
            let op = if patch.op == StreamItemOp::Upsert {
                StreamItemOp::ReplaceContent
            } else {
                patch.op
            };
            let Some(seq) = patch.delta_seq else {
                return ApplyOutcome::Inconsistent;
            };
            let outcome = self.prepare_draft(&parent, seq);
            if outcome != ApplyOutcome::Applied {
                return outcome;
            }
            match patch.item_kind {
                StreamItemKind::AgentMessage | StreamItemKind::AgentThought => {
                    let idx = self.draft_ids[&parent];
                    let TimelineItem::RealtimeDraft(draft) = &mut self.items[idx] else {
                        return ApplyOutcome::Inconsistent;
                    };
                    let kind = if patch.item_kind == StreamItemKind::AgentThought {
                        RealtimeContentKind::Thinking
                    } else {
                        RealtimeContentKind::Text
                    };
                    if full_upsert {
                        replace_kind_content(
                            &mut draft.content_segments,
                            kind,
                            patch.content_index.unwrap_or(0),
                            patch.text.as_deref(),
                        );
                    } else if op == StreamItemOp::ClearContent && patch.content_index.is_none() {
                        clear_kind_content(&mut draft.content_segments, kind);
                    } else {
                        update_content_segment(
                            &mut draft.content_segments,
                            kind,
                            patch.content_index.unwrap_or(0),
                            op,
                            patch.text.as_deref(),
                        );
                    }
                }
                StreamItemKind::ToolCall => {
                    if full_upsert
                        && let Some(&idx) = self.tool_ids.get(&patch.item_id)
                        && let TimelineItem::Tool(tool) = &mut self.items[idx]
                    {
                        tool.argument_segments.clear();
                        tool.partial_json = None;
                    }
                    if op == StreamItemOp::ClearContent && patch.content_index.is_none() {
                        if let Some(&idx) = self.tool_ids.get(&patch.item_id)
                            && let TimelineItem::Tool(tool) = &mut self.items[idx]
                        {
                            tool.argument_segments.clear();
                            tool.partial_json = Some(String::new());
                        }
                    } else {
                        self.apply_tool_content(
                            patch.item_id.clone(),
                            op,
                            patch.content_index.unwrap_or(0),
                            patch.text.as_deref(),
                            Some(parent),
                            source_turn_id,
                        );
                    }
                }
                _ => return ApplyOutcome::Ignored,
            }
            return ApplyOutcome::Applied;
        }

        if let Some((message_id, delta_seq, delta)) = patch.as_realtime_apply() {
            return self.apply_realtime_checked(message_id, delta_seq, &delta);
        }

        if patch.item_kind == StreamItemKind::ToolCall && patch.op == StreamItemOp::Upsert {
            let Some(fields) = patch.fields.as_ref() else {
                return ApplyOutcome::Inconsistent;
            };
            let status = fields.get("status").and_then(|v| v.as_str());
            match status {
                Some("running") => self.apply_tool_started_with_turn(
                    patch.item_id.clone(),
                    fields
                        .get("toolName")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    fields
                        .get("args")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                    fields
                        .get("parentMessageId")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    source_turn_id,
                    None,
                ),
                Some("completed" | "failed") => self.apply_tool_ended_with_turn(
                    patch.item_id.clone(),
                    fields
                        .get("toolName")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    fields.get("result").cloned(),
                    Vec::new(),
                    None,
                    status == Some("failed"),
                    source_turn_id,
                    None,
                ),
                Some("cancelled") => {
                    if let Some(&idx) = self.tool_ids.get(&patch.item_id)
                        && let TimelineItem::Tool(tool) = &mut self.items[idx]
                    {
                        tool.status = ToolStatus::Cancelled;
                        if source_turn_id.is_some() {
                            tool.source_turn_id = source_turn_id;
                        }
                    } else {
                        let live_order = self.allocate_live_order();
                        self.items.push(TimelineItem::Tool(Box::new(ToolItem {
                            tool_call_id: patch.item_id.clone(),
                            tool_name: fields
                                .get("toolName")
                                .and_then(|v| v.as_str())
                                .unwrap_or("tool")
                                .to_string(),
                            args: fields
                                .get("args")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null),
                            partial_json: None,
                            argument_segments: Vec::new(),
                            result: fields.get("result").cloned(),
                            result_content: Vec::new(),
                            result_details: None,
                            status: ToolStatus::Cancelled,
                            parent_message_id: fields
                                .get("parentMessageId")
                                .and_then(|v| v.as_str())
                                .map(str::to_string),
                            source_turn_id,
                            transcript_seq: None,
                            live_order,
                        })));
                        self.tool_ids
                            .insert(patch.item_id.clone(), self.items.len() - 1);
                        self.maintenance_indexes_only();
                    }
                }
                _ => return ApplyOutcome::Ignored,
            }
            return ApplyOutcome::Applied;
        }

        ApplyOutcome::Ignored
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.committed_records.clear();
        self.draft_ids.clear();
        self.tool_ids.clear();
        self.session_entry_ids.clear();
        self.next_live_order = 0;
    }

    fn prepare_draft(&mut self, message_id: &str, delta_seq: u64) -> ApplyOutcome {
        if self.committed_records.contains_key(message_id) {
            return ApplyOutcome::Ignored;
        }
        if !self.draft_ids.contains_key(message_id) {
            let live_order = self.allocate_live_order();
            self.items.push(TimelineItem::RealtimeDraft(RealtimeDraft {
                message_id: message_id.to_string(),
                last_delta_seq: delta_seq,
                content_segments: Vec::new(),
                live_order,
            }));
            self.draft_ids
                .insert(message_id.to_string(), self.items.len() - 1);
            self.maintenance_indexes_only();
            return ApplyOutcome::Applied;
        }
        let idx = self.draft_ids[message_id];
        let TimelineItem::RealtimeDraft(draft) = &mut self.items[idx] else {
            return ApplyOutcome::Inconsistent;
        };
        if delta_seq <= draft.last_delta_seq {
            return ApplyOutcome::Ignored;
        }
        if delta_seq != draft.last_delta_seq + 1 {
            return ApplyOutcome::Inconsistent;
        }
        draft.last_delta_seq = delta_seq;
        ApplyOutcome::Applied
    }

    pub(super) fn allocate_live_order(&mut self) -> u64 {
        let order = self.next_live_order;
        self.next_live_order = self.next_live_order.saturating_add(1);
        order
    }

    pub(super) fn reorder_authored_items(&mut self) {
        let positions: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| authored_seq(item).map(|_| idx))
            .collect();
        let mut authored: Vec<TimelineItem> = positions
            .iter()
            .map(|idx| self.items[*idx].clone())
            .collect();
        authored.sort_by_key(|item| authored_seq(item).unwrap_or(u64::MAX));
        for (idx, item) in positions.into_iter().zip(authored) {
            self.items[idx] = item;
        }
        self.rebuild_indexes();
    }

    /// Enter a batch of mutations. Per-item index rebuilds and authored
    /// reorders are deferred to `end_batch`; lookup maps are still updated
    /// incrementally so ToolCall/ToolResult pairing stays correct.
    ///
    /// Contract: batch input must arrive in hydrated order (tool starts
    /// before their results, no duplicate message ids). Out-of-order or
    /// duplicate input inside a batch is not validated per item.
    pub fn begin_batch(&mut self) {
        self.batch_depth = self.batch_depth.saturating_add(1);
    }

    /// Leave a batch. When the outermost batch ends, indexes are rebuilt and
    /// authored items are reordered once.
    pub fn end_batch(&mut self) {
        if self.batch_depth == 0 {
            return;
        }
        self.batch_depth -= 1;
        if self.batch_depth == 0 {
            self.rebuild_indexes();
            self.reorder_authored_items();
        }
    }

    pub(super) fn maintenance(&mut self) {
        if self.batch_depth == 0 {
            self.rebuild_indexes();
            self.reorder_authored_items();
        }
    }

    pub(super) fn maintenance_indexes_only(&mut self) {
        if self.batch_depth == 0 {
            self.rebuild_indexes();
        }
    }

    pub(super) fn rebuild_indexes(&mut self) {
        self.draft_ids.clear();
        self.tool_ids.clear();
        self.session_entry_ids.clear();
        for (idx, item) in self.items.iter().enumerate() {
            match item {
                TimelineItem::RealtimeDraft(draft) => {
                    self.draft_ids.insert(draft.message_id.clone(), idx);
                }
                TimelineItem::Tool(tool) => {
                    self.tool_ids.insert(tool.tool_call_id.clone(), idx);
                }
                TimelineItem::SessionEntry(entry) => {
                    self.session_entry_ids
                        .insert(entry.entry.id().to_string(), idx);
                }
                TimelineItem::Committed(_) => {}
            }
        }
    }
}

fn authored_seq(item: &TimelineItem) -> Option<u64> {
    match item {
        TimelineItem::Committed(item) => Some(item.transcript_seq),
        TimelineItem::Tool(item) => item.transcript_seq,
        _ => None,
    }
}

fn visible_session_entry(entry: &SessionTreeEntry) -> bool {
    match entry {
        SessionTreeEntry::ModelChange(_)
        | SessionTreeEntry::ThinkingLevelChange(_)
        | SessionTreeEntry::Compaction(_)
        | SessionTreeEntry::BranchSummary(_) => true,
        SessionTreeEntry::ActiveToolsChange(change) => !change.active_tool_names.is_empty(),
        SessionTreeEntry::CustomMessage(custom) => custom.display,
        _ => false,
    }
}
