//! Virtualized frame resolution (F-45/F-46 scroll performance). One
//! allocation-light pass per paint groups backing items into presentation
//! rows; visible-turn payloads map on demand through [`rows_around`], so
//! per-frame cost tracks the viewport instead of conversation length.

use std::rc::Rc;

use super::{
    ClientState, TimelineRow, TimelineState, groups_items, load_error_state, map_group,
    selected_timeline,
};
use piko_client_core::state::SessionPhase;
use piko_client_core::timeline::{TimelineItem, ToolStatus};

/// Per-frame virtualized timeline: item-run groups plus streaming flag.
#[derive(Clone)]
pub struct FrameTimeline {
    /// Contiguous `(start, end)` item-index runs; each run renders exactly
    /// one list row (assistant-side items collapse into a single bubble).
    pub groups: Rc<Vec<(usize, usize)>>,
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
            streaming: false,
            composer_padding,
        }
    }

    /// Total presentation rows across all groups.
    pub fn total(&self) -> usize {
        self.groups.len()
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
    let state = if groups.is_empty() {
        TimelineState::Empty
    } else {
        TimelineState::Ready
    };
    (
        state,
        FrameTimeline {
            groups,
            streaming,
            composer_padding,
        },
    )
}

/// Map presentation rows for list index `ix` and its predecessor. Only the
/// addressed group (and the previous one for the gap) maps payloads, so a
/// paint clones content for the viewport turns only.
pub fn rows_around(
    core: &ClientState,
    frame: &FrameTimeline,
    ix: usize,
) -> Option<(Option<TimelineRow>, TimelineRow)> {
    let timeline = selected_timeline(core)?;
    let items = timeline.items();
    debug_assert_eq!(frame.groups.len(), frame.total());
    if ix >= frame.total() {
        return None;
    }
    let range = frame.groups[ix];
    let cur = map_group(items, range)?;
    let prev = if ix == 0 {
        None
    } else {
        map_group(items, frame.groups[ix - 1])
    };
    Some((prev, cur))
}
