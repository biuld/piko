// ---- Runtime: agent execution implementation ----

pub mod dispatch;
pub mod stream;
pub mod tool_calls;
pub mod tool_executor;

pub use stream::runtime_assistant_message_id;
