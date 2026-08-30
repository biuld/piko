mod agent_work_diff;
mod agents;
mod host;
mod snapshot;
mod transcript;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use agent_work_diff::{
    file_change_from_message, merge_file_change, render_agent_work_diff,
};
pub use transcript::transcript_messages_from_session_entries;
pub use types::{AgentViewState, HostState, SessionModelRef, SessionState};
