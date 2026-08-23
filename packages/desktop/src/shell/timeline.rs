//! Timeline surface: target-keyed states derived from the client-core
//! projection (D-59 Slice 2). One assistant turn maps to a single bubble row
//! whose segments preserve chronological order (F-46 / D-63).

#[cfg(test)]
mod tests;

pub mod frame;

pub use self::frame::{FrameTimeline, frame_timeline, rows_around};

use piko_client_core::{
    ClientState,
    state::SessionPhase,
    timeline::{AgentTimeline, RealtimeContentKind, TimelineItem, ToolItem, ToolStatus},
};
use piko_protocol::{ContentBlock, Message, MessageContent, session::SessionTreeEntry};

/// Target-keyed presentation states. Stale content is never shown as the
/// current target during loading or failure (F-42).
#[derive(Debug, PartialEq)]
pub enum TimelineState {
    NoSession,
    Loading,
    Error(String),
    Empty,
    /// Rows exist; payloads read on demand via [`rows_around`].
    Ready,
}

/// Coarse visual taxonomy used for inter-row rhythm (`row_gap_before`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    User,
    Assistant,
    System,
}

/// One presentation row. An assistant turn splits into consecutive pieces
/// (each at most one text block plus its preceding chip run) so virtualized
/// scrolling never materializes an unbounded turn; pieces of one turn share
/// `turn_id` and pack visually into a single bubble (F-46).
#[derive(Debug, Clone, PartialEq)]
pub enum TimelineRow {
    User {
        id: String,
        text: String,
    },
    Assistant {
        id: String,
        turn_id: String,
        leads_turn: bool,
        /// True for the trailing piece of the turn (bottom corners round).
        ends_turn: bool,
        segments: Vec<TurnSegment>,
    },
    System {
        id: String,
        label: String,
    },
}

/// Chronological piece of an assistant turn (F-46).
#[derive(Debug, Clone, PartialEq)]
pub enum TurnSegment {
    /// Activity chip; `active` drives the spinner on the live draft tail.
    Thinking {
        id: String,
        text: String,
        active: bool,
    },
    /// Activity chip; body resolves into the detail overlay on demand.
    Tool {
        id: String,
        name: String,
        status: ToolStatus,
    },
    /// Markdown response text; `caret` marks the streaming tail.
    Text {
        id: String,
        text: String,
        caret: bool,
    },
}

impl TimelineRow {
    pub fn id(&self) -> &str {
        match self {
            Self::User { id, .. } | Self::Assistant { id, .. } | Self::System { id, .. } => id,
        }
    }

    /// Turn identity for same-bubble packing (`Assistant` rows only).
    pub fn turn_id(&self) -> Option<&str> {
        match self {
            Self::Assistant { turn_id, .. } => Some(turn_id),
            _ => None,
        }
    }
}

pub fn row_kind(row: &TimelineRow) -> RowKind {
    match row {
        TimelineRow::User { .. } => RowKind::User,
        TimelineRow::Assistant { .. } => RowKind::Assistant,
        TimelineRow::System { .. } => RowKind::System,
    }
}

/// The selected agent's live timeline, when one is projected.
pub(super) fn selected_timeline(core: &ClientState) -> Option<&AgentTimeline> {
    if core.session_phase != SessionPhase::Live {
        return None;
    }
    let session = core.live_session.as_ref()?;
    let selected = session.selected_agent.as_ref()?;
    session.timelines.get(selected)
}

/// Resolve a tool item by call id for chip labels and detail overlays.
pub fn find_tool<'a>(core: &'a ClientState, call_id: &str) -> Option<&'a ToolItem> {
    let timeline = selected_timeline(core)?;
    timeline.items().iter().find_map(|item| match item {
        TimelineItem::Tool(tool) if tool.tool_call_id == call_id => Some(tool.as_ref()),
        _ => None,
    })
}

/// Joined plain text of a tool's streamed/committed result blocks.
pub fn tool_result_text(tool: &ToolItem) -> String {
    tool.result_content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn session_load_error(core: &ClientState) -> Option<TimelineState> {
    core.command_failures.iter().rev().find_map(|failure| {
        matches!(
            failure.operation,
            piko_client_core::state::PendingOp::Open { .. }
                | piko_client_core::state::PendingOp::Create
        )
        .then(|| TimelineState::Error(failure.message.clone()))
    })
}

pub(super) fn load_error_state(core: &ClientState) -> TimelineState {
    session_load_error(core).unwrap_or(TimelineState::NoSession)
}

/// Assistant-side items group into one bubble; anything else stands alone.
pub(super) fn groups_items(items: &[TimelineItem]) -> Vec<(usize, usize)> {
    fn assistant_side(item: &TimelineItem) -> bool {
        match item {
            TimelineItem::RealtimeDraft(_) | TimelineItem::Tool(_) => true,
            TimelineItem::Committed(committed) => {
                matches!(committed.message, Message::Assistant { .. })
            }
            _ => false,
        }
    }
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let extend = match groups.last_mut() {
            // Extend only when BOTH neighbours are assistant-side; otherwise
            // a user/system item would be absorbed into the next turn.
            Some((_, end))
                if assistant_side(item)
                    && *end == index
                    && index > 0
                    && assistant_side(&items[index - 1]) =>
            {
                *end = index + 1;
                true
            }
            _ => false,
        };
        if !extend {
            groups.push((index, index + 1));
        }
    }
    groups
}

