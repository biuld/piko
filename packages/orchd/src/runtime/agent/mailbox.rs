use piko_comms::contracts::{
    AgentCommandReply, AgentCommands, AgentSnapshot as AgentSnapshotContract,
};
use piko_comms::{LatestReceiver, MailboxSender, ReplySender};
use piko_orchd_api::AgentApiError;
use piko_protocol::{
    AgentInboxSnapshot, AgentInputReceipt, AgentInstanceLifecycle, AgentLifecycleReceipt,
    AgentSnapshot,
};

use crate::runtime::execution::ExecutionTerminal;
use crate::runtime::reliability::{DetachedDeliveryScope, ExecutionHandoffLease, RunCancellation};

#[derive(Clone)]
pub struct DetachedReportTarget {
    pub agent_instance_id: String,
}

pub enum AgentCommand {
    Input {
        input: piko_protocol::AgentInput,
        runtime: piko_orchd_api::AgentInputRuntime,
        reply: ReplySender<AgentCommandReply, Result<AgentInputReceipt, AgentApiError>>,
        parent: tracing::Span,
    },
    InputDetached {
        input: piko_protocol::AgentInput,
        recipient: DetachedReportTarget,
        reply: ReplySender<AgentCommandReply, Result<AgentInputReceipt, AgentApiError>>,
        parent: tracing::Span,
    },
    ExecutionFinished {
        root_input_id: String,
        terminal: ExecutionHandoffLease<ExecutionTerminal>,
    },
    RetryTerminal {
        root_input_id: String,
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
        input_id: String,
        reply: ReplySender<
            AgentCommandReply,
            Result<piko_protocol::AgentInputCancelReceipt, AgentApiError>,
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
    pub agent_kind: piko_protocol::AgentKind,
    pub command_tx: MailboxSender<AgentCommands, AgentCommand>,
    pub snapshot_rx: LatestReceiver<AgentSnapshotContract, AgentSnapshot>,
    pub(crate) run_cancellation: std::sync::Arc<RunCancellation>,
}
