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
    Running { execution_id: ExecutionId },
    WaitingForApproval { execution_id: ExecutionId },
    Cancelling { execution_id: ExecutionId },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecutionReport {
    pub agent_instance_id: AgentInstanceId,
    pub execution_id: ExecutionId,
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
    pub latest_report: Option<AgentExecutionReport>,
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
    pub origin_execution_id: Option<ExecutionId>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_execution_id: Option<ExecutionId>,
    /// Interaction Turn this input is bound to. `Some` on the root Turn path,
    /// `None` for child agent runs spawned by multi-agent tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<String>,
    pub message_id: String,
    pub content: MessageContent,
    pub delivery: AgentInputDelivery,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<ExecutionId>,
    pub disposition: crate::InputDisposition,
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
    pub report: AgentExecutionReport,
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
    ExecutionStarted {
        agent_instance_id: AgentInstanceId,
        execution_id: ExecutionId,
        started_at: i64,
    },
    RecordExecutionReport {
        report: AgentExecutionReport,
    },
    CommitReport {
        recipient_agent_instance_id: AgentInstanceId,
        report: AgentExecutionReport,
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
            activity: AgentActivity::Running {
                execution_id: "exec-1".into(),
            },
            latest_report: None,
            unread_report_count: 0,
            generation: 1,
        };
        let value = serde_json::to_value(snapshot).expect("serialize snapshot");
        assert_eq!(value["lifecycle"], "open");
        assert_eq!(value["activity"]["type"], "running");
    }
}
