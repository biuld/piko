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
                tool.status = status;
            }
            return;
        }

        let idx = self.items.len();
        self.items.push(TimelineItem::Tool(ToolItem {
            tool_call_id: tool_call_id.clone(),
            tool_name,
            args: serde_json::Value::Null,
            result: Some(result),
            status,
            parent_message_id: None,
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

    /// Apply a realtime delta. Creates a draft if not already committed.
    /// Returns false if this message_id is already committed (ignore further
    /// realtime for it).
    pub fn apply_realtime(
        &mut self,
        message_id: MessageId,
        delta_seq: u64,
        delta: &RealtimeDelta,
    ) -> bool {
        !matches!(
            self.apply_realtime_checked(message_id, delta_seq, delta),
            ApplyOutcome::Ignored
        )
    }

    pub fn apply_realtime_checked(
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
            self.draft_ids.insert(message_id, idx);
            idx
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
        ApplyOutcome::Applied
    }

    /// Clear all items (used on reconcile or agent switch).
    pub fn clear(&mut self) {
        self.items.clear();
        self.committed_ids.clear();
        self.draft_ids.clear();
        self.tool_ids.clear();
    }
}
