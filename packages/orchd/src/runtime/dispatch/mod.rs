pub mod bus;
pub mod consumer;
pub mod lifecycle;
pub mod protocol;
pub mod step;

pub use bus::{ChannelConfig, Dispatch, DispatchSenders, SessionChannels};
pub use consumer::tool::ToolExecutionConsumer;
pub use lifecycle::LifecycleDispatch;
pub use piko_protocol::{DisplayEvent, LifecycleEvent, PersistEvent};
pub use protocol::{
    display_events_from_server_message, lifecycle_events_from_server_message,
    persist_events_from_server_message, server_message_from_display_event,
    server_message_from_persist_event,
};
pub use step::{StepDispatch, StepDispatchResult};

#[cfg(test)]
mod tests;
