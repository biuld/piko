use super::*;

impl AgentTimeline {
    pub fn apply_tool_started(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        parent_message_id: Option<String>,
    ) {
        self.apply_tool_started_with_turn(
            tool_call_id,
            tool_name,
            args,
            parent_message_id,
            None,
            None,
        );
    }

    pub(super) fn apply_tool_started_with_turn(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        parent_message_id: Option<String>,
        source_turn_id: Option<String>,
        transcript_seq: Option<u64>,
    ) {
        if let Some(&idx) = self.tool_ids.get(&tool_call_id) {
            if let TimelineItem::Tool(tool) = &mut self.items[idx] {
                if !tool_name.is_empty() {
                    tool.tool_name = tool_name;
                }
                if !args.is_null() {
                    tool.args = args;
                    tool.partial_json = None;
                    tool.argument_segments.clear();
                }
                if parent_message_id.is_some() {
                    tool.parent_message_id = parent_message_id;
                }
                if source_turn_id.is_some() {
                    tool.source_turn_id = source_turn_id;
                }
                tool.transcript_seq = min_seq(tool.transcript_seq, transcript_seq);
                if tool.result.is_none() && tool.result_content.is_empty() {
                    tool.status = ToolStatus::Running;
                }
            }
            self.maintenance();
            return;
        }

        let live_order = self.allocate_live_order();
        self.items.push(TimelineItem::Tool(Box::new(ToolItem {
            tool_call_id: tool_call_id.clone(),
            tool_name,
            args,
            partial_json: None,
            argument_segments: Vec::new(),
            result: None,
            result_content: Vec::new(),
            result_details: None,
            status: ToolStatus::Running,
            parent_message_id,
            source_turn_id,
            transcript_seq,
            live_order,
            upstream: None,
            upstream_split: None,
        })));
        self.tool_ids
            .insert(tool_call_id.clone(), self.items.len() - 1);
        self.maintenance();
    }

