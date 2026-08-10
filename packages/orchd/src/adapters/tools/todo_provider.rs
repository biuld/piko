// ---- TodoProvider — per-agent todo-list management ----
//
// Self-contained tool provider that manages a working todo list
// keyed by agent_id. Runtime source of truth during the process;
// hostd persists and projects after publish (F-27 / D-39).
//
// Tools:
//   todo_write — replace the todo list for the current agent
//   todo_read  — read the current todo list for the current agent

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use piko_protocol::{TodoItem, TodoList, normalize_todos_from_tool_json, todos_tool_json};
use tokio::sync::RwLock;

use crate::domain::tools::call::ToolCall;
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutionMode, ToolExecutorRef,
    ToolProviderSource,
};
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::ports::tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};

/// Per-agent runtime todo store (typed items + revision).
#[derive(Clone, Debug, Default)]
struct AgentTodoState {
    items: Vec<TodoItem>,
    revision: u64,
    updated_at: i64,
}

/// Todo provider state — keyed by agent_id (persists across tasks).
#[derive(Clone)]
pub struct TodoProvider {
    state: Arc<RwLock<HashMap<String, AgentTodoState>>>,
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

    /// Seed runtime store from host durable lists (session hydrate).
    pub async fn seed_from_lists(&self, lists: impl IntoIterator<Item = TodoList>) {
        let mut guard = self.state.write().await;
        for list in lists {
            guard.insert(
                list.agent_instance_id,
                AgentTodoState {
                    items: list.items,
                    revision: list.revision,
                    updated_at: list.updated_at,
                },
            );
        }
    }

    /// Snapshot one agent's list for host publish / seed checks.
    pub async fn list_for(&self, agent_id: &str) -> Option<TodoList> {
        let guard = self.state.read().await;
        let entry = guard.get(agent_id)?;
        Some(TodoList {
            agent_instance_id: agent_id.to_string(),
            items: entry.items.clone(),
            updated_at: entry.updated_at,
            revision: entry.revision,
        })
    }

