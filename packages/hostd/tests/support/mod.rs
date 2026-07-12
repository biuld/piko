pub mod mock_session;
pub mod mock_turn_runner;

pub use mock_session::MockSessionPublisher;
pub use mock_turn_runner::MockTurnRunner;

use piko_protocol::agent_runtime::SessionEvent;
use piko_protocol::{ExecutionObservationSnapshot, ExecutionStatus};

pub fn execution_running(
    session_id: impl Into<String>,
    turn_id: impl Into<String>,
    execution_id: impl Into<String>,
    agent_id: impl Into<String>,
) -> SessionEvent {
    SessionEvent::ExecutionChanged {
        snapshot: ExecutionObservationSnapshot {
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            execution_id: execution_id.into(),
            agent_instance_id: "root".into(),
            agent_id: agent_id.into(),
            status: ExecutionStatus::Running,
        },
    }
}

pub fn execution_succeeded(
    session_id: impl Into<String>,
    turn_id: impl Into<String>,
    execution_id: impl Into<String>,
    agent_id: impl Into<String>,
) -> SessionEvent {
    SessionEvent::ExecutionChanged {
        snapshot: ExecutionObservationSnapshot {
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            execution_id: execution_id.into(),
            agent_instance_id: "root".into(),
            agent_id: agent_id.into(),
            status: ExecutionStatus::Succeeded,
        },
    }
}
