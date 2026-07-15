//! Prompt assembly DTOs shared across hostd and the Agent runtime.

use serde::{Deserialize, Serialize};

use crate::{AgentInstanceId, AgentSpec, ToolDef};

pub const AGENT_RUN_PROMPT_ASSEMBLY_VERSION: u32 = 1;

/// Host-owned, immutable resources captured for one Agent run.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PromptResourceSnapshot {
    pub product_instructions: String,
    pub context_section: String,
    pub skills_section: String,
    pub prompt_templates_section: String,
    pub environment_section: String,
}

/// Immutable rendered prompt reused by every Model Step in one Agent run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunPrompt {
    pub system_prompt: String,
    pub assembly_version: u32,
    pub source_digest: String,
}

/// Trusted request passed to the host-owned prompt assembler after tool discovery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PromptAssemblyRequest {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub agent_spec: AgentSpec,
    pub resources: PromptResourceSnapshot,
    pub tool_catalog: Vec<ToolDef>,
}
