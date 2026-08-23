use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
};

use piko_protocol::TranscriptCommittedEvent;

use crate::{
    app::{HitId, command::TimelineAction},
    ui::interaction::{ComponentHit, PointerComponent, PointerGesture},
};

mod component;
mod highlight;
mod layout;
mod line_cache;
mod markdown;
mod projection;
mod render;
mod render_diff;
mod store;
mod tool_format;
mod viewport;

#[cfg(test)]
pub use component::TimelineKind;
pub use component::{
    AssistantMessageComponent, ComponentId, ContentBlock, CustomMessageComponent, ErrorComponent,
    SessionFactComponent, SummaryComponent, SummaryKind, TimelineComponent, TimelineEntry,
    ToolEntry, UpstreamInfo, UserMessageComponent,
};
pub(crate) use layout::TimelineRenderPlan;
pub use store::TimelineStore;
pub use viewport::ScrollViewport;

const MAX_COMPONENTS: usize = 500;
pub(crate) const WHEEL_STEP: usize = 3;

/// In-memory component stream plus viewport/presentation state.
pub struct Timeline {
    pub components: VecDeque<TimelineComponent>,
    pub viewport: ScrollViewport,
    pub thinking_visible: bool,
    /// Running and completed tool calls, kept for status lookup.
    pub tool_calls: Vec<ToolEntry>,
    /// Stable hit identity for tool calls: tool call id → local hit id. Kept
    /// across projection rebuilds so pointer hits never target the wrong tool.
    hit_ids: HashMap<String, u64>,
    next_hit_id: u64,
    /// Bumped by every mutation that can change render-plan geometry
    /// (`lines` / `row_owners`). Scroll does not bump it.
    layout_epoch: u64,
    projection: piko_client_core::AgentTimeline,
    next_local_id: u64,
    line_cache: RefCell<line_cache::LineCache>,
    projection_dirty: bool,
    defer_projection_sync: bool,
}

mod internals;
mod timeline_impl;

impl PointerComponent<HitId> for Timeline {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        match (gesture, hit.element) {
            (PointerGesture::ScrollUp, _) => {
                vec![TimelineAction::ScrollUp(WHEEL_STEP).into()]
            }
            (PointerGesture::ScrollDown, _) => {
                vec![TimelineAction::ScrollDown(WHEEL_STEP).into()]
            }
            (PointerGesture::Activate, Some(HitId::TimelineTool(hit_id))) => {
                vec![TimelineAction::ToggleTool(hit_id).into()]
            }
            (PointerGesture::Activate, _) => Vec::new(),
        }
    }
}
