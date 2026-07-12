use std::sync::Arc;

use orchd_api::ExecutionCommitPort;
use piko_protocol::execution::{ExecutionOutcome, ExecutionOutcomeCommit};

use super::scope::SessionExecutionScope;
use super::ExecutionIdentity;

pub async fn finalize_execution(
    scope: &SessionExecutionScope,
    identity: &ExecutionIdentity,
    outcome: ExecutionOutcome,
) -> ExecutionOutcome {
    let commit = ExecutionOutcomeCommit {
        session_id: identity.session_id.clone(),
        turn_id: identity.turn_id.clone(),
        execution_id: identity.execution_id.clone(),
        outcome: outcome.clone(),
        committed_at: chrono::Utc::now().timestamp_millis(),
    };
    match scope.ports().commit.commit_execution_outcome(commit).await {
        Ok(_) => outcome,
        Err(err) => ExecutionOutcome::failed(format!("finalization persist failed: {err}")),
    }
}

#[allow(dead_code)]
fn _ports_type(_: Arc<dyn ExecutionCommitPort>) {}
