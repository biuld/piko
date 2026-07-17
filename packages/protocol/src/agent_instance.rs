//! Long-lived multi-agent identities and host/orchestrator DTOs.
//!
//! An `AgentInstance` is a stable Session member. It may own many short-lived
//! Executions, but Execution state is never folded into its lifecycle.

use serde::{Deserialize, Serialize};

use crate::{ExecutionId, ExecutionOutcome, MessageContent, Usage};

pub type AgentInstanceId = String;
pub type AgentSpecId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInstanceIdentity {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub agent_spec_id: AgentSpecId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_agent_instance_id: Option<AgentInstanceId>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentInstanceLifecycle {
    Open,
    Closed,
    Terminated,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentActivity {
    Idle,
    Running,
    WaitingForApproval,
    Cancelling,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunReport {
    pub agent_instance_id: AgentInstanceId,
    pub report_id: String,
    pub outcome: ExecutionOutcome,
    pub summary: String,
    pub usage: Usage,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<AgentArtifactRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentArtifactRef {
    pub id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSnapshot {
    pub identity: AgentInstanceIdentity,
    pub lifecycle: AgentInstanceLifecycle,
    pub activity: AgentActivity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_report: Option<AgentRunReport>,
    pub unread_report_count: u32,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentInputDelivery {
    Auto,
    StartWhenIdle,
    SteerActive,
    FollowUp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgentRequest {
    pub request_id: String,
    pub session_id: String,
    pub parent_agent_instance_id: AgentInstanceId,
    pub agent_spec_id: AgentSpecId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_agent_instance_id: Option<AgentInstanceId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgentReceipt {
    pub request_id: String,
    pub identity: AgentInstanceIdentity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SendAgentInputRequest {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_agent_instance_id: Option<AgentInstanceId>,
    /// Interaction Turn this input is bound to. `Some` on the root Turn path,
    /// `None` for child agent runs spawned by multi-agent tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<String>,
    pub message_id: String,
    pub content: MessageContent,
    pub delivery: AgentInputDelivery,
    /// Trusted host-owned prompt resources for this run. Child/tool callers
    /// omit this and receive the AgentSpec base prompt plus resolved tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_resources: Option<crate::PromptResourceSnapshot>,
    /// Optional transient restriction intersected with the AgentSpec allow-list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_tool_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SteerAgentRequest {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_agent_instance_id: Option<AgentInstanceId>,
    pub message_id: String,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInputReceipt {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub disposition: crate::InputDisposition,
}

/// Durable follow-up input owned by one AgentInstance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DurableAgentInput {
    pub queued_input_id: String,
    pub request: SendAgentInputRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detached_recipient_agent_instance_id: Option<AgentInstanceId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLifecycleRequest {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_agent_instance_id: Option<AgentInstanceId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLifecycleReceipt {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub lifecycle: AgentInstanceLifecycle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInboxItem {
    pub report_id: String,
    pub recipient_agent_instance_id: AgentInstanceId,
    pub source_agent_instance_id: AgentInstanceId,
    pub report: AgentRunReport,
    pub committed_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInboxSnapshot {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub items: Vec<AgentInboxItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConsumeAgentInboxRequest {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub report_id: String,
    pub consumed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConsumeAgentInboxReceipt {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub report_id: String,
    pub consumed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommitAck {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCancelReceipt {
    pub request_id: String,
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentDurableCommand {
    Create {
        identity: AgentInstanceIdentity,
        spec: crate::AgentSpec,
    },
    SetLifecycle {
        agent_instance_id: AgentInstanceId,
        lifecycle: AgentInstanceLifecycle,
    },
    RunStarted {
        agent_instance_id: AgentInstanceId,
        run_id: String,
        internal_execution_id: ExecutionId,
        request_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_turn_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        detached_recipient_agent_instance_id: Option<AgentInstanceId>,
        #[serde(default)]
        prompt_assembly_version: u32,
        #[serde(default)]
        prompt_digest: String,
        started_at: i64,
    },
    RunTerminal {
        run_id: String,
        report: AgentRunReport,
        finished_at: i64,
    },
    InputQueued {
        agent_instance_id: AgentInstanceId,
        queued_input: DurableAgentInput,
    },
    QueuedInputCancelled {
        agent_instance_id: AgentInstanceId,
        queued_input_id: String,
        cancelled_at: i64,
    },
    QueuedInputStarted {
        agent_instance_id: AgentInstanceId,
        queued_input_id: String,
        run_id: String,
        internal_execution_id: ExecutionId,
        request_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_turn_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        detached_recipient_agent_instance_id: Option<AgentInstanceId>,
        #[serde(default)]
        prompt_assembly_version: u32,
        #[serde(default)]
        prompt_digest: String,
        started_at: i64,
    },
    CommitReport {
        recipient_agent_instance_id: AgentInstanceId,
        report: AgentRunReport,
    },
    ConsumeInboxItem {
        agent_instance_id: AgentInstanceId,
        report_id: String,
        consumed_at: i64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let report = AgentRunReport {
            agent_instance_id: "root".into(),
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
                unread_report_count: 0,
                generation: 1,
            })
            .expect("serialize AgentSnapshot"),
            serde_json::to_value(report).expect("serialize AgentRunReport"),
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
                request_id: "input-1".into(),
                session_id: "session-1".into(),
                agent_instance_id: "root".into(),
                disposition: crate::InputDisposition::Accepted,
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
}