/// Map one contiguous item run into its presentation rows: the turn splits
/// at text boundaries so every row stays bounded (virtualization).
pub(super) fn map_group_pieces(items: &[TimelineItem], range: (usize, usize)) -> Vec<TimelineRow> {
    let Some(slice) = items.get(range.0..range.1) else {
        return Vec::new();
    };
    match slice.first() {
        Some(TimelineItem::Committed(committed)) => match &committed.message {
            Message::User { content, .. } => vec![TimelineRow::User {
                id: format!("{}-user", committed.message_id),
                text: content_text(content),
            }],
            Message::Context { .. } => vec![TimelineRow::System {
                id: format!("{}-context", committed.message_id),
                label: "context".to_string(),
            }],
            _ => split_turn(slice, &committed.message_id),
        },
        Some(TimelineItem::RealtimeDraft(draft)) => split_turn(slice, &draft.message_id),
        _ => first_single(slice).into_iter().collect(),
    }
}

fn first_single(slice: &[TimelineItem]) -> Option<TimelineRow> {
    match slice.first()? {
        TimelineItem::Tool(tool) => Some(TimelineRow::Assistant {
            id: format!("{}#0", tool.tool_call_id),
            turn_id: tool.tool_call_id.clone(),
            leads_turn: true,
            ends_turn: true,
            segments: vec![TurnSegment::Tool {
                id: tool.tool_call_id.clone(),
                name: tool.tool_name.clone(),
                status: tool.status,
            }],
        }),
        TimelineItem::SessionEntry(entry) => {
            entry_label(&entry.entry).map(|label| TimelineRow::System {
                id: format!("session-entry-{}", entry.branch_order),
                label,
            })
        }
        _ => None,
    }
}

/// Merge an assistant-side run into chronological segments, then split into
/// pieces at text boundaries: piece k = pending chips + (text k-1). The
/// final text owns nothing extra; trailing chips flush as their own piece.
fn split_turn(slice: &[TimelineItem], fallback_id: &str) -> Vec<TimelineRow> {
    let (turn_id, segments) = assistant_segments(slice, fallback_id);
    let mut pieces: Vec<TimelineRow> = Vec::new();
    let mut chips: Vec<TurnSegment> = Vec::new();
    for segment in segments {
        match segment {
            TurnSegment::Text { .. } => {
                let mut owned = std::mem::take(&mut chips);
                owned.push(segment);
                pieces.push(TimelineRow::Assistant {
                    id: format!("{turn_id}#{}", pieces.len()),
                    turn_id: turn_id.clone(),
                    leads_turn: pieces.is_empty(),
                    ends_turn: false,
                    segments: owned,
                });
            }
            other => chips.push(other),
        }
    }
    if !chips.is_empty() {
        pieces.push(TimelineRow::Assistant {
            id: format!("{turn_id}#{}", pieces.len()),
            turn_id,
            leads_turn: false,
            ends_turn: true,
            segments: chips,
        });
    }
    if let Some(TimelineRow::Assistant { ends_turn, .. }) = pieces.last_mut() {
        *ends_turn = true;
    }
    if pieces.is_empty() {
        // Degenerate run (no mappable content): keep the group non-empty so
        // offsets stay aligned.
        pieces.push(TimelineRow::Assistant {
            id: format!("{fallback_id}-turn#0"),
            turn_id: format!("{fallback_id}-turn"),
            leads_turn: true,
            ends_turn: true,
            segments: Vec::new(),
        });
    }
    pieces
}

