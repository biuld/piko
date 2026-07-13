use std::sync::Arc;

use orchd_api::{AgentApiError, AgentCommitPort};
use piko_protocol::{AgentDurableCommand, AgentInputReceipt, InputDisposition};

use crate::runtime::execution::PreparedExecution;

/// A prepared Execution whose durable Agent run has not been committed yet.
pub(crate) struct PreparedStartup {
    prepared: PreparedExecution,
}

pub(crate) type RunStartupScope = PreparedStartup;

/// A prepared Execution whose Agent run is durable but whose input is not yet
/// committed to the private transcript.
pub(crate) struct DurableStartup {
    prepared: PreparedExecution,
}

/// A durably started Execution whose initial input is committed and which may
/// now be activated.
pub(crate) struct InputCommittedStartup {
    prepared: PreparedExecution,
}

pub(crate) struct StartedRunFailure {
    pub error: AgentApiError,
    pub receipt: AgentInputReceipt,
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
    ) -> Result<DurableStartup, AgentApiError> {
        if let Err(error) = commit.commit_agent_command(session_id, command).await {
            self.prepared.rollback().await;
            return Err(AgentApiError::PersistenceFailed(error.to_string()));
        }
        Ok(DurableStartup {
            prepared: self.prepared,
        })
    }
}

impl DurableStartup {
    pub async fn commit_input(self) -> Result<InputCommittedStartup, StartedRunFailure> {
        if let Err(error) = self.prepared.commit_input().await {
            let receipt = agent_receipt(&self.prepared);
            self.prepared.rollback().await;
            return Err(StartedRunFailure { error, receipt });
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
        request_id: receipt.request_id,
        session_id: receipt.session_id,
        agent_instance_id: receipt.agent_instance_id,
        disposition: InputDisposition::Accepted,
    }
}
