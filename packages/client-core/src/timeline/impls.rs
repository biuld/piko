use super::*;

impl AgentTimeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn items(&self) -> &[TimelineItem] {
        &self.items
    }

    pub fn committed_count(&self) -> usize {
        self.committed_ids.len()
    }

    pub fn draft_count(&self) -> usize {
        self.draft_ids.len()
    }

    pub fn apply_tool_started(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        parent_message_id: Option<String>,
    ) {
        if let Some(&idx) = self.tool_ids.get(&tool_call_id) {
            if let TimelineItem::Tool(tool) = &mut self.items[idx] {
                tool.tool_name = tool_name;
                tool.args = args;
                tool.parent_message_id = parent_message_id;
                if tool.result.is_none() {
                    tool.status = ToolStatus::Running;
                }
            }
            return;
        }

        let idx = self.items.len();
        self.items.push(TimelineItem::Tool(ToolItem {
            tool_call_id: tool_call_id.clone(),
            tool_name,
            args,
            partial_json: None,
            result: None,
            status: ToolStatus::Running,
            parent_message_id,
        }));
        self.tool_ids.insert(tool_call_id, idx);
    }

    pub fn apply_tool_ended(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
    ) {
        let status = if is_error {
            ToolStatus::Failed
        } else {
            ToolStatus::Completed
        };
        if let Some(&idx) = self.tool_ids.get(&tool_call_id) {
            if let TimelineItem::Tool(tool) = &mut self.items[idx] {
                tool.tool_name = tool_name;
                tool.result = Some(result);
                tool.partial_json = None;
                tool.status = status;
            }
            return;
        }

        let idx = self.items.len();
        self.items.push(TimelineItem::Tool(ToolItem {
            tool_call_id: tool_call_id.clone(),
            tool_name,
            args: serde_json::Value::Null,
            partial_json: None,
            result: Some(result),
            status,
            parent_message_id: None,
        }));
        self.tool_ids.insert(tool_call_id, idx);
    }

    /// Append streaming tool-call argument bytes (RealtimeDelta::ToolCall).
    pub fn apply_tool_arg_chunk(
        &mut self,
        tool_call_id: String,
        chunk: &str,
        parent_message_id: Option<String>,
    ) {
        if let Some(&idx) = self.tool_ids.get(&tool_call_id) {
            if let TimelineItem::Tool(tool) = &mut self.items[idx]
                && tool.status == ToolStatus::Running
            {
                tool.partial_json
                    .get_or_insert_with(String::new)
                    .push_str(chunk);
                if tool.parent_message_id.is_none() {
                    tool.parent_message_id = parent_message_id;
                }
            }
            return;
        }
        let idx = self.items.len();
        self.items.push(TimelineItem::Tool(ToolItem {
            tool_call_id: tool_call_id.clone(),
            tool_name: String::new(),
            args: serde_json::Value::Null,
            partial_json: Some(chunk.to_string()),
            result: None,
            status: ToolStatus::Running,
            parent_message_id,
        }));
        self.tool_ids.insert(tool_call_id, idx);
    }

    /// Apply a committed transcript message. Deduplicates by message_id and
    /// supersedes any existing realtime draft for the same id.
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
        if let Some(&idx) = self.committed_ids.get(&message_id) {
            return match &self.items[idx] {
                TimelineItem::Committed(existing)
                    if existing.transcript_seq == transcript_seq
                        && existing.message == message
                        && existing.source_turn_id == source_turn_id =>
                {
                    ApplyOutcome::Ignored
                }
                _ => ApplyOutcome::Inconsistent,
            };
        }

        // Supersede draft if present
        if let Some(&draft_idx) = self.draft_ids.get(&message_id) {
            self.items[draft_idx] = TimelineItem::Committed(Box::new(CommittedItem {
                message_id: message_id.clone(),
                transcript_seq,
                message,
                source_turn_id,
            }));
            self.draft_ids.remove(&message_id);
            self.committed_ids.insert(message_id, draft_idx);
        } else {
            let idx = self.items.len();
            self.items
                .push(TimelineItem::Committed(Box::new(CommittedItem {
                    message_id: message_id.clone(),
                    transcript_seq,
                    message,
                    source_turn_id,
                })));
            self.committed_ids.insert(message_id, idx);
        }
        ApplyOutcome::Applied
    }

    /// Internal helper: apply a decoded realtime delta onto the timeline draft.
    ///
    /// Live host transport is StreamItem-only ([`apply_stream_item`]). External
    /// crates must not call this; conversion from `StreamItemPatch` happens
    /// inside `apply_stream_item`.
    pub(crate) fn apply_realtime_checked(
        &mut self,
        message_id: MessageId,
        delta_seq: u64,
        delta: &RealtimeDelta,
    ) -> ApplyOutcome {
        if self.committed_ids.contains_key(&message_id) {
            return ApplyOutcome::Ignored;
        }

        let draft_idx = if let Some(&idx) = self.draft_ids.get(&message_id) {
            idx
        } else {
            let idx = self.items.len();
            self.items.push(TimelineItem::RealtimeDraft(RealtimeDraft {
                message_id: message_id.clone(),
                last_delta_seq: 0,
                text_segments: Vec::new(),
                thinking_segments: Vec::new(),
            }));
            self.draft_ids.insert(message_id.clone(), idx);
            idx
        };

        let tool_arg_chunk = match delta {
            RealtimeDelta::ToolCall {
                tool_call_id,
                delta,
                ..
            } => Some((tool_call_id.clone(), delta.clone())),
            _ => None,
        };

        if let TimelineItem::RealtimeDraft(ref mut draft) = self.items[draft_idx] {
            if delta_seq <= draft.last_delta_seq {
                return ApplyOutcome::Ignored;
            }
            if draft.last_delta_seq > 0 && delta_seq != draft.last_delta_seq + 1 {
                return ApplyOutcome::Inconsistent;
            }
            draft.last_delta_seq = delta_seq;
            match delta {
                RealtimeDelta::Text {
                    delta,
                    content_index,
                    ..
                } => {
                    let idx = *content_index as usize;
                    while draft.text_segments.len() <= idx {
                        draft.text_segments.push(String::new());
                    }
                    draft.text_segments[idx].push_str(delta);
                }
                RealtimeDelta::Thinking {
                    delta,
                    content_index,
                    ..
                } => {
                    let idx = *content_index as usize;
                    while draft.thinking_segments.len() <= idx {
                        draft.thinking_segments.push(String::new());
                    }
                    draft.thinking_segments[idx].push_str(delta);
                }
                _ => {}
            }
        }

        if let Some((tool_call_id, chunk)) = tool_arg_chunk {
            self.apply_tool_arg_chunk(tool_call_id, &chunk, Some(message_id));
        }
        ApplyOutcome::Applied
    }

    /// Apply a unified stream-item patch — sole live stream transport (F-22).
    ///
    /// Assistant text/thinking patches carry `delta_seq` for in-order delivery
    /// bookkeeping on the client timeline.
    pub fn apply_stream_item(&mut self, patch: &piko_protocol::StreamItemPatch) -> ApplyOutcome {
        if let Some((message_id, delta_seq, delta)) = patch.as_realtime_apply() {
            return self.apply_realtime_checked(message_id, delta_seq, &delta);
        }

        if let (piko_protocol::StreamItemKind::ToolCall, piko_protocol::StreamItemOp::AppendChunk) =
            (patch.item_kind, patch.op)
        {
            let parent = patch
                .fields
                .as_ref()
                .and_then(|f| f.get("parentMessageId"))
                .and_then(|v| v.as_str().map(str::to_string));
            let chunk = patch.text.as_deref().unwrap_or("");
            self.apply_tool_arg_chunk(patch.item_id.clone(), chunk, parent);
            return ApplyOutcome::Applied;
        }

        if let Some(tool) = patch.tool_upsert_apply() {
            match tool {
                piko_protocol::ToolUpsertApply::Started {
                    tool_call_id,
                    tool_name,
                    args,
                    parent_message_id,
                } => {
                    self.apply_tool_started(tool_call_id, tool_name, args, parent_message_id);
                }
                piko_protocol::ToolUpsertApply::Ended {
                    tool_call_id,
                    tool_name,
                    result,
                    is_error,
                } => {
                    self.apply_tool_ended(tool_call_id, tool_name, result, is_error);
                }
            }
            return ApplyOutcome::Applied;
        }

        ApplyOutcome::Ignored
    }

    /// Clear all items (used on reconcile or agent switch).
    pub fn clear(&mut self) {
        self.items.clear();
        self.committed_ids.clear();
        self.draft_ids.clear();
        self.tool_ids.clear();
    }
}
