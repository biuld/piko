// ---- Protocol: state — orchestrator snapshot state ----

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::agents::{AgentRuntimeState, AgentTaskState};
use super::tools::ToolSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OrchState {
    #[serde(rename = "runId")]
    pub run_id: String,
    pub status: OrchRunStatus,
    #[serde(rename = "toolSets")]
    pub tool_sets: HashMap<String, ToolSet>,
    pub agents: HashMap<String, AgentRuntimeState>,
    pub tasks: HashMap<String, AgentTaskState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum OrchRunStatus {
    Idle,
    Running,
    Stopping,
    Stopped,
}

impl OrchState {
    pub fn new(run_id: String) -> Self {
        Self {
            run_id,
            status: OrchRunStatus::Idle,
            tool_sets: HashMap::new(),
            agents: HashMap::new(),
            tasks: HashMap::new(),
        }
    }
}
