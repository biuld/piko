// ---- TaskControlProvider — multi-agent task control tools ----
//
// Provides spawn, spawn_detached, poll_task, steer_task tools that
// agents use for delegation and multi-agent coordination.
//
// Depends on AgentSpawner port only — no application-layer coupling.

use async_trait::async_trait;
use std::sync::Arc;

use crate::domain::tools::call::ToolCall;
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutionMode, ToolExecutorRef,
    ToolProviderSource,
};
use crate::domain::tools::result::ToolExecResult;
use crate::ports::agent_spawner::AgentSpawner;
use crate::ports::tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};
use crate::runtime::dispatch::consumer::host_task_context_from_execution;

#[derive(Clone)]
pub struct TaskControlProvider {
    spawner: Arc<dyn AgentSpawner>,
}

impl TaskControlProvider {
    pub fn new(spawner: Arc<dyn AgentSpawner>) -> Self {
        Self { spawner }
    }

    fn set_agent_id_description(tool: &mut ToolDef, description: &str) {
        let Some(props) = tool.input_schema.get_mut("properties") else {
            return;
        };
        let Some(agent_id_prop) = props.get_mut("agent_id") else {
            return;
        };
        let Some(obj) = agent_id_prop.as_object_mut() else {
            return;
        };
        obj.insert("description".to_string(), serde_json::json!(description));
    }

    fn tools() -> Vec<ToolDef> {
        vec![
            ToolDef {
                name: "spawn".into(),
                description: "Spawn a task on an agent template and wait for it to complete.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Target agent template ID. Omit to use 'general'." },
                        "prompt": { "type": "string", "description": "The task prompt" }
                    },
                    "required": ["prompt"]
                }),
                executor: ToolExecutorRef {
                    kind: "orchestrator".into(),
                    target: "spawn".into(),
                    extra: None,
                },
                execution_mode: Some(ToolExecutionMode::Sequential),
                exposure: None,
                capabilities: Some(vec![ToolCapability::Delegation]),
                approval: Some(ToolApprovalRequirement::Never),
                metadata: None,
            },
            ToolDef {
                name: "spawn_detached".into(),
                description: "Spawn a task on an agent template without waiting. Returns a task ID."
                    .into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Target agent template ID. Omit to use 'general'." },
                        "prompt": { "type": "string", "description": "The task prompt" }
                    },
                    "required": ["prompt"]
                }),
                executor: ToolExecutorRef {
                    kind: "orchestrator".into(),
                    target: "spawn_detached".into(),
                    extra: None,
                },
                execution_mode: Some(ToolExecutionMode::Sequential),
                exposure: None,
                capabilities: Some(vec![ToolCapability::Delegation]),
                approval: Some(ToolApprovalRequirement::Never),
                metadata: None,
            },
            ToolDef {
                name: "poll_task".into(),
                description: "Check whether detached tasks have completed and collect their cached results. Returns immediately with whatever reports are ready; call again later if tasks are still running.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task_ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Specific task IDs to poll. If omitted, no tasks are polled."
                        }
                    }
                }),
                executor: ToolExecutorRef {
                    kind: "orchestrator".into(),
                    target: "poll_task".into(),
                    extra: None,
                },
                execution_mode: Some(ToolExecutionMode::Sequential),
                exposure: None,
                capabilities: Some(vec![]),
                approval: Some(ToolApprovalRequirement::Never),
                metadata: None,
            },
            ToolDef {
                name: "steer_task".into(),
                description: "Send a steering message to a running task.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task_id": { "type": "string", "description": "ID of the task to steer" },
                        "message": { "type": "string", "description": "The steering message" }
                    },
                    "required": ["task_id", "message"]
                }),
                executor: ToolExecutorRef {
                    kind: "orchestrator".into(),
                    target: "steer_task".into(),
                    extra: None,
                },
                execution_mode: Some(ToolExecutionMode::Sequential),
                exposure: None,
                capabilities: Some(vec![ToolCapability::Delegation]),
                approval: Some(ToolApprovalRequirement::Never),
                metadata: None,
            },
        ]
    }
}

#[async_trait]
impl ToolProvider for TaskControlProvider {
    fn id(&self) -> &str {
        "task_control"
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Orch
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        let mut tools = Self::tools();
        let agents = self.spawner.list_agents().await;

        let mut agent_list_text = String::new();
        if !agents.is_empty() {
            let names: Vec<String> = agents
                .iter()
                .filter(|a| a.id != "main" && a.id != "general")
                .map(|a| format!("'{}' ({})", a.id, a.role))
                .collect();
            if !names.is_empty() {
                agent_list_text = format!(
                    " Available agent templates: {}. Omit to use 'general'.",
                    names.join(", ")
                );
            }
        }

        let description = format!(
            "Target agent template ID. Omit to use 'general'.{}",
            agent_list_text
        );

        for tool in &mut tools {
            if tool.name == "spawn" || tool.name == "spawn_detached" {
                Self::set_agent_id_description(tool, &description);
            }
        }

        tools
    }

