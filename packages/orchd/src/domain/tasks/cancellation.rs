// ---- Domain: task cancellation ----
//
// Cancellation models. Currently cancellation is implemented via
// tokio_util::sync::CancellationToken at the runtime layer.

use tokio_util::sync::CancellationToken;

/// A cancellation handle for a running task.
///
/// This is a domain-level wrapper around the runtime cancellation primitive.
#[derive(Clone)]
pub struct TaskCancellation {
    pub(crate) token: CancellationToken,
}

impl TaskCancellation {
    /// Create a new cancellation handle.
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }

    /// Signal cancellation.
    pub fn cancel(&self) {
        self.token.cancel();
    }

    /// Check if cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// Get a child token for sub-tasks.
    pub fn child(&self) -> Self {
        Self {
            token: self.token.child_token(),
        }
    }
}

impl Default for TaskCancellation {
    fn default() -> Self {
        Self::new()
    }
}
