use std::sync::Arc;

use crate::runtime::execution::ExecutionTerminal;
use crate::runtime::reliability::{CommitFailure, ExecutionHandoffLease, RetryState};
use orchd_api::AgentCommitPort;
use piko_protocol::{AgentDurableCommand, AgentRunReport, CommitError, ExecutionOutcome, Message};

/// Frozen terminal state. Publication data is private until `commit` returns a
/// `CommittedTerminal` capability.
pub(crate) struct PendingTerminal {
    execution_id: String,
    report: AgentRunReport,
    transcript: Vec<Message>,
    head_message_id: Option<String>,
    retry: RetryState,
    finished_at: i64,
    handoff: Option<ExecutionHandoffLease<ExecutionTerminal>>,
}

pub(crate) type TerminalCommitScope = PendingTerminal;

pub(crate) struct CommittedTerminal {
    pub execution_id: String,
    pub report: AgentRunReport,
    pub transcript: Vec<Message>,
    pub head_message_id: Option<String>,
    handoff: Option<ExecutionHandoffLease<ExecutionTerminal>>,
}

pub(crate) struct TerminalPersistenceFailure {
    pub execution_id: String,
    pub error: CommitError,
    handoff: Option<ExecutionHandoffLease<ExecutionTerminal>>,
}

pub(crate) enum TerminalCommitResult {
    Committed(CommittedTerminal),
    Retry { execution_id: String, delay_ms: u64 },
    PermanentFailure(TerminalPersistenceFailure),
}

impl PendingTerminal {
    pub fn new(
        execution_id: String,
        agent_instance_id: String,
        terminal: ExecutionHandoffLease<ExecutionTerminal>,
    ) -> Self {
        let candidate = terminal.payload();
        let report = AgentRunReport {
            agent_instance_id,
            report_id: report_id(&execution_id),
            summary: transcript_summary(&candidate.transcript),
            usage: match &candidate.outcome {
                ExecutionOutcome::Succeeded { usage } => usage.clone(),
                _ => Default::default(),
            },
            outcome: candidate.outcome.clone(),
            artifacts: Vec::new(),
        };
        Self {
            execution_id,
            report,
            transcript: candidate.transcript.clone(),
            head_message_id: candidate.head_message_id.clone(),
            retry: RetryState::default(),
            finished_at: chrono::Utc::now().timestamp_millis(),
            handoff: Some(terminal),
        }
    }

    pub fn execution_id(&self) -> &str {
        &self.execution_id
    }

    pub async fn commit(
        &mut self,
        port: &Arc<dyn AgentCommitPort>,
        session_id: &str,
    ) -> TerminalCommitResult {
        let result = port
            .commit_agent_command(
                session_id,
                AgentDurableCommand::RunTerminal {
                    run_id: self.execution_id.clone(),
                    report: self.report.clone(),
                    finished_at: self.finished_at,
                },
            )
            .await;
        match result {
            Ok(_) => TerminalCommitResult::Committed(CommittedTerminal {
                execution_id: self.execution_id.clone(),
                report: self.report.clone(),
                transcript: self.transcript.clone(),
                head_message_id: self.head_message_id.clone(),
                handoff: self.handoff.take(),
            }),
            Err(error) => match RetryState::classify(error) {
                CommitFailure::Permanent(error) => {
                    TerminalCommitResult::PermanentFailure(TerminalPersistenceFailure {
                        execution_id: self.execution_id.clone(),
                        error,
                        handoff: self.handoff.take(),
                    })
                }
                CommitFailure::Retryable => TerminalCommitResult::Retry {
                    execution_id: self.execution_id.clone(),
                    delay_ms: self.retry.next_delay_ms(),
                },
            },
        }
    }
}

fn report_id(internal_execution_id: &str) -> String {
    orchd_api::stable_internal_id("report", &[internal_execution_id])
}

impl CommittedTerminal {
    pub fn acknowledge_handoff(&mut self) {
        if let Some(mut handoff) = self.handoff.take() {
            handoff.acknowledge();
        }
    }
}

impl TerminalPersistenceFailure {
    pub fn acknowledge_handoff(&mut self) {
        if let Some(mut handoff) = self.handoff.take() {
            handoff.acknowledge();
        }
    }
}

fn transcript_summary(transcript: &[Message]) -> String {
    transcript
        .iter()
        .rev()
        .find_map(|message| match message {
            Message::Assistant { content, .. } => Some(
                content
                    .iter()
                    .filter_map(|block| match block {
                        piko_protocol::ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
            ),
            _ => None,
        })
        .unwrap_or_default()
}
