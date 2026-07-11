use piko_protocol::agent_runtime::{SubmitTaskInput, TaskControlRequest};

/// Internal delivery wrapper for runtime mailbox input.
#[derive(Debug)]
pub struct TaskInputEnvelope {
    pub input: SubmitTaskInput,
    pub ack_tx: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
}

impl TaskInputEnvelope {
    pub fn without_ack(input: SubmitTaskInput) -> Self {
        Self {
            input,
            ack_tx: None,
        }
    }

    pub fn complete_ack(&mut self, result: Result<(), String>) {
        if let Some(tx) = self.ack_tx.take() {
            let _ = tx.send(result);
        }
    }
}

/// Unified task mailbox used by the runtime state machine.
#[derive(Debug)]
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
