// ---- Ports: abstractions for external capabilities ----
//
// Ports define interfaces (traits) that the domain needs but does not implement.
// Concrete implementations live in adapters/.

pub mod approval_gateway;
pub mod clock;
pub mod id_generator;
pub mod model_gateway;
pub mod persist_sink;
pub mod task_control;
pub mod tool_provider;
