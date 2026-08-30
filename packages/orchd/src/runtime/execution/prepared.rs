use super::*;

use super::terminal::supervise_execution;

pub(crate) struct PreparedExecution {
    pub(super) scope: Arc<SessionExecutionScope>,
    pub(super) actor: Option<ExecutionActor>,
    pub(super) generation: u64,
    pub(super) terminal_tx:
        Option<piko_comms::ReplySender<ExecutionTerminalContract, ExecutionTerminal>>,
    pub(super) receipt: ExecutionReceipt,
    pub(super) world_state_commit: Option<piko_protocol::execution::MessageCommit>,
    pub(super) completion_commits: Vec<piko_protocol::execution::MessageCommit>,
    pub(super) mention_commits: Vec<piko_protocol::execution::MessageCommit>,
    pub(super) input_commit: piko_protocol::execution::MessageCommit,
    pub(super) trace_span: tracing::Span,
}

impl PreparedExecution {
    pub fn identity(&self) -> &ExecutionIdentity {
        self.actor
            .as_ref()
            .expect("prepared Execution must own its Actor")
            .identity()
    }

    pub async fn activate(mut self) -> ExecutionReceipt {
        let actor = self
            .actor
            .take()
            .expect("prepared Execution owns its Actor until activation");
        let terminal_tx = self
            .terminal_tx
            .take()
            .expect("prepared Execution owns its terminal channel until activation");
        let scope = Arc::clone(&self.scope);
        let generation = self.generation;
        let trace_span = self.trace_span.clone();
        tokio::spawn(async move {
            let _exit = supervise_execution(scope, actor, generation, terminal_tx)
                .instrument(trace_span)
                .await;
        });
        self.receipt.clone()
    }

    pub fn receipt(&self) -> ExecutionReceipt {
        self.receipt.clone()
    }

    pub fn committed_input(&self) -> (piko_protocol::Message, String) {
        (
            self.input_commit.message.clone(),
            self.input_commit.message_id.clone(),
        )
    }

    pub async fn commit_input(&self) -> Result<(), AgentApiError> {
        if let Some(commit) = &self.world_state_commit {
            self.scope
                .ports()
                .commit
                .commit_message(commit.clone())
                .await
                .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        }
        for commit in &self.completion_commits {
            self.scope
                .ports()
                .commit
                .commit_message(commit.clone())
                .await
                .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        }
        for commit in &self.mention_commits {
            self.scope
                .ports()
                .commit
                .commit_message(commit.clone())
                .await
                .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        }
        self.scope
            .ports()
            .commit
            .commit_message(self.input_commit.clone())
            .await
            .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        Ok(())
    }

    pub async fn rollback(mut self) {
        let root_input_id = self.identity().root_input_id.clone();
        self.actor.take();
        self.terminal_tx.take();
        self.scope
            .rollback_reservation(&root_input_id, self.generation)
            .await;
    }
}

impl Drop for PreparedExecution {
    fn drop(&mut self) {
        let Some(actor) = self.actor.as_ref() else {
            return;
        };
        let root_input_id = actor.identity().root_input_id.clone();
        let generation = self.generation;
        let scope = Arc::clone(&self.scope);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                scope.rollback_reservation(&root_input_id, generation).await;
            });
        }
    }
}
