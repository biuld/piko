use tokio_util::sync::CancellationToken;

use crate::runtime::task::TaskMailboxMessage;

/// Runtime handle for an active task: cancellation token and mailbox sender.
#[derive(Clone)]
pub(crate) struct ActiveTaskHandle {
    pub cancel: CancellationToken,
    pub control_tx: tokio::sync::mpsc::UnboundedSender<TaskMailboxMessage>,
    /// Monotonic id so a finishing runtime does not remove a replacement handle.
    pub generation: u64,
}
