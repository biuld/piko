use orchd_api::AgentApiError;
use tokio::sync::oneshot;

/// The receiving Actor owns the only payload and the obligation to explicitly
/// acknowledge that it has accepted the handoff.
pub(crate) struct ExecutionHandoffLease<T> {
    payload: T,
    accepted: Option<oneshot::Sender<()>>,
}

/// The sending supervisor retains only the completion obligation, never a
/// second mutable copy of the payload.
pub(crate) struct ExecutionHandoffWaiter {
    accepted: oneshot::Receiver<()>,
}

impl<T> ExecutionHandoffLease<T> {
    pub fn new(payload: T) -> (Self, ExecutionHandoffWaiter) {
        let (accepted, waiter) = oneshot::channel();
        (
            Self {
                payload,
                accepted: Some(accepted),
            },
            ExecutionHandoffWaiter { accepted: waiter },
        )
    }

    pub fn payload(&self) -> &T {
        &self.payload
    }

    pub fn acknowledge(&mut self) {
        if let Some(accepted) = self.accepted.take() {
            let _ = accepted.send(());
        }
    }
}

impl ExecutionHandoffWaiter {
    pub async fn wait(self) -> Result<(), AgentApiError> {
        self.accepted
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn waiter_completes_only_after_explicit_acknowledgement() {
        let (mut lease, waiter) = ExecutionHandoffLease::new("terminal");
        assert_eq!(lease.payload(), &"terminal");
        lease.acknowledge();
        assert!(waiter.wait().await.is_ok());
    }

    #[tokio::test]
    async fn dropping_unacknowledged_lease_fails_the_waiter() {
        let (lease, waiter) = ExecutionHandoffLease::new("terminal");
        drop(lease);
        assert!(waiter.wait().await.is_err());
    }
}
