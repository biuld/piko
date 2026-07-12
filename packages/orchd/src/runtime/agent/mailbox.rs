use orchd_api::AgentApiError;
use piko_protocol::{
    AgentInboxSnapshot, AgentInputReceipt, AgentInstanceLifecycle, AgentLifecycleReceipt,
    AgentSnapshot, SendAgentInputRequest,
};
use tokio::sync::{mpsc, oneshot, watch};

pub enum AgentCommand {
    Input {
        request: SendAgentInputRequest,
        reply: oneshot::Sender<Result<AgentInputReceipt, AgentApiError>>,
    },
    ExecutionFinished {
        execution_id: String,
        terminal: super::super::execution::ExecutionTerminal,
    },
    InboxReport {
        item: piko_protocol::AgentInboxItem,
    },
    SetLifecycle {
        request_id: String,
        lifecycle: AgentInstanceLifecycle,
        reply: oneshot::Sender<Result<AgentLifecycleReceipt, AgentApiError>>,
    },
    Inbox {
        reply: oneshot::Sender<AgentInboxSnapshot>,
    },
    ConsumeInbox {
        request: piko_protocol::ConsumeAgentInboxRequest,
        reply: oneshot::Sender<Result<piko_protocol::ConsumeAgentInboxReceipt, AgentApiError>>,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}

#[derive(Clone)]
pub struct AgentHandle {
    pub generation: u64,
    pub command_tx: mpsc::Sender<AgentCommand>,
    pub snapshot_rx: watch::Receiver<AgentSnapshot>,
}
