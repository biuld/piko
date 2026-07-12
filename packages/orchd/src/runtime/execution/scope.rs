use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use orchd_api::{AgentApiError, SessionExecutionConfig, SessionExecutionPorts};
use tokio::sync::Mutex;

use super::mailbox::ExecutionHandle;
use super::ExecutionIdentity;
use piko_protocol::execution::ExecutionOutcome;

pub struct SessionExecutionScope {
    session_id: String,
    ports: SessionExecutionPorts,
    executions: Mutex<HashMap<String, ExecutionHandle>>,
    completed: Mutex<HashMap<String, ExecutionOutcome>>,
    generation: AtomicU64,
}

pub struct ExecutionExit {
    pub identity: ExecutionIdentity,
    pub terminal: ExecutionOutcome,
}

impl SessionExecutionScope {
    pub fn new(config: SessionExecutionConfig) -> Self {
        Self {
            session_id: config.session_id,
            ports: config.ports,
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
        if !executions.is_empty() {
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

    pub async fn publish_terminal(&self, execution_id: &str, outcome: ExecutionOutcome) {
        self.completed
            .lock()
            .await
            .insert(execution_id.to_string(), outcome);
    }

    pub async fn take_completed(&self, execution_id: &str) -> Option<ExecutionOutcome> {
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

    pub async fn cancel_all(&self) {
        let executions = self.executions.lock().await;
        for handle in executions.values() {
            handle.cancel.cancel();
        }
    }
}

#[allow(dead_code)]
fn _arc_scope(_: Arc<SessionExecutionScope>) {}
