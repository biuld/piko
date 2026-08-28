//! Structured committed and realtime timeline projection.
//!
//! Provides deduplication of committed items and draft superseding when a
//! committed message arrives for an existing realtime draft.

use std::collections::HashMap;

use piko_protocol::agent_runtime::RealtimeDelta;
use piko_protocol::execution::ModelStepBoundary;
use piko_protocol::messages::{ContentBlock, Message, UpstreamAction};
use piko_protocol::{MessageId, SessionTreeEntry, StreamItemKind, StreamItemOp};

/// A single timeline item: either committed (authoritative) or a realtime draft.
#[derive(Debug, Clone)]
pub enum TimelineItem {
    Committed(Box<CommittedItem>),
    RealtimeDraft(RealtimeDraft),
    Tool(Box<ToolItem>),
    SessionEntry(Box<SessionEntryItem>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TimelineOrderKey {
    ActiveBranch(u64),
    Transcript(u64),
    Live(u64),
}

impl TimelineItem {
    pub fn order_key(&self) -> TimelineOrderKey {
        match self {
            Self::Committed(item) => TimelineOrderKey::Transcript(item.transcript_seq),
            Self::RealtimeDraft(item) => TimelineOrderKey::Live(item.live_order),
            Self::Tool(item) => item
                .transcript_seq
                .map(TimelineOrderKey::Transcript)
                .unwrap_or(TimelineOrderKey::Live(item.live_order)),
            Self::SessionEntry(item) => TimelineOrderKey::ActiveBranch(item.branch_order),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
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
    /// Segment-preserving streamed arguments. `partial_json` is their joined
    /// compatibility view.
    pub argument_segments: Vec<String>,
    pub result: Option<serde_json::Value>,
    pub result_content: Vec<ContentBlock>,
    pub result_details: Option<serde_json::Value>,
    pub status: ToolStatus,
    pub parent_message_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub transcript_seq: Option<u64>,
    pub live_order: u64,
    /// Provider-side ("upstream") marker when this card represents an upstream
    /// tool activity/approval rather than a locally dispatched tool call.
    pub upstream: Option<ToolUpstream>,
    /// Live split anchor: the assistant text/thinking already emitted when this
    /// upstream tool started. Lets the projection render the single message
    /// draft as  text-before → card → text-after, mirroring a normal tool call.
    pub upstream_split: Option<UpstreamSplit>,
}

/// Upstream tool metadata surfaced on a timeline tool card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolUpstream {
    pub kind: String,
    /// Present for an upstream approval request.
    pub summary: Option<String>,
    /// Typed, cleaned action for a known upstream tool (e.g. `Search`/`OpenPage`).
    /// `None` for approvals or unknown action types, which stay opaque.
    pub action: Option<UpstreamAction>,
}

/// Before-snapshot captured when an upstream tool starts streaming.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamSplit {
    pub before_text: String,
    pub before_thinking: String,
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
    /// Text and thinking segments in first-seen order. A segment is addressed
    /// by `(kind, content_index)`; subsequent chunks mutate it in place.
    pub content_segments: Vec<RealtimeContentSegment>,
    pub live_order: u64,
    /// The currently open thinking segment, if the latest realtime content is
    /// still thinking. A non-thinking delta closes the previous segment even
    /// when its text content index is reused.
    pub active_thinking_index: Option<u32>,
    /// Realtime message lifecycle has reached its terminal frame. The
    /// committed message still supersedes this draft when it arrives.
    pub ended: bool,
    pub stop_reason: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeContentKind {
    Text,
    Thinking,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealtimeContentSegment {
    pub kind: RealtimeContentKind,
    pub content_index: u32,
    pub text: String,
}

impl RealtimeDraft {
    pub fn text(&self) -> String {
        self.content_for(RealtimeContentKind::Text)
    }

    pub fn thinking(&self) -> String {
        self.content_for(RealtimeContentKind::Thinking)
    }

    fn content_for(&self, kind: RealtimeContentKind) -> String {
        self.content_segments
            .iter()
            .filter(|segment| segment.kind == kind)
            .map(|segment| segment.text.as_str())
            .collect()
    }
}

/// A visible, durable non-message entry in active-branch order.
#[derive(Debug, Clone)]
pub struct SessionEntryItem {
    pub entry: SessionTreeEntry,
    pub branch_order: u64,
}

/// Per-agent timeline projection.
#[derive(Debug, Clone, Default)]
pub struct AgentTimeline {
    items: Vec<TimelineItem>,
    committed_records: HashMap<MessageId, CommittedItem>,
    model_steps: HashMap<String, ModelStepBoundary>,
    draft_ids: HashMap<MessageId, usize>,
    tool_ids: HashMap<String, usize>,
    session_entry_ids: HashMap<String, usize>,
    next_live_order: u64,
    /// While > 0, mutations skip the per-item index rebuild / authored
    /// reorder; `end_batch` performs both once. Index maps used for lookups
    /// are still maintained incrementally inside the batch.
    ///
    /// Batch input must be ordered like a hydrated snapshot: a ToolCall (or
    /// tool-start) entry must precede its ToolResult so incremental
    /// `tool_ids` lookup pairs them. Realtime streams never enter batches.
    batch_depth: u32,
}

mod content;
mod impls;
#[cfg(test)]
mod tests;
mod tools;
#[cfg(test)]
mod upstream_tests;
