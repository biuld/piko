//! Agent todo list projection types (F-27 / D-39).
//!
//! Product/docs/UI = **todo list**; tools remain `todo_write` / `todo_read`
//! with a top-level `todos` array. Protocol uses `TodoList` / `TodoItem`.

use serde::{Deserialize, Serialize};

use crate::AgentInstanceId;

/// Item progress status (wire: snake_case strings).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

/// One todo item on an agent's list.
///
/// Unknown future fields are ignored on deserialize (no `deny_unknown_fields`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    /// Always a string on the wire. Tool/model numbers normalize to decimal.
    pub id: String,
    pub status: TodoStatus,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Current durable + projected todo list for one AgentInstance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TodoList {
    pub agent_instance_id: AgentInstanceId,
    pub items: Vec<TodoItem>,
    /// Epoch milliseconds.
    pub updated_at: i64,
    /// Monotonic per agent list; starts at 0.
    pub revision: u64,
}

/// Live projection event: full replace of one agent's list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TodoListUpdated {
    pub todo_list: TodoList,
}

/// Errors from normalizing tool JSON into typed items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TodoNormalizeError {
    MissingTodos,
    NotAnArray,
    InvalidItem { index: usize, reason: String },
}

impl std::fmt::Display for TodoNormalizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TodoNormalizeError::MissingTodos => write!(f, "missing todos array"),
            TodoNormalizeError::NotAnArray => write!(f, "todos is not an array"),
            TodoNormalizeError::InvalidItem { index, reason } => {
                write!(f, "invalid todo item at index {index}: {reason}")
            }
        }
    }
}

impl std::error::Error for TodoNormalizeError {}

/// Normalize a tool-args / tool-result value that carries `todos: [...]`.
///
/// - `id` number → decimal string; string kept as-is
/// - missing/empty `content` (after trim) → reject
/// - missing `status` → default to `pending` (F-27 Rev B)
/// - unknown `status` → reject with the accepted values named
/// - empty array is valid (clear list)
pub fn normalize_todos_from_tool_json(
    value: &serde_json::Value,
) -> Result<Vec<TodoItem>, TodoNormalizeError> {
    let todos = value.get("todos").ok_or(TodoNormalizeError::MissingTodos)?;
    let arr = todos.as_array().ok_or(TodoNormalizeError::NotAnArray)?;
    normalize_todo_items(arr)
}

/// Normalize a raw JSON array of todo item objects.
pub fn normalize_todo_items(
    items: &[serde_json::Value],
) -> Result<Vec<TodoItem>, TodoNormalizeError> {
    let mut out = Vec::with_capacity(items.len());
    for (index, raw) in items.iter().enumerate() {
        out.push(normalize_one_item(raw, index)?);
    }
    Ok(out)
}

fn normalize_one_item(
    raw: &serde_json::Value,
    index: usize,
) -> Result<TodoItem, TodoNormalizeError> {
    let obj = raw
        .as_object()
        .ok_or_else(|| TodoNormalizeError::InvalidItem {
            index,
            reason: "item is not an object".into(),
        })?;

    let id = match obj.get("id") {
        Some(serde_json::Value::String(s)) if !s.is_empty() => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(_) => {
            return Err(TodoNormalizeError::InvalidItem {
                index,
                reason: "id must be a string or number".into(),
            });
        }
        None => {
            return Err(TodoNormalizeError::InvalidItem {
                index,
                reason: "missing id".into(),
            });
        }
    };

    let status = match obj.get("status").and_then(|v| v.as_str()) {
        Some("pending") => TodoStatus::Pending,
        Some("in_progress") => TodoStatus::InProgress,
        Some("completed") => TodoStatus::Completed,
        Some(other) => {
            return Err(TodoNormalizeError::InvalidItem {
                index,
                reason: format!(
                    "unknown status '{other}' (expected pending|in_progress|completed)"
                ),
            });
        }
        None => TodoStatus::Pending,
    };

    let content = match obj.get("content").and_then(|v| v.as_str()) {
        Some(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Err(TodoNormalizeError::InvalidItem {
                    index,
                    reason: "content is empty".into(),
                });
            }
            trimmed.to_string()
        }
        None => {
            return Err(TodoNormalizeError::InvalidItem {
                index,
                reason: "missing content".into(),
            });
        }
    };

    let detail = obj
        .get("detail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    Ok(TodoItem {
        id,
        status,
        content,
        detail,
    })
}

