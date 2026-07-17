use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use orchd_api::{AgentApiError, SessionExecutionPorts};
use tokio::sync::Mutex;

use super::ExecutionIdentity;
use super::ExecutionTerminal;
use super::mailbox::ExecutionHandle;
use piko_protocol::execution::ExecutionOutcome;

pub struct SessionExecutionScope {
    session_id: String,
    ports: SessionExecutionPorts,
    executions: Mutex<HashMap<String, ExecutionHandle>>,
    completed: Mutex<HashMap<String, ExecutionTerminal>>,
    generation: AtomicU64,
}

pub struct ExecutionExit {
    pub identity: ExecutionIdentity,
    pub terminal: ExecutionOutcome,
}

impl SessionExecutionScope {
    pub fn new(session_id: String, ports: SessionExecutionPorts) -> Self {
        Self {
            session_id,
            ports,
            executions: Mutex::new(HashMap::new()),
            completed: Mutex::new(HashMap::new()),
            generation: AtomicU64::new(0),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn ports(&self) -> &SessionExecutionPorts {
        &self.ports
    }

    pub fn next_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub async fn reserve_execution(&self, handle: ExecutionHandle) -> Result<(), AgentApiError> {
        let mut executions = self.executions.lock().await;
        if executions
            .values()
            .any(|active| active.identity.agent_instance_id == handle.identity.agent_instance_id)
        {
            return Err(AgentApiError::ExecutionAlreadyActive);
        }
        self.completed
            .lock()
            .await
            .remove(&handle.identity.execution_id);
        executions.insert(handle.identity.execution_id.clone(), handle);
        Ok(())
    }

    pub async fn get_execution(&self, execution_id: &str) -> Option<ExecutionHandle> {
        self.executions.lock().await.get(execution_id).cloned()
    }

    pub async fn publish_terminal(&self, execution_id: &str, outcome: ExecutionTerminal) {
        self.completed
            .lock()
            .await
            .insert(execution_id.to_string(), outcome);
    }

    pub async fn take_completed(&self, execution_id: &str) -> Option<ExecutionTerminal> {
        self.completed.lock().await.remove(execution_id)
    }

    pub async fn remove_if_generation(&self, execution_id: &str, generation: u64) {
        let mut executions = self.executions.lock().await;
        if let Some(handle) = executions.get(execution_id)
            && handle.generation == generation
        {
            executions.remove(execution_id);
        }
    }

    pub async fn rollback_reservation(&self, execution_id: &str, generation: u64) {
        self.remove_if_generation(execution_id, generation).await;
    }

    pub async fn cancel_all(&self) {
        let executions = self.executions.lock().await;
        for handle in executions.values() {
            handle.cancel.cancel();
        }
    }

    pub async fn drain(&self) -> bool {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if self.executions.lock().await.is_empty() {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use piko_protocol::execution::{CommitAck, CommitError, ExecutionSnapshot, ExecutionStatus};

    use super::*;
    use crate::runtime::execution::mailbox::{ArcTerminalReceiver, ExecutionHandle};

    struct NoopCommit;

    #[async_trait]
    impl orchd_api::ExecutionCommitPort for NoopCommit {
        async fn commit_message(
            &self,
            commit: piko_protocol::execution::MessageCommit,
        ) -> Result<CommitAck, CommitError> {
            Ok(CommitAck {
                session_id: commit.session_id,
                execution_id: commit.execution_id,
                agent_instance_id: commit.agent_instance_id,
                message_id: Some(commit.message_id),
                revision: 1,
            })
        }
    }

    fn handle(execution_id: &str, generation: u64) -> ExecutionHandle {
        let identity = ExecutionIdentity {
            session_id: "session".into(),
            source_turn_id: None,
            execution_id: execution_id.into(),
            agent_instance_id: "agent".into(),
            agent_id: "main".into(),
        };
        let (command_tx, _) = tokio::sync::mpsc::channel(1);
        let (_, snapshot_rx) = tokio::sync::watch::channel(ExecutionSnapshot {
            session_id: "session".into(),
            source_turn_id: None,
            execution_id: execution_id.into(),
            agent_instance_id: "agent".into(),
            agent_id: "main".into(),
            status: ExecutionStatus::Accepted,
            model_step_index: 0,
            usage: Default::default(),
            error: None,
        });
        let (_, terminal_rx) = tokio::sync::oneshot::channel();
        ExecutionHandle {
            identity,
            generation,
            command_tx,
            cancel: tokio_util::sync::CancellationToken::new(),
            snapshot_rx,
            terminal_rx: ArcTerminalReceiver::new(terminal_rx),
        }
    }

    #[tokio::test]
    async fn stale_cleanup_cannot_remove_a_new_generation() {
        let scope = SessionExecutionScope::new(
            "session".into(),
            orchd_api::SessionExecutionPorts::new(Arc::new(NoopCommit)),
        );
        scope.reserve_execution(handle("exec", 2)).await.unwrap();
        scope.rollback_reservation("exec", 1).await;
        assert!(scope.get_execution("exec").await.is_some());
        scope.rollback_reservation("exec", 2).await;
        assert!(scope.get_execution("exec").await.is_none());
    }
}
