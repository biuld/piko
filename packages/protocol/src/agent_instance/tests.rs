use super::*;
use crate::{ExecutionOutcome, MessageContent, Usage};

#[test]
fn identity_keeps_spec_and_runtime_address_separate() {
    let identity = AgentInstanceIdentity {
        session_id: "session-1".into(),
        agent_instance_id: "agent-instance-1".into(),
        agent_spec_id: "coder".into(),
        parent_agent_instance_id: Some("root".into()),
    };
    let value = serde_json::to_value(identity).expect("serialize identity");
    assert_eq!(value["agentInstanceId"], "agent-instance-1");
    assert_eq!(value["agentSpecId"], "coder");
    assert_eq!(value["parentAgentInstanceId"], "root");
}

#[test]
fn activity_is_separate_from_lifecycle() {
    let snapshot = AgentSnapshot {
        identity: AgentInstanceIdentity {
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            agent_spec_id: "main".into(),
            parent_agent_instance_id: None,
        },
        lifecycle: AgentInstanceLifecycle::Open,
        activity: AgentActivity::Running,
        latest_report: None,
        active_root_input_id: None,
        pending_follow_up_ids: Vec::new(),
        unread_report_count: 0,
        generation: 1,
    };
    let value = serde_json::to_value(snapshot).expect("serialize snapshot");
    assert_eq!(value["lifecycle"], "open");
    assert_eq!(value["activity"]["type"], "running");
}

#[test]
fn agent_facing_dtos_never_serialize_execution_identity() {
    let identity = AgentInstanceIdentity {
        session_id: "session-1".into(),
        agent_instance_id: "root".into(),
        agent_spec_id: "main".into(),
        parent_agent_instance_id: None,
    };
    let report = AgentWorkReport {
        agent_instance_id: "root".into(),
        root_input_id: "input-1".into(),
        report_id: "report-1".into(),
        outcome: ExecutionOutcome::Succeeded {
            usage: Usage::default(),
        },
        summary: "done".into(),
        usage: Usage::default(),
        artifacts: Vec::new(),
    };
    let values = [
        serde_json::to_value(AgentSnapshot {
            identity,
            lifecycle: AgentInstanceLifecycle::Open,
            activity: AgentActivity::Running,
            latest_report: Some(report.clone()),
            active_root_input_id: None,
            pending_follow_up_ids: Vec::new(),
            unread_report_count: 0,
            generation: 1,
        })
        .expect("serialize AgentSnapshot"),
        serde_json::to_value(report).expect("serialize AgentWorkReport"),
        serde_json::to_value(CreateAgentRequest {
            request_id: "create-1".into(),
            session_id: "session-1".into(),
            parent_agent_instance_id: "root".into(),
            agent_spec_id: "main".into(),
            requested_agent_instance_id: None,
            origin_tool_call_id: Some("tool-1".into()),
        })
        .expect("serialize CreateAgentRequest"),
        serde_json::to_value(SendAgentInputRequest {
            request_id: "input-1".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: Some("turn-1".into()),
            message_id: "message-1".into(),
            content: MessageContent::String("hello".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
        })
        .expect("serialize SendAgentInputRequest"),
        serde_json::to_value(AgentInputReceipt {
            input_id: "input-1".into(),
            request_id: "input-1".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            disposition: crate::AgentInputDisposition::AppliedAsRoot,
            queued_position: None,
        })
        .expect("serialize AgentInputReceipt"),
        serde_json::to_value(AgentCancelReceipt {
            request_id: "cancel-1".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            accepted: true,
        })
        .expect("serialize AgentCancelReceipt"),
    ];

    for value in values {
        assert_no_execution_identity(&value);
    }
}

fn assert_no_execution_identity(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            for (field, value) in fields {
                assert!(
                    !matches!(
                        field.as_str(),
                        "executionId" | "requestedExecutionId" | "originExecutionId"
                    ),
                    "Agent-facing DTO leaked `{field}`: {value}"
                );
                assert_no_execution_identity(value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                assert_no_execution_identity(value);
            }
        }
        _ => {}
    }
}
