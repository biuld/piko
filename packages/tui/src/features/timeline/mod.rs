use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
};

use piko_protocol::{ModelStepBoundary, TranscriptCommittedEvent};

use crate::{
    app::{HitId, command::TimelineAction},
    ui::interaction::{ComponentHit, PointerComponent, PointerGesture},
};

mod component;
mod highlight;
mod layout;
#[cfg(test)]
mod layout_tests;
mod line_cache;
mod markdown;
mod model_step;
mod projection;
mod render;
mod render_diff;
mod selection;
mod store;
mod thought;
mod tool_format;
mod viewport;

#[cfg(test)]
mod timeline_test_api;

#[cfg(test)]
pub use component::TimelineKind;
pub use component::{
    AssistantMessageComponent, ComponentId, ContentBlock, CustomMessageComponent, ErrorComponent,
    ModelStepDividerComponent, SessionFactComponent, SummaryComponent, SummaryKind,
    ThoughtComponent, ThoughtKey, ThoughtPhase, TimelineComponent, TimelineEntry, ToolEntry,
    UpstreamInfo, UserMessageComponent,
};
pub(crate) use layout::TimelineRenderPlan;
pub(crate) use selection::SelectionPoint;
pub use store::TimelineStore;
pub(crate) use thought::{THOUGHT_SPINNER, elapsed_ms, format_duration_ms, phase_duration_ms};
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
    /// Message id → provider tool-call id, retained so model-step boundaries
    /// can anchor a divider even though client-core exposes tool items by call
    /// id rather than by their committed message id.
    tool_message_ids: HashMap<String, String>,
    /// Ordered boundaries received from hostd. client-core keeps the same
    /// facts keyed by id; the TUI also needs arrival order for presentation.
    model_step_boundaries: Vec<ModelStepBoundary>,
    /// Stable hit identity for semantic thought rows.
    thought_hit_ids: HashMap<ThoughtKey, u64>,
    /// TUI-local monotonic start times for live thought rows.
    thought_starts: HashMap<ThoughtKey, std::time::Instant>,
    next_hit_id: u64,
    /// Bumped by every mutation that can change render-plan geometry
    /// (`lines` / content ownership). Scroll does not bump it.
    layout_epoch: u64,
    projection: piko_client_core::AgentTimeline,
    next_local_id: u64,
    line_cache: RefCell<line_cache::LineCache>,
    selection: RefCell<selection::TimelineSelection>,
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
            (PointerGesture::Activate, Some(HitId::TimelineThought(hit_id))) => {
                vec![TimelineAction::OpenThought(hit_id).into()]
            }
            (PointerGesture::Activate, _) => Vec::new(),
        }
    }
}