/// Merge an assistant-side run into chronological segments. Adjacent
/// same-kind pieces merge only within one source message. Streaming flags
/// land on the live draft's tail when it ends the run.
fn assistant_segments(slice: &[TimelineItem], fallback_id: &str) -> (String, Vec<TurnSegment>) {
    let mut segments: Vec<TurnSegment> = Vec::new();
    for item in slice {
        match item {
            TimelineItem::Committed(committed) => {
                if let Message::Assistant { content, .. } = &committed.message {
                    push_message_segments(&mut segments, &committed.message_id, content);
                }
            }
            TimelineItem::RealtimeDraft(draft) => {
                push_draft_segments(&mut segments, draft);
            }
            TimelineItem::Tool(tool) => segments.push(TurnSegment::Tool {
                id: tool.tool_call_id.clone(),
                name: tool.tool_name.clone(),
                status: tool.status,
            }),
            _ => {}
        }
    }
    // Live draft tail drives spinner / caret. Only when the draft ends the
    // run: later tools own their own spinners meanwhile.
    if let Some(TimelineItem::RealtimeDraft(draft)) = slice.last() {
        mark_draft_tail(&mut segments, draft);
    }
    (format!("{fallback_id}-turn"), segments)
}

fn push_message_segments(segments: &mut Vec<TurnSegment>, base: &str, blocks: &[ContentBlock]) {
    for block in blocks {
        match block {
            ContentBlock::Thinking { thinking, .. } => push_kind(
                segments,
                base,
                TurnSegment::Thinking {
                    id: seg_id(base, segments.len()),
                    text: thinking.clone(),
                    active: false,
                },
            ),
            ContentBlock::Text { text } => push_kind(
                segments,
                base,
                TurnSegment::Text {
                    id: seg_id(base, segments.len()),
                    text: text.clone(),
                    caret: false,
                },
            ),
            _ => {}
        }
    }
}

fn push_draft_segments(segments: &mut Vec<TurnSegment>, draft: &piko_client_core::RealtimeDraft) {
    let base = draft.message_id.clone();
    for segment in &draft.content_segments {
        let kind = segment.kind;
        let text = segment.text.clone();
        let next = match kind {
            RealtimeContentKind::Thinking => TurnSegment::Thinking {
                id: seg_id(&base, segments.len()),
                text,
                active: false,
            },
            RealtimeContentKind::Text => TurnSegment::Text {
                id: seg_id(&base, segments.len()),
                text,
                caret: false,
            },
        };
        push_kind(segments, &base, next);
    }
}

/// Merge into the previous segment only when it is the same kind from the
/// same source message; ids stay anchored to the first merged piece.
fn push_kind(segments: &mut Vec<TurnSegment>, base: &str, next: TurnSegment) {
    let merge = segments.last().is_some_and(|previous| {
        same_source(previous.id(), base)
            && matches!(
                (previous, &next),
                (TurnSegment::Thinking { .. }, TurnSegment::Thinking { .. })
                    | (TurnSegment::Text { .. }, TurnSegment::Text { .. })
            )
    });
    if merge {
        match (segments.last_mut().unwrap(), next) {
            (TurnSegment::Thinking { text, .. }, TurnSegment::Thinking { text: more, .. }) => {
                text.push('\n');
                text.push_str(&more);
            }
            (TurnSegment::Text { text, .. }, TurnSegment::Text { text: more, .. }) => {
                text.push('\n');
                text.push_str(&more);
            }
            _ => unreachable!(),
        }
    } else {
        segments.push(next);
    }
}

/// Segment ids are `{message-id}-s{n}`; a shared prefix binds them to one
/// source message.
fn same_source(id: &str, base: &str) -> bool {
    id.strip_prefix(base)
        .is_some_and(|rest| rest.starts_with('-'))
}

/// Point the streaming flags at the live draft's final segment.
fn mark_draft_tail(segments: &mut [TurnSegment], draft: &piko_client_core::RealtimeDraft) {
    let base = draft.message_id.as_str();
    let last = segments
        .iter()
        .rposition(|segment| segment.id().starts_with(&format!("{base}-")));
    let Some(index) = last else {
        return;
    };
    match &mut segments[index] {
        TurnSegment::Thinking { active, .. } => *active = true,
        TurnSegment::Text { caret, .. } => *caret = true,
        TurnSegment::Tool { .. } => {}
    }
}

fn seg_id(base: &str, index: usize) -> String {
    format!("{base}-s{index}")
}

impl TurnSegment {
    pub fn id(&self) -> &str {
        match self {
            Self::Thinking { id, .. } | Self::Tool { id, .. } | Self::Text { id, .. } => id,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Thinking { text, .. } | Self::Text { text, .. } => Some(text),
            Self::Tool { .. } => None,
        }
    }
}

fn entry_label(entry: &SessionTreeEntry) -> Option<String> {
    match entry {
        SessionTreeEntry::Compaction(_) => Some("Compaction".to_string()),
        SessionTreeEntry::BranchSummary(_) => Some("Branch summary".to_string()),
        SessionTreeEntry::ModelChange(change) => {
            Some(format!("Model · {}/{}", change.provider, change.model_id))
        }
        _ => None,
    }
}

fn content_text(content: &MessageContent) -> String {
    match content {
        MessageContent::String(text) => text.clone(),
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
