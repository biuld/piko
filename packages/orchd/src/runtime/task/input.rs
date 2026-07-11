use std::sync::Arc;

use piko_protocol::MessageContent;
use piko_protocol::agent_runtime::{InputDelivery, InputSource, SubmitTaskInput};

use crate::domain::events::event::Event;
use crate::integration::{MessageCommit, PersistSink};
use crate::runtime::types::TaskInputEnvelope;
use crate::runtime::utils::now_ms;

use super::context::TaskContext;
use super::run_state::TaskRunState;

#[derive(Debug, thiserror::Error)]
pub(super) enum InputCommitError {
    #[error("persistence failed: {0}")]
    PersistenceFailed(String),
}

pub(crate) fn build_user_input(
    session_id: &str,
    task_id: &str,
    work_id: &str,
    content: impl Into<MessageContent>,
    source: InputSource,
    source_turn_id: Option<String>,
) -> SubmitTaskInput {
    SubmitTaskInput {
        request_id: format!("req_{}", uuid::Uuid::new_v4()),
        session_id: session_id.to_string(),
        task_id: task_id.to_string(),
        message_id: format!("msg_{}", uuid::Uuid::new_v4()),
        work_id: work_id.to_string(),
        source_turn_id,
        source,
        content: content.into(),
        delivery: InputDelivery::AfterCurrentStep,
        submitted_at: now_ms(),
    }
}

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
    pub events: Vec<Event>,
    pub committed: bool,
}

pub(super) async fn commit_mailbox_input(
    task_context: &TaskContext,
    run_state: &mut TaskRunState,
    envelope: &mut TaskInputEnvelope,
    persist_sink: Arc<dyn PersistSink>,
) -> InputCommitOutcome {
    match commit_input(task_context, run_state, &envelope.input, persist_sink).await {
        Ok(events) => {
            run_state.accept_input(envelope);
            envelope.complete_ack(Ok(()));
            InputCommitOutcome {
                events,
                committed: true,
            }
        }
        Err(error) => {
            envelope.complete_ack(Err(error.to_string()));
            tracing::error!(%error, "failed to commit task input");
            InputCommitOutcome {
                events: Vec::new(),
                committed: false,
            }
        }
    }
}

pub(super) async fn commit_input(
    task_context: &TaskContext,
    run_state: &mut TaskRunState,
    input: &SubmitTaskInput,
    persist_sink: Arc<dyn PersistSink>,
) -> Result<Vec<Event>, InputCommitError> {
    if run_state.is_message_committed(&input.message_id) {
        return Ok(Vec::new());
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
    Ok(emitter.take_local_events())
}
