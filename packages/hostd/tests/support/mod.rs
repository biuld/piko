pub mod mock_session;

pub use mock_session::MockSessionPublisher;

pub fn successful_turn_run(
    subscription: orchd_api::SessionSubscription,
    session_id: impl Into<String>,
    turn_id: impl Into<String>,
    root_agent_instance_id: impl Into<String>,
    barrier_seq: u64,
    delay: std::time::Duration,
) -> hostd::ports::AgentRunHandle {
    let session_id = session_id.into();
    let turn_id = turn_id.into();
    let root_agent_instance_id = root_agent_instance_id.into();
    let barrier = piko_protocol::agent_runtime::SessionCursor {
        epoch: subscription.cursor.epoch.clone(),
        seq: barrier_seq,
    };
    let (completion_tx, completion) = tokio::sync::oneshot::channel();
    let (started_tx, started) = tokio::sync::oneshot::channel();
    let _ = started_tx.send(subscription);
    let handle_root_agent_instance_id = root_agent_instance_id.clone();
    let address = hostd::ports::AgentOperationAddress {
        session_id: session_id.clone(),
        operation_id: turn_id.clone(),
        agent_instance_id: handle_root_agent_instance_id,
    };
    let completion_address = address.clone();
    let report_agent_instance_id = root_agent_instance_id.clone();
    tokio::spawn(async move {
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let _ = completion_tx.send(hostd::ports::AgentRunCompletion {
            address: completion_address,
            result: Ok(success_report(report_agent_instance_id)),
            observation_barrier: barrier,
        });
    });
    hostd::ports::AgentRunHandle {
        address,
        receipt: piko_protocol::AgentInputReceipt {
            request_id: turn_id,
            session_id,
            agent_instance_id: root_agent_instance_id.clone(),
            disposition: piko_protocol::InputDisposition::Accepted,
        },
        started,
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
pub fn execution_running() -> SessionEvent {
    SessionEvent::InteractionResolved {
        resolution: serde_json::json!({"marker": "running"}),
    }
}

pub fn execution_succeeded() -> SessionEvent {
    SessionEvent::InteractionResolved {
        resolution: serde_json::json!({"marker": "completed"}),
    }
}
