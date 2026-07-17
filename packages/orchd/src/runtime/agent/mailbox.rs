use piko_comms::contracts::{
    AgentCommandReply, AgentCommands, AgentSnapshot as AgentSnapshotContract,
};
use piko_comms::{LatestReceiver, MailboxSender, ReplySender};
use piko_orchd_api::{AgentApiError, AgentRunAcceptance};
use piko_protocol::{
    AgentInboxSnapshot, AgentInputReceipt, AgentInstanceLifecycle, AgentLifecycleReceipt,
    AgentSnapshot, SendAgentInputRequest,
};

use crate::runtime::execution::ExecutionTerminal;
use crate::runtime::reliability::{DetachedDeliveryScope, ExecutionHandoffLease, RunCancellation};

#[derive(Clone)]
pub struct DetachedReportTarget {
    pub agent_instance_id: String,
}

pub enum AgentCommand {
    Input {
        request: SendAgentInputRequest,
        reply: ReplySender<AgentCommandReply, Result<AgentInputReceipt, AgentApiError>>,
    },
    Run {
        request: SendAgentInputRequest,
        reply: ReplySender<AgentCommandReply, Result<AgentRunAcceptance, AgentApiError>>,
    },
    InputDetached {
        request: SendAgentInputRequest,
        recipient: DetachedReportTarget,
        reply: ReplySender<AgentCommandReply, Result<AgentInputReceipt, AgentApiError>>,
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
        reply: ReplySender<AgentCommandReply, Result<AgentLifecycleReceipt, AgentApiError>>,
    },
    CancelRun {
        request_id: String,
        reply: ReplySender<
            AgentCommandReply,
            Result<piko_protocol::AgentCancelReceipt, AgentApiError>,
        >,
    },
    CancelInput {
        request_id: String,
        reply: ReplySender<
            AgentCommandReply,
            Result<piko_protocol::AgentCancelReceipt, AgentApiError>,
        >,
    },
    Inbox {
        reply: ReplySender<AgentCommandReply, AgentInboxSnapshot>,
    },
    ConsumeInbox {
        request: piko_protocol::ConsumeAgentInboxRequest,
        reply: ReplySender<
            AgentCommandReply,
            Result<piko_protocol::ConsumeAgentInboxReceipt, AgentApiError>,
        >,
    },
    Shutdown {
        reply: ReplySender<AgentCommandReply, ()>,
    },
}

#[derive(Clone)]
pub struct AgentHandle {
    pub generation: u64,
    pub command_tx: MailboxSender<AgentCommands, AgentCommand>,
    pub snapshot_rx: LatestReceiver<AgentSnapshotContract, AgentSnapshot>,
    pub(crate) run_cancellation: std::sync::Arc<RunCancellation>,
}
