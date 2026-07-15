pub mod mock_session;

pub use mock_session::MockSessionPublisher;

pub fn successful_turn_run(
    subscription: orchd_api::SessionSubscription,
    session_id: impl Into<String>,
    turn_id: impl Into<String>,
    root_agent_instance_id: impl Into<String>,
    barrier_seq: u64,
    delay: std::time::Duration,
) -> hostd::ports::TurnRunHandle {
    let session_id = session_id.into();
    let turn_id = turn_id.into();
    let root_agent_instance_id = root_agent_instance_id.into();
    let barrier = piko_protocol::agent_runtime::SessionCursor {
        epoch: subscription.cursor.epoch.clone(),
        seq: barrier_seq,
    };
    let (completion_tx, completion) = tokio::sync::oneshot::channel();
    let completion_session_id = session_id.clone();
    let completion_turn_id = turn_id.clone();
    let handle_root_agent_instance_id = root_agent_instance_id.clone();
    tokio::spawn(async move {
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let _ = completion_tx.send(hostd::ports::TurnRunCompletion {
            session_id: completion_session_id,
            turn_id: completion_turn_id,
            root_agent_instance_id: root_agent_instance_id.clone(),
            result: Ok(success_report(root_agent_instance_id)),
            observation_barrier: barrier,
        });
    });
    hostd::ports::TurnRunHandle {
        session_id,
        turn_id,
        root_agent_instance_id: handle_root_agent_instance_id,
        observation: subscription,
        completion,
    }
}

pub fn success_report(agent_instance_id: impl Into<String>) -> piko_protocol::AgentRunReport {
    piko_protocol::AgentRunReport {
        agent_instance_id: agent_instance_id.into(),
        report_id: format!("report_{}", uuid::Uuid::new_v4()),
        outcome: piko_protocol::ExecutionOutcome::Succeeded {
            usage: Default::default(),
        },
        summary: "done".into(),
        usage: Default::default(),
        artifacts: Vec::new(),
    }
}

use piko_protocol::agent_runtime::SessionEvent;
pub fn execution_running(
    _session_id: impl Into<String>,
    _turn_id: impl Into<String>,
    _execution_id: impl Into<String>,
    _agent_id: impl Into<String>,
) -> SessionEvent {
    SessionEvent::InteractionResolved {
        resolution: serde_json::json!({"marker": "running"}),
    }
}

pub fn execution_succeeded(
    _session_id: impl Into<String>,
    _turn_id: impl Into<String>,
    _execution_id: impl Into<String>,
    _agent_id: impl Into<String>,
) -> SessionEvent {
    SessionEvent::InteractionResolved {
        resolution: serde_json::json!({"marker": "completed"}),
    }
}
