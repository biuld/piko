use std::sync::Arc;

use piko_protocol::MessageContent;
use piko_protocol::agent_runtime::{InputSource, SubmitTaskInput};

use crate::runtime::task::mailbox::TaskInputEnvelope;
use orchd_api::{MessageCommit, PersistSink};

use super::context::TaskContext;
use super::state::TaskRunState;

#[derive(Debug, thiserror::Error)]
pub(super) enum InputCommitError {
    #[error("persistence failed: {0}")]
    PersistenceFailed(String),
}

pub use orchd_api::build_user_input;

pub(super) fn input_text(content: &MessageContent) -> Option<String> {
    match content {
        MessageContent::String(text) => Some(text.clone()),
        MessageContent::Blocks(blocks) => {
            let text = blocks
                .iter()
                .filter_map(|block| match block {
                    piko_protocol::ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
            if text.is_empty() { None } else { Some(text) }
        }
    }
}

pub(super) fn source_task_agent(source: &InputSource) -> (String, String) {
    match source {
        InputSource::Task { task_id, agent_id } => (task_id.clone(), agent_id.clone()),
        _ => (String::new(), String::new()),
    }
}

pub(super) struct InputCommitOutcome {
    pub committed: bool,
}

pub(super) async fn commit_mailbox_input(
    task_context: &TaskContext,
    run_state: &mut TaskRunState,
    envelope: &mut TaskInputEnvelope,
    persist_sink: Arc<dyn PersistSink>,
) -> InputCommitOutcome {
    match commit_input(task_context, run_state, &envelope.input, persist_sink).await {
        Ok(()) => {
            run_state.accept_input(envelope);
            envelope.complete_ack(Ok(()));
            InputCommitOutcome { committed: true }
        }
        Err(error) => {
            envelope.complete_ack(Err(error.to_string()));
            tracing::error!(%error, "failed to commit task input");
            InputCommitOutcome { committed: false }
        }
    }
}

pub(super) async fn commit_input(
    task_context: &TaskContext,
    run_state: &mut TaskRunState,
    input: &SubmitTaskInput,
    persist_sink: Arc<dyn PersistSink>,
) -> Result<(), InputCommitError> {
    if run_state.is_message_committed(&input.message_id) {
        return Ok(());
    }

    let commit_lock = run_state.persist_commit_lock();
    let _commit_guard = commit_lock.lock().await;
    let task_seq = run_state.next_task_seq();
    let commit = MessageCommit {
        session_id: input.session_id.clone(),
        task_id: input.task_id.clone(),
        agent_id: task_context.agent_id().to_string(),
        work_id: input.work_id.clone(),
        task_seq,
        message_id: input.message_id.clone(),
        parent_message_id: run_state.head_message_id(),
        message: piko_protocol::Message::User {
            content: input.content.clone(),
            timestamp: Some(input.submitted_at),
        },
        committed_at: input.submitted_at,
    };
    persist_sink
        .commit_message(commit)
        .await
        .map_err(|error| InputCommitError::PersistenceFailed(error.to_string()))?;
    let emitter = run_state.event_emitter(task_context.dispatch_identity(), input.work_id.clone());
    run_state.push_user_content(input.content.clone(), Some(input.submitted_at));
    run_state.record_head(input.message_id.clone(), task_seq);
    emitter
        .emit_persist_observation(
            piko_protocol::PersistEvent::UserCommitted {
                session_id: input.session_id.clone(),
                message_id: input.message_id.clone(),
                task_id: input.task_id.clone(),
                agent_id: task_context.agent_id().to_string(),
                work_id: input.work_id.clone(),
                message: piko_protocol::Message::User {
                    content: input.content.clone(),
                    timestamp: Some(input.submitted_at),
                },
            },
            Some(task_seq),
        )
        .await;
    Ok(())
}
