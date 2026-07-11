use std::sync::Arc;

use piko_protocol::MessageContent;
use piko_protocol::agent_runtime::{InputDelivery, InputSource, SubmitTaskInput};

use crate::domain::events::event::Event;
use crate::integration::{MessageCommit, PersistSink};
use crate::runtime::dispatch::DispatchSenders;
use crate::runtime::events::SharedSessionOutputHub;
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
) -> SubmitTaskInput {
    SubmitTaskInput {
        request_id: format!("req_{}", uuid::Uuid::new_v4()),
        session_id: session_id.to_string(),
        task_id: task_id.to_string(),
        message_id: format!("msg_{}", uuid::Uuid::new_v4()),
        work_id: work_id.to_string(),
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

pub(super) async fn commit_input(
    task_context: &TaskContext,
    run_state: &mut TaskRunState,
    input: &SubmitTaskInput,
    senders: Option<DispatchSenders>,
    output_hub: Option<SharedSessionOutputHub>,
    persist_sink: Option<Arc<dyn PersistSink>>,
) -> Result<Vec<Event>, InputCommitError> {
    if run_state.is_message_committed(&input.message_id) {
        return Ok(Vec::new());
    }

    if let Some(sink) = persist_sink {
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
        sink.commit_message(commit)
            .await
            .map_err(|error| InputCommitError::PersistenceFailed(error.to_string()))?;
        if let Some(text) = input_text(&input.content) {
            run_state.push_user_message(text);
        }
        run_state.record_head(input.message_id.clone(), task_seq);
        return Ok(Vec::new());
    }

    let events = task_context
        .commit_user_input(input, senders, output_hub, run_state.last_task_seq())
        .await;
    if let Some(text) = input_text(&input.content) {
        run_state.push_user_message(text);
    }
    let task_seq = run_state.next_task_seq();
    run_state.record_head(input.message_id.clone(), task_seq);
    Ok(events)
}
