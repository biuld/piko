mod agent;
mod bus;
mod lifecycle;
mod protocol;
mod tool;

pub use agent::StepDispatch;
pub(crate) use agent::StepDispatchResult;
pub use bus::{ChannelConfig, Dispatch, DispatchSenders, SessionChannels};
pub use lifecycle::{LifecycleDispatch, TaskLifecycleDispatcher};
pub use piko_protocol::{DisplayEvent, LifecycleEvent, PersistEvent};
pub use protocol::{
    display_events_from_server_message, lifecycle_events_from_server_message,
    persist_events_from_server_message, server_message_from_display_event,
    server_message_from_persist_event,
};
pub use tool::{ToolExecutionConsumer, ToolExecutionDispatcher};

#[cfg(test)]
mod tests;