    fn tools() -> Vec<ToolDef> {
        vec![
            ToolDef {
                name: "todo_write".into(),
                version: "1".into(),
                provenance: piko_protocol::PromptSource::new("built-in-tool", "todo/todo_write"),
                description: "Create or replace a structured todo list for your current coding session. Use this to plan and track multi-step work. Each item has 'id' (string or number), 'content' (description), and optional 'status' (pending/in_progress/completed; omitted status defaults to pending).".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "todos": {
                            "type": "array",
                            "description": "The complete list of todo items",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "oneOf": [ { "type": "string" }, { "type": "integer" } ] },
                                    "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] },
                                    "content": { "type": "string" }
                                },
                                "required": ["id", "content"]
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
                version: "1".into(),
                provenance: piko_protocol::PromptSource::new("built-in-tool", "todo/todo_read"),
                description: "Read the current todo list for this agent.".into(),
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

    fn now_ms() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
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

    async fn execute(&self, call: ToolCall, context: ToolExecutionContext) -> ToolExecResult {
        let tool_name = call.name.clone();
        let args = call.arguments.clone();

        match tool_name.as_str() {
            "todo_write" => match normalize_todos_from_tool_json(&args) {
                Ok(items) => {
                    let key = context.agent_instance_id.clone();
                    let mut guard = self.state.write().await;
                    let entry = guard.entry(key).or_default();
                    entry.revision = entry.revision.saturating_add(1);
                    entry.updated_at = Self::now_ms();
                    entry.items = items;
                    let normalized = todos_tool_json(&entry.items);
                    ToolExecResult {
                        ok: true,
                        value: Some(normalized),
                        error: None,
                    }
                }
                Err(err) => ToolExecResult {
                    ok: false,
                    value: None,
                    error: Some(ToolExecError {
                        code: "invalid_todo_list".into(),
                        message: err.to_string(),
                        retryable: Some(false),
                    }),
                },
            },
            "todo_read" => {
                let items = self
                    .state
                    .read()
                    .await
                    .get(&context.agent_instance_id)
                    .map(|e| e.items.clone())
                    .unwrap_or_default();
                ToolExecResult {
                    ok: true,
                    value: Some(todos_tool_json(&items)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::tools::call::ToolCall;
    use crate::ports::tool_provider::{ToolExecutionContext, ToolProvider};

    fn ctx(agent: &str) -> ToolExecutionContext {
        ToolExecutionContext {
            session_id: "s1".into(),
            agent_instance_id: agent.into(),
            execution_id: "e1".into(),
            cancellation: None,
            agent_id: "main".into(),
            agent_role: None,
            tool_set_ids: vec![],
            turn_index: None,
            event_seq: None,
            next_event_seq: None,
            parent_message_id: None,
            content_index: None,
            tool_call_index: None,
            tool_entity_id: None,
            host_context: None,
            source_turn_id: None,
            context_remaining: None,
        }
    }

    fn call(name: &str, args: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "c1".into(),
            name: name.into(),
            arguments: args,
            partial_json: None,
        }
    }

    #[tokio::test]
    async fn write_normalizes_and_replaces() {
        let provider = TodoProvider::new();
        let result = provider
            .execute(
                call(
                    "todo_write",
                    serde_json::json!({
                        "todos": [
                            { "id": 1, "status": "pending", "content": "A" },
                            { "id": "2", "status": "completed", "content": "B" }
                        ]
                    }),
                ),
                ctx("agent-a"),
            )
            .await;
        assert!(result.ok);
        let todos = result.value.unwrap()["todos"].as_array().unwrap().clone();
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0]["id"], "1");
        assert_eq!(todos[1]["id"], "2");

        let list = provider.list_for("agent-a").await.unwrap();
        assert_eq!(list.revision, 1);
        assert_eq!(list.items.len(), 2);
    }

    #[tokio::test]
    async fn invalid_write_does_not_mutate() {
        let provider = TodoProvider::new();
        // Seed valid list
        provider
            .execute(
                call(
                    "todo_write",
                    serde_json::json!({
                        "todos": [{ "id": 1, "status": "pending", "content": "keep" }]
                    }),
                ),
                ctx("agent-a"),
            )
            .await;
        let before = provider.list_for("agent-a").await.unwrap();

        let bad = provider
            .execute(
                call(
                    "todo_write",
                    serde_json::json!({
                        "todos": [{ "id": 2, "status": "nope", "content": "x" }]
                    }),
                ),
                ctx("agent-a"),
            )
            .await;
        assert!(!bad.ok);
        let after = provider.list_for("agent-a").await.unwrap();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn empty_write_clears_list() {
        let provider = TodoProvider::new();
        provider
            .execute(
                call(
                    "todo_write",
                    serde_json::json!({
                        "todos": [{ "id": 1, "status": "pending", "content": "x" }]
                    }),
                ),
                ctx("agent-a"),
            )
            .await;
        let clear = provider
            .execute(
                call("todo_write", serde_json::json!({ "todos": [] })),
                ctx("agent-a"),
            )
            .await;
        assert!(clear.ok);
        let list = provider.list_for("agent-a").await.unwrap();
        assert!(list.items.is_empty());
    }

    #[tokio::test]
    async fn read_returns_normalized_todos() {
        let provider = TodoProvider::new();
        provider
            .execute(
                call(
                    "todo_write",
                    serde_json::json!({
                        "todos": [{ "id": 7, "status": "in_progress", "content": "work" }]
                    }),
                ),
                ctx("agent-b"),
            )
            .await;
        let read = provider
            .execute(call("todo_read", serde_json::json!({})), ctx("agent-b"))
            .await;
        assert!(read.ok);
        assert_eq!(read.value.unwrap()["todos"][0]["id"], "7");
    }

    #[tokio::test]
    async fn seed_from_lists() {
        let provider = TodoProvider::new();
        provider
            .seed_from_lists([TodoList {
                agent_instance_id: "seeded".into(),
                items: vec![TodoItem {
                    id: "a".into(),
                    status: piko_protocol::TodoStatus::Pending,
                    content: "from host".into(),
                    detail: None,
                }],
                updated_at: 9,
                revision: 4,
            }])
            .await;
        let list = provider.list_for("seeded").await.unwrap();
        assert_eq!(list.revision, 4);
        assert_eq!(list.items[0].content, "from host");
    }

    #[tokio::test]
    async fn clone_shares_state_for_seed_and_registry() {
        // Bootstrap registers one TodoProvider and keeps a clone for seed.
        let registered = TodoProvider::new();
        let seed_handle = registered.clone();
        seed_handle
            .seed_from_lists([TodoList {
                agent_instance_id: "agent-x".into(),
                items: vec![TodoItem {
                    id: "1".into(),
                    status: piko_protocol::TodoStatus::InProgress,
                    content: "hydrated".into(),
                    detail: None,
                }],
                updated_at: 1,
                revision: 7,
            }])
            .await;
        let read = registered
            .execute(call("todo_read", serde_json::json!({})), ctx("agent-x"))
            .await;
        assert!(read.ok);
        assert_eq!(read.value.unwrap()["todos"][0]["content"], "hydrated");
    }
}