    pub fn apply_tool_ended(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
    ) {
        self.apply_tool_ended_with_turn(
            tool_call_id,
            Some(tool_name),
            Some(result),
            Vec::new(),
            None,
            is_error,
            None,
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_tool_ended_with_turn(
        &mut self,
        tool_call_id: String,
        tool_name: Option<String>,
        result: Option<serde_json::Value>,
        result_content: Vec<ContentBlock>,
        result_details: Option<serde_json::Value>,
        is_error: bool,
        source_turn_id: Option<String>,
        transcript_seq: Option<u64>,
    ) {
        let status = if is_error {
            ToolStatus::Failed
        } else {
            ToolStatus::Completed
        };
        if let Some(&idx) = self.tool_ids.get(&tool_call_id) {
            let recovered = self.committed_tool_args(&tool_call_id);
            if let TimelineItem::Tool(tool) = &mut self.items[idx] {
                if let Some(name) = tool_name.filter(|name| !name.is_empty()) {
                    tool.tool_name = name;
                }
                if result.is_some() {
                    tool.result = result;
                }
                if !result_content.is_empty() {
                    tool.result_content = result_content;
                }
                if result_details.is_some() {
                    tool.result_details = result_details;
                }
                if source_turn_id.is_some() {
                    tool.source_turn_id = source_turn_id;
                }
                tool.transcript_seq = min_seq(tool.transcript_seq, transcript_seq);
                if tool.args.is_null()
                    && let Some(args) = recovered
                {
                    tool.args = args;
                }
                seal_streamed_args(tool);
                tool.status = status;
            }
            self.maintenance();
            return;
        }

        let live_order = self.allocate_live_order();
        let args = self
            .committed_tool_args(&tool_call_id)
            .unwrap_or(serde_json::Value::Null);
        self.items.push(TimelineItem::Tool(Box::new(ToolItem {
            tool_call_id: tool_call_id.clone(),
            tool_name: tool_name.unwrap_or_else(|| "tool".to_string()),
            args,
            partial_json: None,
            argument_segments: Vec::new(),
            result,
            result_content,
            result_details,
            status,
            parent_message_id: None,
            source_turn_id,
            transcript_seq,
            live_order,
            upstream: None,
            upstream_split: None,
        })));
        self.tool_ids
            .insert(tool_call_id.clone(), self.items.len() - 1);
        self.maintenance();
    }

    pub fn apply_tool_arg_chunk(
        &mut self,
        tool_call_id: String,
        chunk: &str,
        parent_message_id: Option<String>,
    ) {
        self.apply_tool_content(
            tool_call_id,
            StreamItemOp::AppendChunk,
            0,
            Some(chunk),
            parent_message_id,
            None,
        );
    }

    pub(super) fn apply_tool_content(
        &mut self,
        tool_call_id: String,
        op: StreamItemOp,
        content_index: u32,
        text: Option<&str>,
        parent_message_id: Option<String>,
        source_turn_id: Option<String>,
    ) {
        if !self.tool_ids.contains_key(&tool_call_id) {
            let live_order = self.allocate_live_order();
            self.items.push(TimelineItem::Tool(Box::new(ToolItem {
                tool_call_id: tool_call_id.clone(),
                tool_name: String::new(),
                args: serde_json::Value::Null,
                partial_json: None,
                argument_segments: Vec::new(),
                result: None,
                result_content: Vec::new(),
                result_details: None,
                status: ToolStatus::Running,
                parent_message_id: parent_message_id.clone(),
                source_turn_id: source_turn_id.clone(),
                transcript_seq: None,
                live_order,
                upstream: None,
                upstream_split: None,
            })));
            self.tool_ids
                .insert(tool_call_id.clone(), self.items.len() - 1);
            self.maintenance_indexes_only();
        }
        let idx = self.tool_ids[&tool_call_id];
        let TimelineItem::Tool(tool) = &mut self.items[idx] else {
            return;
        };
        if tool.status != ToolStatus::Running {
            return;
        }
        if tool.parent_message_id.is_none() {
            tool.parent_message_id = parent_message_id;
        }
        if tool.source_turn_id.is_none() {
            tool.source_turn_id = source_turn_id;
        }
        let index = content_index as usize;
        match op {
            StreamItemOp::AppendChunk | StreamItemOp::ReplaceContent => {
                while tool.argument_segments.len() <= index {
                    tool.argument_segments.push(String::new());
                }
                if op == StreamItemOp::AppendChunk {
                    tool.argument_segments[index].push_str(text.unwrap_or_default());
                } else {
                    tool.argument_segments[index] = text.unwrap_or_default().to_string();
                }
            }
            StreamItemOp::ClearContent => {
                if tool.argument_segments.len() > index {
                    tool.argument_segments[index].clear();
                }
            }
            StreamItemOp::Upsert => {}
        }
        tool.partial_json = Some(tool.argument_segments.concat());
    }

    fn committed_tool_args(&self, tool_call_id: &str) -> Option<serde_json::Value> {
        self.committed_records
            .values()
            .find_map(|record| match &record.message {
                Message::ToolCall { id, arguments, .. }
                    if id == tool_call_id && !arguments.is_null() =>
                {
                    Some(arguments.clone())
                }
                _ => None,
            })
    }

    /// Tag a tool timeline item as provider-side ("upstream") and update its
    /// arguments when a later lifecycle block carries them. The tool item is
    /// keyed by `activity_id`, so repeated blocks update the same card.
    pub(super) fn mark_upstream(
        &mut self,
        tool_call_id: &str,
        upstream: ToolUpstream,
        args: Option<serde_json::Value>,
    ) {
        let Some(&idx) = self.tool_ids.get(tool_call_id) else {
            return;
        };
        if let TimelineItem::Tool(tool) = &mut self.items[idx] {
            tool.upstream = Some(upstream);
            if let Some(args) = args {
                tool.args = args;
                tool.partial_json = None;
                tool.argument_segments.clear();
            }
        }
    }

    /// Snapshot the parent draft's text/thinking so the projection can split
    /// the single streaming message into  before → card → after.
    pub(super) fn capture_upstream_split(&mut self, tool_call_id: &str) {
        let Some(&idx) = self.tool_ids.get(tool_call_id) else {
            return;
        };
        if let TimelineItem::Tool(tool) = &self.items[idx]
            && tool.upstream_split.is_some()
        {
            return;
        }
        let parent = match &self.items[idx] {
            TimelineItem::Tool(tool) => tool.parent_message_id.clone(),
            _ => return,
        };
        let Some(parent) = parent else {
            return;
        };
        let split = {
            let Some(&draft_idx) = self.draft_ids.get(&parent) else {
                return;
            };
            match &self.items[draft_idx] {
                TimelineItem::RealtimeDraft(draft) => UpstreamSplit {
                    before_text: draft.text(),
                    before_thinking: draft.thinking(),
                },
                _ => return,
            }
        };
        if let TimelineItem::Tool(tool) = &mut self.items[idx] {
            tool.upstream_split = Some(split);
        }
    }

    /// Lift upstream tool activity/approval content blocks out of a committed
    /// assistant message into timeline tool cards. Cards are keyed by
    /// `activity_id` / `approval_id`, so repeated lifecycle blocks update one
    /// card in place and a live streamed card is folded into the durable one.
    pub(super) fn upsert_committed_upstream(
        &mut self,
        content: &[ContentBlock],
        transcript_seq: u64,
        source_turn_id: &str,
    ) {
        use piko_protocol::messages::UpstreamActivityStatus;

        for block in content {
            match block {
                ContentBlock::UpstreamToolActivity {
                    activity_id,
                    tool_name,
                    kind,
                    status,
                    arguments,
                    action,
                } => {
                    let args = arguments.clone();
                    match status {
                        UpstreamActivityStatus::Completed => {
                            self.apply_tool_ended_with_turn(
                                activity_id.clone(),
                                Some(tool_name.clone()),
                                None,
                                Vec::new(),
                                None,
                                false,
                                Some(source_turn_id.to_string()),
                                Some(transcript_seq),
                            );
                        }
                        UpstreamActivityStatus::Failed => {
                            self.apply_tool_ended_with_turn(
                                activity_id.clone(),
                                Some(tool_name.clone()),
                                None,
                                Vec::new(),
                                None,
                                true,
                                Some(source_turn_id.to_string()),
                                Some(transcript_seq),
                            );
                        }
                        UpstreamActivityStatus::Started | UpstreamActivityStatus::InProgress => {
                            self.apply_tool_started_with_turn(
                                activity_id.clone(),
                                tool_name.clone(),
                                args.clone().unwrap_or(serde_json::Value::Null),
                                None,
                                Some(source_turn_id.to_string()),
                                Some(transcript_seq),
                            );
                        }
                    }
                    self.mark_upstream(
                        activity_id,
                        ToolUpstream {
                            kind: kind.clone(),
                            summary: None,
                            action: action.clone(),
                        },
                        args,
                    );
                }
                ContentBlock::UpstreamToolApproval {
                    approval_id,
                    tool_name,
                    summary,
                } => {
                    self.apply_tool_started_with_turn(
                        approval_id.clone(),
                        tool_name.clone(),
                        serde_json::Value::Null,
                        None,
                        Some(source_turn_id.to_string()),
                        Some(transcript_seq),
                    );
                    self.mark_upstream(
                        approval_id,
                        ToolUpstream {
                            kind: String::new(),
                            summary: Some(summary.clone()),
                            action: None,
                        },
                        None,
                    );
                }
                _ => {}
            }
        }
    }
}

fn seal_streamed_args(tool: &mut ToolItem) {
    if tool.args.is_null() {
        let joined = tool.argument_segments.concat();
        let raw = if joined.is_empty() {
            tool.partial_json.as_deref().unwrap_or("")
        } else {
            joined.as_str()
        };
        if !raw.is_empty() {
            tool.args = serde_json::from_str(raw)
                .unwrap_or_else(|_| serde_json::Value::String(raw.to_string()));
        }
    }
    tool.partial_json = None;
    tool.argument_segments.clear();
}

fn min_seq(current: Option<u64>, next: Option<u64>) -> Option<u64> {
    match (current, next) {
        (Some(current), Some(next)) => Some(current.min(next)),
        (current, next) => current.or(next),
    }
}
