//! Structured committed and realtime timeline projection.
//!
//! Provides deduplication of committed items and draft superseding when a
//! committed message arrives for an existing realtime draft.

use std::collections::HashMap;

use piko_protocol::MessageId;
use piko_protocol::agent_runtime::RealtimeDelta;
use piko_protocol::messages::Message;

/// A single timeline item: either committed (authoritative) or a realtime draft.
#[derive(Debug, Clone)]
pub enum TimelineItem {
    Committed(Box<CommittedItem>),
    RealtimeDraft(RealtimeDraft),
    Tool(ToolItem),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied,
    Ignored,
    Inconsistent,
}

/// Authoritative tool lifecycle projected from tree replay and live events.
#[derive(Debug, Clone)]
pub struct ToolItem {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    /// Streaming tool-call argument JSON (F-22 stream append chunks).
    pub partial_json: Option<String>,
    pub result: Option<serde_json::Value>,
    pub status: ToolStatus,
    pub parent_message_id: Option<String>,
}

/// An authoritative committed transcript entry.
#[derive(Debug, Clone)]
pub struct CommittedItem {
    pub message_id: MessageId,
    pub transcript_seq: u64,
    pub message: Message,
    pub source_turn_id: String,
}

/// An ephemeral realtime draft assembled from deltas.
#[derive(Debug, Clone)]
pub struct RealtimeDraft {
    pub message_id: MessageId,
    pub last_delta_seq: u64,
    pub text_segments: Vec<String>,
    pub thinking_segments: Vec<String>,
}

/// Per-agent timeline projection.
#[derive(Debug, Clone, Default)]
pub struct AgentTimeline {
    items: Vec<TimelineItem>,
    committed_ids: HashMap<MessageId, usize>,
    draft_ids: HashMap<MessageId, usize>,
    tool_ids: HashMap<String, usize>,
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::agent_runtime::RealtimeDelta;

    #[test]
    fn tool_arg_chunks_upsert_by_tool_call_id() {
        let mut tl = AgentTimeline::new();
        for (seq, chunk) in [(1u64, "{\"path\":"), (2, "\"a\"}")] {
            let patch = piko_protocol::StreamItemPatch::from_realtime_delta(
                Some("s".into()),
                Some("root".into()),
                "msg-1",
                Some(seq),
                &RealtimeDelta::ToolCall {
                    content_index: 0,
                    tool_call_id: "call-1".into(),
                    delta: chunk.into(),
                },
            )
            .into_iter()
            .next()
            .unwrap();
            assert!(matches!(
                tl.apply_stream_item(&patch),
                ApplyOutcome::Applied
            ));
        }

        // ToolCall chunks also open a RealtimeDraft for message-level seq tracking.
        let tool = tl
            .items()
            .iter()
            .find_map(|item| match item {
                TimelineItem::Tool(t) if t.tool_call_id == "call-1" => Some(t),
                _ => None,
            })
            .expect("expected tool item");
        assert_eq!(tool.partial_json.as_deref(), Some("{\"path\":\"a\"}"));
        assert_eq!(tool.status, ToolStatus::Running);
        assert_eq!(tool.parent_message_id.as_deref(), Some("msg-1"));
    }

    #[test]
    fn tool_ended_clears_partial_json() {
        let mut tl = AgentTimeline::new();
        tl.apply_tool_arg_chunk("call-1".into(), "{", Some("msg".into()));
        tl.apply_tool_ended(
            "call-1".into(),
            "read".into(),
            serde_json::json!({"ok": true}),
            false,
        );
        let TimelineItem::Tool(tool) = &tl.items()[0] else {
            panic!("expected tool");
        };
        assert!(tool.partial_json.is_none());
        assert_eq!(tool.status, ToolStatus::Completed);
    }

    #[test]
    fn stream_item_applies_text_chunk() {
        let mut tl = AgentTimeline::new();
        let patch = piko_protocol::StreamItemPatch::from_realtime_delta(
            Some("s1".into()),
            Some("root".into()),
            "msg-1",
            Some(1),
            &RealtimeDelta::Text {
                content_index: 0,
                delta: "hi".into(),
            },
        )
        .into_iter()
        .next()
        .unwrap();
        assert!(matches!(
            tl.apply_stream_item(&patch),
            ApplyOutcome::Applied
        ));
        // Same seq is ignored (idempotent re-delivery).
        assert!(matches!(
            tl.apply_stream_item(&patch),
            ApplyOutcome::Ignored
        ));
        let TimelineItem::RealtimeDraft(draft) = &tl.items()[0] else {
            panic!("expected draft");
        };
        assert_eq!(draft.text_segments[0], "hi");
    }

    #[test]
    fn stream_item_tool_upsert_starts_tool() {
        let mut tl = AgentTimeline::new();
        let patch = piko_protocol::StreamItemPatch {
            session_id: Some("s".into()),
            agent_instance_id: Some("root".into()),
            item_id: "call-1".into(),
            item_kind: piko_protocol::StreamItemKind::ToolCall,
            op: piko_protocol::StreamItemOp::Upsert,
            text: None,
            content_index: None,
            delta_seq: None,
            fields: Some(serde_json::json!({
                "toolName": "read",
                "args": {"path": "a"},
                "status": "running",
                "parentMessageId": "msg",
            })),
        };
        assert!(matches!(
            tl.apply_stream_item(&patch),
            ApplyOutcome::Applied
        ));
        let TimelineItem::Tool(tool) = &tl.items()[0] else {
            panic!("expected tool");
        };
        assert_eq!(tool.tool_name, "read");
        assert_eq!(tool.status, ToolStatus::Running);
    }
}
