// ---- TaskControlProvider — built-in task control tools ----
//
// Provides spawn, spawn_detached, await_task, state, plan tools that
// agents use for multi-agent coordination.
//
// The OrchCore reference is injected after construction via
// set_orchestrator(). Once set, execute() delegates to the real
// OrchCore methods.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::application::orchestrator::OrchCore;
use crate::domain::tasks::task::{AgentTask, TaskSource};
use crate::domain::tools::call::ToolCall;
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutionMode, ToolExecutorRef,
    ToolProviderSource,
};
use crate::domain::tools::result::ToolExecResult;
use crate::ports::tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};

/// Built-in task control tool provider.
///
/// Stateless on its own — all control-plane state is managed by the
/// Orchestrator facade, injected via set_orchestrator().
#[derive(Clone)]
pub struct TaskControlProvider {
    orch: Arc<Mutex<Option<Arc<OrchCore>>>>,
}

impl std::fmt::Debug for TaskControlProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskControlProvider")
            .field("orch", &"..")
            .finish()
    }
}

impl Default for TaskControlProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskControlProvider {
    pub fn new() -> Self {
        Self {
            orch: Arc::new(Mutex::new(None)),
        }
    }

    /// Inject the orchestrator reference. Must be called before any tool
    /// execution, typically during `OrchCore::init()`.
    pub async fn set_orchestrator(&self, orchestrator: Arc<OrchCore>) {
        let mut guard = self.orch.lock().await;
        *guard = Some(orchestrator);
    }

    fn builtin_tools() -> Vec<ToolDef> {
        vec![
            ToolDef {
                name: "spawn".into(),
                description: "Spawn a task on a sub-agent and wait for it to complete. Use this when you need results from a sub-agent before continuing.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "description": "Target agent ID to spawn the task on"
                        },
                        "prompt": {
                            "type": "string",
                            "description": "The task prompt for the sub-agent"
                        }
                    },
                    "required": ["agent_id", "prompt"]
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
                description: "Spawn a task on a sub-agent without waiting. Returns immediately with a task ID. Use await_task later to collect results.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "description": "Target agent ID to spawn the task on"
                        },
                        "prompt": {
                            "type": "string",
                            "description": "The task prompt for the sub-agent"
                        }
                    },
                    "required": ["agent_id", "prompt"]
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
                name: "await_task".into(),
                description: "Wait for previously detached tasks to complete and collect their results.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task_ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional specific task IDs to await. If omitted, awaits all outstanding detached tasks."
                        }
                    }
                }),
                executor: ToolExecutorRef {
                    kind: "orchestrator".into(),
                    target: "await_task".into(),
                    extra: None,
                },
                execution_mode: Some(ToolExecutionMode::Sequential),
                exposure: None,
                capabilities: Some(vec![]),
                approval: Some(ToolApprovalRequirement::Never),
                metadata: None,
            },
            ToolDef {
                name: "state".into(),
                description: "Get the current orchestrator state snapshot".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "description": "Optional: filter by agent ID"
                        }
                    }
                }),
                executor: ToolExecutorRef {
                    kind: "orchestrator".into(),
                    target: "state".into(),
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
                description: "Send a steering message to a running task. The target task will consume the message on its next step. Use this to guide sub-agents in real-time.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "ID of the task to steer"
                        },
                        "message": {
                            "type": "string",
                            "description": "The steering message to inject into the target task's context"
                        }
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
            ToolDef {
                name: "plan".into(),
                description: "Update the agent's plan with structured steps".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "plan": {
                            "type": "array",
                            "description": "Structured plan steps"
                        }
                    },
                    "required": ["plan"]
                }),
                executor: ToolExecutorRef {
                    kind: "orchestrator".into(),
                    target: "plan".into(),
                    extra: None,
                },
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
impl ToolProvider for TaskControlProvider {
    fn id(&self) -> &str {
        "orch"
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Orch
    }

    async fn discover(
        &self,
        _context: ToolDiscoveryContext,
    ) -> Vec<ToolDef> {
        Self::builtin_tools()
    }

