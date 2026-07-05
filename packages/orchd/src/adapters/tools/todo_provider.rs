// ---- TodoProvider — per-agent todo-list management ----
//
// Self-contained tool provider that manages a working todo list
// keyed by agent_id. State lives inside the provider (ephemeral,
// per-session) — not coupled to agent/task core types.
//
// Tools:
//   todo_write — replace the todo list for the current agent
//   todo_read  — read the current todo list for the current agent

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::domain::tools::call::ToolCallData;
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutionMode, ToolExecutorRef,
    ToolProviderSource,
};
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::ports::tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};

/// Todo provider state — keyed by agent_id (persists across tasks).
#[derive(Clone)]
pub struct TodoProvider {
    state: Arc<RwLock<HashMap<String, Vec<serde_json::Value>>>>,
}

impl Default for TodoProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl TodoProvider {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn tools() -> Vec<ToolDef> {
        vec![
            ToolDef {
                name: "todo_write".into(),
                description: "Create or replace a structured task list for your current coding session. Use this to plan and track multi-step work. Each item should have 'id' (number), 'status' (pending/in_progress/completed), and 'content' (description).".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "todos": {
                            "type": "array",
                            "description": "The complete list of todo items",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "integer" },
                                    "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] },
                                    "content": { "type": "string" }
                                },
                                "required": ["id", "status", "content"]
                            }
                        }
                    },
                    "required": ["todos"]
                }),
                executor: ToolExecutorRef { kind: "todo".into(), target: "todo_write".into(), extra: None },
                execution_mode: Some(ToolExecutionMode::Sequential),
                exposure: None,
                capabilities: Some(vec![ToolCapability::UpdatePlan]),
                approval: Some(ToolApprovalRequirement::Never),
                metadata: None,
            },
            ToolDef {
                name: "todo_read".into(),
                description: "Read the current todo list for this task.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
                executor: ToolExecutorRef { kind: "todo".into(), target: "todo_read".into(), extra: None },
                execution_mode: Some(ToolExecutionMode::Sequential),
                exposure: None,
                capabilities: Some(vec![ToolCapability::UpdatePlan]),
                approval: Some(ToolApprovalRequirement::Never),
                metadata: None,
            },
        ]
    }
}

#[async_trait]
impl ToolProvider for TodoProvider {
    fn id(&self) -> &str {
        "todo"
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Orch
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        Self::tools()
    }

    async fn execute(&self, call: ToolCallData, context: ToolExecutionContext) -> ToolExecResult {
        let tool_name = call.name.clone();
        let args = call.arguments.clone();

        match tool_name.as_str() {
            "todo_write" => {
                let todos: Vec<serde_json::Value> = args
                    .get("todos")
                    .and_then(|v| v.as_array())
                    .map(|a| a.to_vec())
                    .unwrap_or_default();
                self.state
                    .write()
                    .await
                    .insert(context.agent_id.clone(), todos);
                ToolExecResult {
                    ok: true,
                    value: Some(serde_json::json!({"updated": true})),
                    error: None,
                }
            }
            "todo_read" => {
                let todos = self
                    .state
                    .read()
                    .await
                    .get(&context.agent_id)
                    .cloned()
                    .unwrap_or_default();
                ToolExecResult {
                    ok: true,
                    value: Some(serde_json::json!({"todos": todos})),
                    error: None,
                }
            }
            _ => ToolExecResult {
                ok: false,
                value: None,
                error: Some(ToolExecError {
                    code: "unknown_tool".into(),
                    message: format!("Unknown todo tool: {tool_name}"),
                    retryable: Some(false),
                }),
            },
        }
    }
}
