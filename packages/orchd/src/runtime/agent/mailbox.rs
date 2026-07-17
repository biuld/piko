use orchd_api::{AgentApiError, AgentRunAcceptance};
use piko_protocol::{
    AgentInboxSnapshot, AgentInputReceipt, AgentInstanceLifecycle, AgentLifecycleReceipt,
    AgentSnapshot, SendAgentInputRequest,
};
use tokio::sync::{mpsc, oneshot, watch};

use crate::runtime::execution::ExecutionTerminal;
use crate::runtime::reliability::{DetachedDeliveryScope, ExecutionHandoffLease, RunCancellation};

#[derive(Clone)]
pub struct DetachedReportTarget {
    pub agent_instance_id: String,
}

pub enum AgentCommand {
    Input {
        request: SendAgentInputRequest,
        reply: oneshot::Sender<Result<AgentInputReceipt, AgentApiError>>,
    },
    Run {
        request: SendAgentInputRequest,
        reply: oneshot::Sender<Result<AgentRunAcceptance, AgentApiError>>,
    },
    InputDetached {
        request: SendAgentInputRequest,
        recipient: DetachedReportTarget,
        reply: oneshot::Sender<Result<AgentInputReceipt, AgentApiError>>,
    },
    ExecutionFinished {
        execution_id: String,
        terminal: ExecutionHandoffLease<ExecutionTerminal>,
    },
    RetryTerminal {
        execution_id: String,
    },
    RetryQueuedInput,
    RetryDetachedReport {
        delivery: DetachedDeliveryScope,
    },
    InboxReport {
        item: piko_protocol::AgentInboxItem,
    },
    SetLifecycle {
        request_id: String,
        lifecycle: AgentInstanceLifecycle,
        reply: oneshot::Sender<Result<AgentLifecycleReceipt, AgentApiError>>,
    },
    CancelRun {
        request_id: String,
        reply: oneshot::Sender<Result<piko_protocol::AgentCancelReceipt, AgentApiError>>,
    },
    CancelInput {
        request_id: String,
        reply: oneshot::Sender<Result<piko_protocol::AgentCancelReceipt, AgentApiError>>,
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
    pub(crate) run_cancellation: std::sync::Arc<RunCancellation>,
}
