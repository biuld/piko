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
            self.reorder_authored_items();
            return;
        }

        let live_order = self.allocate_live_order();
        self.items.push(TimelineItem::Tool(Box::new(ToolItem {
            tool_call_id,
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
        })));
        self.rebuild_indexes();
        self.reorder_authored_items();
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
                tool.partial_json = None;
                tool.argument_segments.clear();
                tool.status = status;
            }
            self.reorder_authored_items();
            return;
        }

        let live_order = self.allocate_live_order();
        self.items.push(TimelineItem::Tool(Box::new(ToolItem {
            tool_call_id,
            tool_name: tool_name.unwrap_or_else(|| "tool".to_string()),
            args: serde_json::Value::Null,
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
        })));
        self.rebuild_indexes();
        self.reorder_authored_items();
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
            })));
            self.rebuild_indexes();
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

    pub fn finish_turn(&mut self, turn_id: &str, terminal: ToolStatus) {
        debug_assert!(matches!(
            terminal,
            ToolStatus::Failed | ToolStatus::Cancelled
        ));
        for item in &mut self.items {
            if let TimelineItem::Tool(tool) = item
                && tool.status == ToolStatus::Running
                && tool.source_turn_id.as_deref() == Some(turn_id)
            {
                tool.status = terminal;
                tool.partial_json = None;
                tool.argument_segments.clear();
            }
        }
    }
}

fn min_seq(current: Option<u64>, next: Option<u64>) -> Option<u64> {
    match (current, next) {
        (Some(current), Some(next)) => Some(current.min(next)),
        (current, next) => current.or(next),
    }
}
