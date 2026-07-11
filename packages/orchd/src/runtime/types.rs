use piko_protocol::agent_runtime::{SubmitTaskInput, TaskControlRequest};

use crate::runtime::dispatch::DispatchSenders;

/// Internal delivery wrapper for runtime channel linking.
#[derive(Debug, Clone)]
pub struct TaskInputEnvelope {
    pub input: SubmitTaskInput,
    pub senders: Option<DispatchSenders>,
}

/// Unified task mailbox used by the runtime state machine.
#[derive(Debug, Clone)]
pub(crate) enum TaskMailboxMessage {
    Input(TaskInputEnvelope),
    Control(TaskControlRequest),
}

/// A tool call entity produced by step dispatch and consumed by the tool executor.
#[derive(Clone, Debug)]
pub struct ToolCallItem {
    pub tool_call_index: u32,
    pub content_index: u32,
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}
