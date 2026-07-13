use std::collections::HashMap;
use std::sync::Arc;
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

#[allow(dead_code)]
fn _arc_scope(_: Arc<SessionExecutionScope>) {}
