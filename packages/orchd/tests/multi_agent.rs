//! Multi-agent integration tests (spawn, detached, steer, poll).

#[path = "common/faux_provider.rs"]
mod faux_provider;
#[path = "common/runtime.rs"]
mod runtime;
#[path = "common/session_output.rs"]
mod session_output;

#[path = "multi_agent/detached.rs"]
mod detached;
#[path = "multi_agent/poll.rs"]
mod poll;
#[path = "multi_agent/shared_agent_spec.rs"]
mod shared_agent_spec;
#[path = "multi_agent/spawn.rs"]
mod spawn;
#[path = "multi_agent/steer.rs"]
mod steer;
