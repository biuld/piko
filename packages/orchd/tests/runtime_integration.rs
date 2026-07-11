//! Runtime lifecycle, observation, and tool integration tests.

#[path = "common/faux_provider.rs"]
mod faux_provider;
#[path = "common/runtime.rs"]
mod runtime;
#[path = "common/session_output.rs"]
mod session_output;

#[path = "runtime_integration/cancel.rs"]
mod cancel;
#[path = "runtime_integration/errors.rs"]
mod errors;
#[path = "runtime_integration/observation.rs"]
mod observation;
#[path = "runtime_integration/snapshot.rs"]
mod snapshot;
#[path = "runtime_integration/tools.rs"]
mod tools;
