use std::sync::Arc;

use piko_orchd_api::{AgentApiError, AgentCommitPort};
use piko_protocol::{AgentDurableCommand, AgentInputReceipt};

use crate::runtime::execution::PreparedExecution;

/// A prepared Execution whose retained prelude and durable Agent work have not
/// been committed yet.
pub(crate) struct PreparedStartup {
    prepared: PreparedExecution,
}

pub(crate) type RunStartupScope = PreparedStartup;

/// A durably started Execution whose initial input is committed and which may
/// now be activated.
pub(crate) struct InputCommittedStartup {
    prepared: PreparedExecution,
}

impl PreparedStartup {
    pub fn new(prepared: PreparedExecution) -> Self {
        Self { prepared }
    }

    pub async fn commit_start(
        self,
        commit: &Arc<dyn AgentCommitPort>,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<InputCommittedStartup, AgentApiError> {
        if let Err(error) = self.prepared.commit_prelude().await {
            self.prepared.rollback().await;
            return Err(error);
        }
        if let Err(error) = commit.commit_agent_command(session_id, command).await {
            self.prepared.rollback().await;
            return Err(AgentApiError::PersistenceFailed(error.to_string()));
        }
        Ok(InputCommittedStartup {
            prepared: self.prepared,
        })
    }
}

impl InputCommittedStartup {
    pub fn receipt(&self) -> AgentInputReceipt {
        agent_receipt(&self.prepared)
    }

    pub async fn rollback(self) {
        self.prepared.rollback().await;
    }

    pub fn committed_input(&self) -> (piko_protocol::Message, String) {
        self.prepared.committed_input()
    }

    pub async fn activate(self) -> AgentInputReceipt {
        let receipt = agent_receipt(&self.prepared);
        self.prepared.activate().await;
        receipt
    }
}

fn agent_receipt(prepared: &PreparedExecution) -> AgentInputReceipt {
    let receipt = prepared.receipt();
    AgentInputReceipt {
        input_id: receipt.request_id.clone(),
        request_id: receipt.request_id.clone(),
        session_id: receipt.session_id,
        agent_instance_id: receipt.agent_instance_id,
        disposition: piko_protocol::AgentInputDisposition::AppliedAsRoot,
        queued_position: None,
    }
}
