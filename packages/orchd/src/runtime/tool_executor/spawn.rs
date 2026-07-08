use crate::ports::agent_spawner::AgentSpawner;

pub(super) async fn execute_spawn_tool(
    spawner: &std::sync::Arc<dyn AgentSpawner>,
    source_agent_id: &str,
    parent_task_id: &str,
    host_context: &Option<crate::domain::tasks::task::HostTaskContext>,
    tool_name: &str,
    args: &serde_json::Value,
    senders: &Option<crate::runtime::dispatch::DispatchSenders>,
) -> Result<serde_json::Value, String> {
    let hc = host_context
        .clone()
        .unwrap_or_else(|| crate::domain::tasks::task::HostTaskContext {
            session_id: String::new(),
            turn_id: String::new(),
        });
    let agent_id = args
        .get("agent_id")
        .and_then(|v| v.as_str())
        .filter(|id| !id.is_empty())
        .unwrap_or("general");
    let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

    match tool_name {
        "spawn" => match spawner
            .spawn(
                agent_id,
                prompt,
                Some(source_agent_id.to_string()),
                Some(parent_task_id.to_string()),
                hc,
                senders.clone(),
            )
            .await
        {
            Some(report) => {
                let mut json = serde_json::json!({
                    "status": report.status,
                    "text": report.text,
                    "total_steps": report.total_steps,
                });
                if let Some(ref tid) = report.task_id {
                    json["task_id"] = serde_json::Value::String(tid.clone());
                }
                Ok(json)
            }
            None => Err(format!("spawn on agent '{}' returned no result", agent_id)),
        },
        "spawn_detached" => {
            let task_id = spawner
                .spawn_detached(
                    agent_id,
                    prompt,
                    Some(source_agent_id.to_string()),
                    Some(parent_task_id.to_string()),
                    hc,
                    senders.clone(),
                )
                .await;
            Ok(serde_json::json!({"task_id": task_id, "status": "detached"}))
        }
        "poll_task" => {
            let task_id = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
            let timeout_ms = args.get("timeout_ms").and_then(|v| v.as_u64());
            match spawner.poll_task(task_id, timeout_ms).await {
                Some(report) => Ok(serde_json::json!({
                    "status": report.status,
                    "text": report.text,
                    "total_steps": report.total_steps,
                    "task_id": task_id,
                })),
                None => Err(format!("task {} not found or not completed", task_id)),
            }
        }
        "steer_task" => {
            let task_id = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
            let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
            let ok = spawner.steer_task(task_id, message).await;
            Ok(serde_json::json!({"ok": ok}))
        }
        _ => Err(format!("unknown spawn tool: {}", tool_name)),
    }
}

pub(super) fn is_spawn_tool(name: &str) -> bool {
    matches!(
        name,
        "spawn" | "spawn_detached" | "poll_task" | "steer_task"
    )
}
