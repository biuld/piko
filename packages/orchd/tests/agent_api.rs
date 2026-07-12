//! Agent API behavior tests (create/submit/control/idempotency).

#[path = "common/faux_provider.rs"]
mod faux_provider;

#[path = "agent_api/control_task.rs"]
mod control_task;
#[path = "agent_api/create_task.rs"]
mod create_task;
#[path = "agent_api/idle_barrier.rs"]
mod idle_barrier;
#[path = "agent_api/input_idempotency.rs"]
mod input_idempotency;
#[path = "agent_api/observation.rs"]
mod observation;
#[path = "agent_api/persistence.rs"]
mod persistence;
#[path = "agent_api/recovery.rs"]
mod recovery;
#[path = "agent_api/submit_input.rs"]
mod submit_input;
#[path = "agent_api/support.rs"]
mod support;