    async fn execute(
        &self,
        call: ToolCall,
        context: ToolExecutionContext,
    ) -> ToolExecResult {
        let orch = {
            let guard = self.orch.lock().await;
            guard.clone()
        };

        let Some(orchestrator) = orch else {
            return ToolExecResult {
                ok: false,
                value: None,
                error: Some(crate::domain::tools::result::ToolExecError {
                    code: "not_initialized".into(),
                    message: "Orchestrator not injected into TaskControlProvider. Call set_orchestrator() during init.".into(),
                    retryable: Some(false),
                }),
            };
        };

        // Extract tool name and args
        let (tool_name, args) = match &call {
            ToolCall::ToolCall {
                name, arguments, ..
            } => (name.clone(), arguments.clone()),
            _ => {
                return ToolExecResult {
                    ok: false,
                    value: None,
                    error: Some(crate::domain::tools::result::ToolExecError {
                        code: "invalid_call".into(),
                        message: "Expected a ToolCall content block".into(),
                        retryable: Some(false),
                    }),
                };
            }
        };

        match tool_name.as_str() {
            "spawn" => {
                let agent_id = args.get("agent_id").and_then(|v| v.as_str()).unwrap_or("");
                let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

                if agent_id.is_empty() || prompt.is_empty() {
                    return ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(crate::domain::tools::result::ToolExecError {
                            code: "invalid_args".into(),
                            message: "spawn requires agent_id and prompt".into(),
                            retryable: Some(false),
                        }),
                    };
                }

                let task = AgentTask {
                    id: None,
                    target_agent_id: agent_id.to_string(),
                    prompt: prompt.to_string(),
                    source: TaskSource::Agent {
                        agent_id: context.agent_id.clone(),
                        task_id: context.task_id.clone(),
                    },
                    priority: None,
                    parent_task_id: Some(context.task_id.clone()),
                    history: None,
                    host_context: context.host_context.clone(),
                };

                let (task_id, result) = orchestrator.spawn(task).await;
                ToolExecResult {
                    ok: true,
                    value: Some(serde_json::json!({
                        "task_id": task_id,
                        "status": "completed",
                        "result": result
                    })),
                    error: None,
                }
            }
            "spawn_detached" => {
                let agent_id = args.get("agent_id").and_then(|v| v.as_str()).unwrap_or("");
                let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

                if agent_id.is_empty() || prompt.is_empty() {
                    return ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(crate::domain::tools::result::ToolExecError {
                            code: "invalid_args".into(),
                            message: "spawn_detached requires agent_id and prompt".into(),
                            retryable: Some(false),
                        }),
                    };
                }

                let task = AgentTask {
                    id: None,
                    target_agent_id: agent_id.to_string(),
                    prompt: prompt.to_string(),
                    source: TaskSource::Agent {
                        agent_id: context.agent_id.clone(),
                        task_id: context.task_id.clone(),
                    },
                    priority: None,
                    parent_task_id: Some(context.task_id.clone()),
                    history: None,
                    host_context: context.host_context.clone(),
                };

                let task_id = orchestrator.spawn_detached(task).await;
                ToolExecResult {
                    ok: true,
                    value: Some(serde_json::json!({
                        "task_id": task_id,
                        "status": "detached"
                    })),
                    error: None,
                }
            }
            "await_task" => {
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
                    match orchestrator.await_task(tid).await {
                        Some(val) => {
                            results.push(serde_json::json!({
                                "task_id": tid,
                                "result": val
                            }));
                        }
                        None => {
                            results.push(serde_json::json!({
                                "task_id": tid,
                                "result": null,
                                "warning": "Task result not available (may still be running or already collected)"
                            }));
                        }
                    }
                }

                ToolExecResult {
                    ok: true,
                    value: Some(serde_json::json!({
                        "results": results
                    })),
                    error: None,
                }
            }
            "state" => {
                let snapshot = orchestrator.snapshot().await;
                ToolExecResult {
                    ok: true,
                    value: Some(serde_json::to_value(&snapshot).unwrap_or_default()),
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

                let steered = orchestrator
                    .steer_task(task_id, &context.task_id, &context.agent_id, message)
                    .await;
                ToolExecResult {
                    ok: steered,
                    value: Some(serde_json::json!({
                        "steered": steered,
                        "task_id": task_id
                    })),
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
            "plan" => {
                let plan: Vec<serde_json::Value> = args
                    .get("plan")
                    .and_then(|v| v.as_array())
                    .map(|a| a.to_vec())
                    .unwrap_or_default();

                orchestrator
                    .update_plan(&context.agent_id, &context.task_id, plan)
                    .await;
                ToolExecResult {
                    ok: true,
                    value: Some(serde_json::json!({"updated": true})),
                    error: None,
                }
            }
            _ => ToolExecResult {
                ok: false,
                value: None,
                error: Some(crate::domain::tools::result::ToolExecError {
                    code: "unknown_tool".into(),
                    message: format!("Unknown orch tool: {}", tool_name),
                    retryable: Some(false),
                }),
            },
        }
    }
}
