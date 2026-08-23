//! Virtualized frame resolution (F-45/F-46 scroll performance). One
//! allocation-light pass per paint groups backing items into turns and
//! counts the presentation pieces each turn splits into; visible payloads
//! map on demand through [`rows_around`], so per-frame cost tracks the
//! viewport instead of conversation length.

use std::rc::Rc;

use super::{
    ClientState, TimelineRow, TimelineState, groups_items, load_error_state, map_group_pieces,
    selected_timeline,
};
use piko_client_core::state::SessionPhase;
use piko_client_core::timeline::{TimelineItem, ToolStatus};

/// Per-frame virtualized timeline: turn groups, piece offsets, streaming.
#[derive(Clone)]
pub struct FrameTimeline {
    /// Contiguous `(start, end)` item-index runs; each run is one turn.
    pub groups: Rc<Vec<(usize, usize)>>,
    /// `offsets[g]` is the number of list rows before group `g`; length is
    /// groups + 1 and the last entry is the total row count.
    pub offsets: Rc<Vec<usize>>,
    /// A realtime draft or running tool exists; the tail is remeasured.
    pub streaming: bool,
    /// Composer footprint inset for the last row's bottom padding.
    pub composer_padding: f32,
}

impl FrameTimeline {
    /// An empty frame with no rows (loading, error, or no session).
    pub fn empty(composer_padding: f32) -> Self {
        Self {
            groups: Rc::new(Vec::new()),
            offsets: Rc::new(vec![0]),
            streaming: false,
            composer_padding,
        }
    }

    /// Total presentation rows across all turns.
    pub fn total(&self) -> usize {
        self.offsets.last().copied().unwrap_or(0)
    }
}

/// Resolve the target-keyed state and the virtualized frame in one pass over
/// the projection. Payloads are never cloned here.
pub fn frame_timeline(core: &ClientState, composer_padding: f32) -> (TimelineState, FrameTimeline) {
    let empty = FrameTimeline::empty(composer_padding);
    match core.session_phase {
        SessionPhase::IdleNoSession => return (load_error_state(core), empty),
        SessionPhase::OpeningOrCreating { .. } | SessionPhase::Hydrating { .. } => {
            return (TimelineState::Loading, empty);
        }
        SessionPhase::Live => {}
    }
    if core.live_session.is_none() {
        return (TimelineState::Loading, empty);
    }
    let Some(timeline) = selected_timeline(core) else {
        return (TimelineState::Empty, empty);
    };
    let items = timeline.items();
    let mut streaming = false;
    for item in items {
        streaming |= match item {
            TimelineItem::RealtimeDraft(_) => true,
            TimelineItem::Tool(tool) => tool.status == ToolStatus::Running,
            _ => false,
        };
    }
    let groups = Rc::new(groups_items(items));
    let mut offsets = Vec::with_capacity(groups.len() + 1);
    offsets.push(0);
    for range in groups.iter() {
        let previous = offsets.last().copied().unwrap_or(0);
        offsets.push(previous + count_pieces(items, *range));
    }
    let offsets = Rc::new(offsets);
    let state = if offsets.last().is_some_and(|total| *total > 0) {
        TimelineState::Ready
    } else {
        TimelineState::Empty
    };
    (
        state,
        FrameTimeline {
            groups,
            offsets,
            streaming,
            composer_padding,
        },
    )
}

/// Map presentation rows for list index `ix` and its predecessor. Only the
/// addressed turn (and the previous turn at piece boundaries) maps payloads,
/// so a paint clones content for the viewport only.
pub fn rows_around(
    core: &ClientState,
    frame: &FrameTimeline,
    ix: usize,
) -> Option<(Option<TimelineRow>, TimelineRow)> {
    let timeline = selected_timeline(core)?;
    let items = timeline.items();
    debug_assert_eq!(frame.offsets.len(), frame.groups.len() + 1);
    if ix >= frame.total() {
        return None;
    }
    let g = frame
        .offsets
        .partition_point(|&start| start <= ix)
        .checked_sub(1)?;
    let pieces = map_group_pieces(items, frame.groups[g]);
    let local = ix - frame.offsets[g];
    let cur = pieces.get(local)?.clone();
    let prev = if local > 0 {
        pieces.get(local - 1).cloned()
    } else if g > 0 {
        map_group_pieces(items, frame.groups[g - 1]).pop()
    } else {
        None
    };
    Some((prev, cur))
}

/// Piece count for one turn without cloning payloads. Mirrors the merge and
/// split rules of [`super::map_group_pieces`]; parity is pinned in tests.
fn count_pieces(items: &[TimelineItem], range: (usize, usize)) -> usize {
    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        Think,
        Text,
        Chip,
    }
    let Some(slice) = items.get(range.0..range.1) else {
        return 0;
    };
    // Non-assistant runs are single rows by construction.
    let assistant_side = match slice.first() {
        Some(TimelineItem::RealtimeDraft(_)) | Some(TimelineItem::Tool(_)) => true,
        Some(TimelineItem::Committed(committed)) => {
            matches!(committed.message, piko_protocol::Message::Assistant { .. })
        }
        _ => false,
    };
    if !assistant_side {
        return 1;
    }
    let mut kinds: Vec<(&str, Kind)> = Vec::new();
    for item in slice {
        match item {
            TimelineItem::Committed(committed) => {
                if let piko_protocol::Message::Assistant { content, .. } = &committed.message {
                    for block in content {
                        match block {
                            piko_protocol::ContentBlock::Thinking { .. } => {
                                kinds.push((&committed.message_id, Kind::Think))
                            }
                            piko_protocol::ContentBlock::Text { .. } => {
                                kinds.push((&committed.message_id, Kind::Text))
                            }
                            _ => {}
                        }
                    }
                }
            }
            TimelineItem::RealtimeDraft(draft) => {
                for segment in &draft.content_segments {
                    let kind = match segment.kind {
                        piko_client_core::timeline::RealtimeContentKind::Thinking => Kind::Think,
                        piko_client_core::timeline::RealtimeContentKind::Text => Kind::Text,
                    };
                    kinds.push((draft.message_id.as_str(), kind));
                }
            }
            TimelineItem::Tool(tool) => kinds.push((tool.tool_call_id.as_str(), Kind::Chip)),
            _ => {}
        }
    }
    // Merge adjacent same-kind same-source entries exactly like push_kind.
    let mut merged: Vec<(&str, Kind)> = Vec::with_capacity(kinds.len());
    for entry in kinds {
        let merge = merged
            .last()
            .is_some_and(|(base, kind)| *base == entry.0 && *kind == entry.1);
        if !merge {
            merged.push(entry);
        }
    }
    // Pieces: one per merged text, plus a trailing chip-only piece when the
    // turn ends on thinking/tool activity (or carries no text at all).
    let text_pieces = merged
        .iter()
        .filter(|(_, kind)| *kind == Kind::Text)
        .count();
    let tail_chips = matches!(merged.last(), Some((_, kind)) if *kind != Kind::Text);
    text_pieces + usize::from(tail_chips) + usize::from(merged.is_empty())
}
