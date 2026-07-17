use std::sync::Arc;

use piko_orchd_api::AgentCommitPort;
use piko_protocol::{AgentDurableCommand, AgentInboxItem, AgentRunReport};

use crate::runtime::reliability::{CommitFailure, RetryState};

pub(crate) struct DetachedDeliveryScope {
    recipient_agent_instance_id: String,
    report: AgentRunReport,
    committed_at: i64,
    retry: RetryState,
}

pub(crate) enum DetachedDeliveryResult {
    Committed(AgentInboxItem),
    Retry { delay_ms: u64 },
    PermanentFailure,
}

impl DetachedDeliveryScope {
    pub fn new(recipient_agent_instance_id: String, report: AgentRunReport) -> Self {
        Self {
            recipient_agent_instance_id,
            report,
            committed_at: chrono::Utc::now().timestamp_millis(),
            retry: RetryState::default(),
        }
    }

    pub fn recipient_agent_instance_id(&self) -> &str {
        &self.recipient_agent_instance_id
    }

    pub async fn commit(
        &mut self,
        port: &Arc<dyn AgentCommitPort>,
        session_id: &str,
    ) -> DetachedDeliveryResult {
        let result = port
            .commit_agent_command(
                session_id,
                AgentDurableCommand::CommitReport {
                    recipient_agent_instance_id: self.recipient_agent_instance_id.clone(),
                    report: self.report.clone(),
                },
            )
            .await;
        match result {
            Ok(_) => DetachedDeliveryResult::Committed(AgentInboxItem {
                report_id: self.report.report_id.clone(),
                recipient_agent_instance_id: self.recipient_agent_instance_id.clone(),
                source_agent_instance_id: self.report.agent_instance_id.clone(),
                report: self.report.clone(),
                committed_at: self.committed_at,
                consumed_at: None,
            }),
            Err(error) => match RetryState::classify(error) {
                CommitFailure::Retryable => DetachedDeliveryResult::Retry {
                    delay_ms: self.retry.next_delay_ms(),
                },
                CommitFailure::Permanent(_) => DetachedDeliveryResult::PermanentFailure,
            },
        }
    }
}
