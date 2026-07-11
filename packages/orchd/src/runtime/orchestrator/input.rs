use piko_protocol::agent_runtime::{InputDelivery, InputSource, SubmitTaskInput};
use piko_protocol::MessageContent;

use crate::domain::events::event::Event;
use crate::runtime::dispatch::DispatchSenders;
use crate::runtime::utils::now_ms;

use super::context::TaskContext;
use super::run_state::TaskRunState;

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
) -> Vec<Event> {
    let events = task_context
        .commit_user_input(input, senders)
        .await;
    if let Some(text) = input_text(&input.content) {
        run_state.push_user_message(text);
    }
    events
}
