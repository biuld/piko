use std::sync::Arc;

use tokio::sync::Mutex;

use crate::api::{ApprovalDecision, ApprovalId, ProtocolError, TaskId};
use crate::domain::turns::runner::TurnRunner;

#[derive(Clone)]
pub struct TurnSupervisor {
    runner: Arc<Mutex<Arc<dyn TurnRunner>>>,
}

impl TurnSupervisor {
    pub fn new(runner: Arc<dyn TurnRunner>) -> Self {
        Self {
            runner: Arc::new(Mutex::new(runner)),
        }
    }

    pub async fn runner(&self) -> Arc<dyn TurnRunner> {
        self.runner.lock().await.clone()
    }

    pub async fn set_runner(&self, runner: Arc<dyn TurnRunner>) {
        *self.runner.lock().await = runner;
    }

    pub async fn respond_approval(
        &self,
        approval_id: &ApprovalId,
        decision: ApprovalDecision,
    ) -> Result<bool, ProtocolError> {
        self.runner()
            .await
            .respond_approval(approval_id, decision)
            .await
    }

    pub async fn steer_task(&self, task_id: &TaskId, message: &str) -> bool {
        self.runner()
            .await
            .steer_task(task_id, "queue", "hostd", message)
            .await
    }
}
