use piko_comms::contracts::{
    ExecutionCommandReply, ExecutionCommands, ExecutionTerminal as ExecutionTerminalContract,
};
use piko_comms::{MailboxSender, ReplyReceiver, ReplySender};
use piko_orchd_api::{AgentApiError, CancelReceipt};
use piko_protocol::execution::ExecutionInputReceipt;
use piko_protocol::execution::{CancelReason, SteerExecutionRequest};
use tokio_util::sync::CancellationToken;

use super::{ExecutionIdentity, ExecutionTerminal};

pub enum ExecutionCommand {
    Steer {
        request: SteerExecutionRequest,
        reply: ReplySender<ExecutionCommandReply, Result<ExecutionInputReceipt, AgentApiError>>,
    },
    Cancel {
        request_id: String,
        reason: CancelReason,
        reply: ReplySender<ExecutionCommandReply, Result<CancelReceipt, AgentApiError>>,
    },
    Shutdown {
        reply: ReplySender<ExecutionCommandReply, ()>,
    },
}

#[derive(Clone)]
pub struct ExecutionHandle {
    pub identity: ExecutionIdentity,
    pub generation: u64,
    pub command_tx: MailboxSender<ExecutionCommands, ExecutionCommand>,
    pub cancel: CancellationToken,
    /// Receives the durable terminal outcome once (finalizer).
    pub terminal_rx: ArcTerminalReceiver,
}

/// Cloneable waiter for the terminal ExecutionOutcome.
#[derive(Clone)]
pub struct ArcTerminalReceiver {
    inner: std::sync::Arc<
        tokio::sync::Mutex<Option<ReplyReceiver<ExecutionTerminalContract, ExecutionTerminal>>>,
    >,
}

impl ArcTerminalReceiver {
    pub fn new(rx: ReplyReceiver<ExecutionTerminalContract, ExecutionTerminal>) -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(Some(rx))),
        }
    }

    pub async fn wait(self) -> Result<ExecutionTerminal, AgentApiError> {
        let mut guard = self.inner.lock().await;
        let rx = guard.take().ok_or(AgentApiError::InvalidState)?;
        rx.await.map_err(|_| AgentApiError::RuntimeUnavailable)
    }
}
