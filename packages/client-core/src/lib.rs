//! Headless client projection and product intents for hostd.
//!
//! Depends on `piko-protocol` only. No GPUI, process, or async-runtime types.

#![forbid(unsafe_code)]

pub mod attention;
pub mod branch;
pub mod effect;
pub mod foreground;
pub mod intent;
pub mod msg;
pub mod state;
pub mod timeline;
pub mod update;
pub mod usage;

pub use attention::{
    AttentionItem, AttentionKind, find_approval, find_interaction, front_prompt,
    front_prompt_from_state, prompt_queue, prompt_queue_from_state,
};
pub use branch::{active_branch_entries, active_path_ids};
pub use effect::ClientEffect;
pub use foreground::{agent_foreground, focused_or_any_busy};
pub use intent::ClientIntent;
pub use msg::{ClientMsg, TransportObservation};
pub use state::{ClientState, ConnectionState, LiveSession, ModelState, SessionPhase};
pub use timeline::{
    AgentTimeline, ApplyOutcome, CommittedItem, RealtimeDraft, TimelineItem, ToolItem, ToolStatus,
};
pub use update::{CommandIdSource, UpdateContext, update};
pub use usage::{
    context_fill_from_usage, format_context, format_cost, format_tokens, usage_has_signal,
};
