// ---- Protocol: commands — orchestrator command/response envelopes ----
// Protocol-level command and response types for remote orchestrator communication.

use serde::{Deserialize, Serialize};

use super::agents::{AgentSpec, AgentTask, AgentTaskId};
use super::runtime::{OrchModelConfig, OrchRunCommandOptions, OrchRunResult};
use super::state::OrchState;
use super::tools::ToolSet;

// ---- OrchestratorCommand ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OrchestratorCommand {
    #[serde(rename = "register_agent")]
    RegisterAgent { spec: AgentSpec },
    #[serde(rename = "unregister_agent")]
    UnregisterAgent {
        #[serde(rename = "agentId")]
        agent_id: String,
    },
    #[serde(rename = "register_tool_set")]
    RegisterToolSet {
        #[serde(rename = "toolSet")]
        tool_set: ToolSet,
    },
    #[serde(rename = "unregister_tool_set")]
    UnregisterToolSet {
        #[serde(rename = "toolSetId")]
        tool_set_id: String,
    },
    #[serde(rename = "set_model_config")]
    SetModelConfig { config: OrchModelConfig },
    #[serde(rename = "spawn")]
    Spawn { task: AgentTask },
    #[serde(rename = "run")]
    Run {
        prompt: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        options: Option<OrchRunCommandOptions>,
    },
    #[serde(rename = "cancel_task")]
    CancelTask {
        #[serde(rename = "taskId")]
        task_id: AgentTaskId,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    #[serde(rename = "snapshot")]
    Snapshot,
}

// ---- OrchestratorResponse ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OrchestratorResponse {
    #[serde(rename = "ok")]
    Ok {
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<serde_json::Value>,
    },
    #[serde(rename = "error")]
    Error { code: String, message: String },
    #[serde(rename = "task_spawned")]
    TaskSpawned {
        #[serde(rename = "taskId")]
        task_id: AgentTaskId,
    },
    #[serde(rename = "run_result")]
    RunResult { result: OrchRunResult },
    #[serde(rename = "snapshot")]
    Snapshot { state: OrchState },
}
