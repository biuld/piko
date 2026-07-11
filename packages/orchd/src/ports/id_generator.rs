/// Generate a unique work identifier for a task input cycle.
pub fn generate_work_id() -> String {
    format!("work_{}", uuid::Uuid::new_v4())
}

/// Generate a unique request identifier for API idempotency.
pub fn generate_request_id() -> String {
    format!("req_{}", uuid::Uuid::new_v4())
}

/// Generate a unique message identifier for transcript commits.
pub fn generate_message_id() -> String {
    format!("msg_{}", uuid::Uuid::new_v4())
}
