use std::sync::Arc;

use tokio::sync::oneshot;

use crate::ports::{AgentRunFailure, TurnRunCompletion, TurnRunCompletionReceiver};

pub(super) struct TurnCompletionScope {
    sender: Option<oneshot::Sender<TurnRunCompletion>>,
    session_id: String,
    turn_id: String,
    root_agent_instance_id: String,
    observation: Arc<orchd::testing::SessionOutputHub>,
}

impl TurnCompletionScope {
    pub fn new(
        session_id: String,
        turn_id: String,
        root_agent_instance_id: String,
        observation: Arc<orchd::testing::SessionOutputHub>,
    ) -> (Self, TurnRunCompletionReceiver) {
        let (sender, receiver) = oneshot::channel();
        (
            Self {
                sender: Some(sender),
                session_id,
                turn_id,
                root_agent_instance_id,
                observation,
            },
            receiver,
        )
    }

    pub fn complete(mut self, result: Result<piko_protocol::AgentRunReport, AgentRunFailure>) {
        self.send(result);
    }

    fn send(&mut self, result: Result<piko_protocol::AgentRunReport, AgentRunFailure>) {
        let Some(sender) = self.sender.take() else {
            return;
        };
        let _ = sender.send(TurnRunCompletion {
            session_id: self.session_id.clone(),
            turn_id: self.turn_id.clone(),
            root_agent_instance_id: self.root_agent_instance_id.clone(),
            result,
            observation_barrier: self.observation.cursor(),
        });
    }
}

impl Drop for TurnCompletionScope {
    fn drop(&mut self) {
        self.send(Err(AgentRunFailure {
            message: "root Agent run task ended without a terminal result".into(),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dropped_scope_delivers_failure_once() {
        let hub = Arc::new(orchd::testing::SessionOutputHub::new(
            "session".into(),
            "epoch".into(),
            4,
        ));
        let (scope, receiver) =
            TurnCompletionScope::new("session".into(), "turn".into(), "root".into(), hub);
        drop(scope);

        let completion = receiver.await.unwrap();
        assert!(completion.result.is_err());
        assert_eq!(completion.turn_id, "turn");
    }
}
