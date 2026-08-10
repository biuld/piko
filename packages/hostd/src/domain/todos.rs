//! Host-authoritative agent todo lists (F-27 / D-39).

use piko_protocol::{
    Message, TodoList, TodoListUpdated, TodoStatus, normalize_todos_from_tool_json,
};

/// Extract a durable todo list from a successful `todo_write` tool result.
///
/// Returns `None` when the message is not a successful todo_write, or when
/// the result payload cannot be normalized (should not happen after orch
/// validation — still fail closed without mutating).
pub fn todo_list_from_tool_result(
    agent_instance_id: &str,
    message: &Message,
    previous: Option<&TodoList>,
) -> Option<TodoList> {
    let Message::ToolResult {
        tool_name,
        details,
        is_error,
        ..
    } = message
    else {
        return None;
    };
    if is_error.unwrap_or(false) {
        return None;
    }
    if tool_name.as_deref() != Some("todo_write") {
        return None;
    }
    let details = details.as_ref()?;
    let items = normalize_todos_from_tool_json(details).ok()?;
    let now = now_ms();
    let revision = previous.map(|p| p.revision.saturating_add(1)).unwrap_or(1);
    Some(TodoList {
        agent_instance_id: agent_instance_id.to_string(),
        items,
        updated_at: now,
        revision,
    })
}

/// Render the `todo.list` prompt fragment body for a non-empty list.
pub fn todo_list_fragment_content(list: &TodoList) -> Option<String> {
    if list.items.is_empty() {
        return None;
    }
    let remaining = list
        .items
        .iter()
        .filter(|i| matches!(i.status, TodoStatus::Pending | TodoStatus::InProgress))
        .count();
    let mut lines = vec![format!(
        "Current todo list ({} items, {} remaining):",
        list.items.len(),
        remaining
    )];
    for item in &list.items {
        lines.push(format!(
            "{} {}: {}",
            status_mark(item.status),
            item.id,
            item.content
        ));
    }
    Some(lines.join("\n"))
}

/// Standing drive instruction when todo tools are available.
pub const TODO_DRIVE_INSTRUCTION: &str = "\
Maintain a todo list for multi-step work via todo_write / todo_read. \
The current list is injected when non-empty. Prefer completing remaining \
pending and in_progress items unless the user redirects; update the list \
when the plan or progress changes so it stays an accurate lossy plan.";

fn status_mark(status: TodoStatus) -> &'static str {
    match status {
        TodoStatus::Completed => "[x]",
        TodoStatus::InProgress => "[~]",
        TodoStatus::Pending => "[ ]",
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Build a live projection event from a list.
pub fn todo_list_updated_event(list: TodoList) -> TodoListUpdated {
    TodoListUpdated { todo_list: list }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::{ContentBlock, TodoItem};

    fn tool_result(name: &str, details: serde_json::Value, err: bool) -> Message {
        Message::ToolResult {
            tool_call_id: "c1".into(),
            tool_name: Some(name.into()),
            content: vec![ContentBlock::Text { text: "ok".into() }],
            details: Some(details),
            is_error: Some(err),
            timestamp: None,
        }
    }

    #[test]
    fn extracts_normalized_list_from_todo_write() {
        let msg = tool_result(
            "todo_write",
            serde_json::json!({
                "todos": [
                    { "id": "1", "status": "pending", "content": "A" },
                    { "id": "2", "status": "completed", "content": "B" }
                ]
            }),
            false,
        );
        let list = todo_list_from_tool_result("agent-a", &msg, None).unwrap();
        assert_eq!(list.agent_instance_id, "agent-a");
        assert_eq!(list.items.len(), 2);
        assert_eq!(list.revision, 1);
    }

    #[test]
    fn failed_write_does_not_extract() {
        let msg = tool_result("todo_write", serde_json::json!({ "error": "bad" }), true);
        assert!(todo_list_from_tool_result("a", &msg, None).is_none());
    }

    #[test]
    fn other_tools_ignored() {
        let msg = tool_result("bash", serde_json::json!({ "todos": [] }), false);
        assert!(todo_list_from_tool_result("a", &msg, None).is_none());
    }

    #[test]
    fn fragment_omits_empty() {
        let list = TodoList {
            agent_instance_id: "a".into(),
            items: vec![],
            updated_at: 0,
            revision: 0,
        };
        assert!(todo_list_fragment_content(&list).is_none());
    }

    #[test]
    fn fragment_renders_statuses() {
        let list = TodoList {
            agent_instance_id: "a".into(),
            items: vec![
                TodoItem {
                    id: "1".into(),
                    status: TodoStatus::Completed,
                    content: "done work".into(),
                    detail: None,
                },
                TodoItem {
                    id: "2".into(),
                    status: TodoStatus::InProgress,
                    content: "in progress work".into(),
                    detail: None,
                },
                TodoItem {
                    id: "3".into(),
                    status: TodoStatus::Pending,
                    content: "still pending".into(),
                    detail: None,
                },
            ],
            updated_at: 1,
            revision: 1,
        };
        let text = todo_list_fragment_content(&list).unwrap();
        assert!(text.contains("3 items, 2 remaining"));
        assert!(text.contains("[x] 1: done work"));
        assert!(text.contains("[~] 2: in progress work"));
        assert!(text.contains("[ ] 3: still pending"));
    }

    #[test]
    fn bumps_revision_from_previous() {
        let prev = TodoList {
            agent_instance_id: "a".into(),
            items: vec![],
            updated_at: 0,
            revision: 4,
        };
        let msg = tool_result(
            "todo_write",
            serde_json::json!({ "todos": [{ "id": "1", "status": "pending", "content": "x" }] }),
            false,
        );
        let list = todo_list_from_tool_result("a", &msg, Some(&prev)).unwrap();
        assert_eq!(list.revision, 5);
    }

    #[test]
    fn empty_write_extracts_empty_list_for_clear() {
        let prev = TodoList {
            agent_instance_id: "a".into(),
            items: vec![TodoItem {
                id: "1".into(),
                status: TodoStatus::Pending,
                content: "old".into(),
                detail: None,
            }],
            updated_at: 1,
            revision: 2,
        };
        let msg = tool_result("todo_write", serde_json::json!({ "todos": [] }), false);
        let list = todo_list_from_tool_result("a", &msg, Some(&prev)).unwrap();
        assert!(list.items.is_empty());
        assert_eq!(list.revision, 3);
    }
}
