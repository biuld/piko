use piko_protocol::MessageContent;
use piko_protocol::agent_runtime::{InputDelivery, InputSource, SubmitTaskInput};

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn build_user_input(
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
