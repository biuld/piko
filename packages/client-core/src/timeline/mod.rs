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

mod impls;
#[cfg(test)]
mod tests;
