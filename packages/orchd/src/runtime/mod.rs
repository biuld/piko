// ---- Runtime: actor system and agent actor implementation ----
//
// The runtime layer implements the execution mechanisms required by
// the application layer. It uses tokio_actors for actor lifecycle
// and contains the agent loop, step runner, and tool executor.

pub mod agent_actor;