/// Serialize items back to tool JSON `{ "todos": [ ... ] }` with snake_case status.
pub fn todos_tool_json(items: &[TodoItem]) -> serde_json::Value {
    serde_json::json!({ "todos": items })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_stringifies_numeric_id() {
        let v = serde_json::json!({
            "todos": [
                { "id": 1, "status": "in_progress", "content": "Ship it" }
            ]
        });
        let items = normalize_todos_from_tool_json(&v).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "1");
        assert_eq!(items[0].status, TodoStatus::InProgress);
        assert_eq!(items[0].content, "Ship it");
    }

    #[test]
    fn normalize_keeps_string_id() {
        let v = serde_json::json!({
            "todos": [
                { "id": "a", "status": "pending", "content": "Next" }
            ]
        });
        let items = normalize_todos_from_tool_json(&v).unwrap();
        assert_eq!(items[0].id, "a");
    }

    #[test]
    fn reject_empty_content() {
        let v = serde_json::json!({
            "todos": [
                { "id": 1, "status": "pending", "content": "  " }
            ]
        });
        assert!(normalize_todos_from_tool_json(&v).is_err());
    }

    #[test]
    fn reject_unknown_status() {
        let v = serde_json::json!({
            "todos": [
                { "id": 1, "status": "blocked", "content": "x" }
            ]
        });
        assert!(normalize_todos_from_tool_json(&v).is_err());
    }

    #[test]
    fn default_missing_status_to_pending() {
        let v = serde_json::json!({
            "todos": [ { "id": 1, "content": "x" } ]
        });
        let items = normalize_todos_from_tool_json(&v).unwrap();
        assert_eq!(items[0].status, TodoStatus::Pending);
    }

    #[test]
    fn empty_array_ok() {
        let v = serde_json::json!({ "todos": [] });
        let items = normalize_todos_from_tool_json(&v).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn roundtrip_serde_camel_case_and_snake_status() {
        let list = TodoList {
            agent_instance_id: "agent-1".into(),
            items: vec![TodoItem {
                id: "1".into(),
                status: TodoStatus::InProgress,
                content: "work".into(),
                detail: None,
            }],
            updated_at: 42,
            revision: 3,
        };
        let json = serde_json::to_value(&list).unwrap();
        assert_eq!(json["agentInstanceId"], "agent-1");
        assert_eq!(json["updatedAt"], 42);
        assert_eq!(json["revision"], 3);
        assert_eq!(json["items"][0]["status"], "in_progress");
        assert_eq!(json["items"][0]["id"], "1");
        let back: TodoList = serde_json::from_value(json).unwrap();
        assert_eq!(back, list);
    }

    #[test]
    fn tool_json_uses_todos_key() {
        let items = vec![TodoItem {
            id: "1".into(),
            status: TodoStatus::Completed,
            content: "done".into(),
            detail: None,
        }];
        let v = todos_tool_json(&items);
        assert!(v.get("todos").is_some());
        assert_eq!(v["todos"][0]["status"], "completed");
    }

    #[test]
    fn ignores_unknown_item_fields() {
        let v = serde_json::json!({
            "todos": [{
                "id": "1",
                "status": "pending",
                "content": "x",
                "futureField": true
            }]
        });
        let items = normalize_todos_from_tool_json(&v).unwrap();
        assert_eq!(items[0].content, "x");
    }
}