    async fn execute(&self, call: ToolCall, context: ToolExecutionContext) -> ToolExecResult {
        let tool_name = call.name.clone();
        let args = call.arguments.clone();

        match tool_name.as_str() {
            "spawn" => {
                let mut agent_id = args.get("agent_id").and_then(|v| v.as_str()).unwrap_or("");
                if agent_id.is_empty() {
                    agent_id = "general";
                }
                let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
                if prompt.is_empty() {
                    return ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(crate::domain::tools::result::ToolExecError {
                            code: "invalid_args".into(),
                            message: "spawn requires prompt".into(),
                            retryable: Some(false),
                        }),
                    };
                }
                let hc = host_task_context_from_execution(&context);
                let result = self
                    .spawner
                    .spawn(
                        agent_id,
                        prompt,
                        Some(context.agent_id.clone()),
                        Some(context.task_id.clone()),
                        hc,
                        context.senders.clone(),
                    )
                    .await;
                ToolExecResult {
                    ok: true,
                    value: Some(
                        result
                            .map(|r| {
                                serde_json::json!({
                                    "status": r.status,
                                    "text": r.text,
                                    "total_steps": r.total_steps,
                                })
                            })
                            .unwrap_or_else(
                                || serde_json::json!({"status": "error", "text": "no result"}),
                            ),
                    ),
                    error: None,
                }
            }
            "spawn_detached" => {
                let mut agent_id = args.get("agent_id").and_then(|v| v.as_str()).unwrap_or("");
                if agent_id.is_empty() {
                    agent_id = "general";
                }
                let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
                if prompt.is_empty() {
                    return ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(crate::domain::tools::result::ToolExecError {
                            code: "invalid_args".into(),
                            message: "spawn_detached requires prompt".into(),
                            retryable: Some(false),
                        }),
                    };
                }
                let hc = host_task_context_from_execution(&context);
                let task_id = self
                    .spawner
                    .spawn_detached(
                        agent_id,
                        prompt,
                        Some(context.agent_id.clone()),
                        Some(context.task_id.clone()),
                        hc,
                        context.senders.clone(),
                    )
                    .await;
                ToolExecResult {
                    ok: true,
                    value: Some(serde_json::json!({ "task_id": task_id, "status": "detached" })),
                    error: None,
                }
            }
            "poll_task" => {
                let task_ids: Vec<String> = args
                    .get("task_ids")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let mut results = Vec::new();
                for tid in &task_ids {
                    match self.spawner.poll_task(tid).await {
                        Some(val) => results.push(serde_json::json!({ "task_id": tid, "result": val })),
                        None => results.push(serde_json::json!({ "task_id": tid, "result": null, "warning": "Task result not available" })),
                    }
                }
                ToolExecResult {
                    ok: true,
                    value: Some(serde_json::json!({ "results": results })),
                    error: None,
                }
            }
            "steer_task" => {
                let task_id = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
                if task_id.is_empty() || message.is_empty() {
                    return ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(crate::domain::tools::result::ToolExecError {
                            code: "invalid_args".into(),
                            message: "steer_task requires task_id and message".into(),
                            retryable: Some(false),
                        }),
                    };
                }
                let steered = self
                    .spawner
                    .steer_task(
                        task_id,
                        message,
                        Some(context.task_id.clone()),
                        Some(context.agent_id.clone()),
                        context.senders.clone(),
                    )
                    .await;
                ToolExecResult {
                    ok: steered,
                    value: Some(serde_json::json!({ "steered": steered, "task_id": task_id })),
                    error: if steered {
                        None
                    } else {
                        Some(crate::domain::tools::result::ToolExecError {
                            code: "not_found".into(),
                            message: format!("Task {task_id} not found or not running"),
                            retryable: Some(true),
                        })
                    },
                }
            }
            _ => ToolExecResult {
                ok: false,
                value: None,
                error: Some(crate::domain::tools::result::ToolExecError {
                    code: "unknown_tool".into(),
                    message: format!("Unknown task_control tool: {tool_name}"),
                    retryable: Some(false),
                }),
            },
        }
    }
}
