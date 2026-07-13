use orchd_api::{AgentApiError, CancelReceipt};
use piko_protocol::execution::ExecutionInputReceipt;
use piko_protocol::execution::{CancelReason, ExecutionSnapshot, SteerExecutionRequest};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use super::{ExecutionIdentity, ExecutionTerminal};

pub enum ExecutionCommand {
    Steer {
        request: SteerExecutionRequest,
        reply: oneshot::Sender<Result<ExecutionInputReceipt, AgentApiError>>,
    },
    Cancel {
        request_id: String,
        reason: CancelReason,
        reply: oneshot::Sender<Result<CancelReceipt, AgentApiError>>,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}

#[derive(Clone)]
pub struct ExecutionHandle {
    pub identity: ExecutionIdentity,
    pub generation: u64,
    pub command_tx: mpsc::Sender<ExecutionCommand>,
    pub cancel: CancellationToken,
    pub snapshot_rx: watch::Receiver<ExecutionSnapshot>,
    /// Receives the durable terminal outcome once (finalizer).
    pub terminal_rx: ArcTerminalReceiver,
}

/// Cloneable waiter for the terminal ExecutionOutcome.
#[derive(Clone)]
pub struct ArcTerminalReceiver {
    inner: std::sync::Arc<tokio::sync::Mutex<Option<oneshot::Receiver<ExecutionTerminal>>>>,
}

impl ArcTerminalReceiver {
    pub fn new(rx: oneshot::Receiver<ExecutionTerminal>) -> Self {
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
