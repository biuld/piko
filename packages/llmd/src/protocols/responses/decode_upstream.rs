use serde_json::Value;

use super::support::string;
use crate::gateway::InferenceRequest;
use crate::target::ModelTarget;
use crate::tools::{UpstreamActivityStatus, UpstreamToolActivity};
use piko_protocol::messages::UpstreamAction;

pub(super) fn upstream_activity(
    item: &Value,
    item_type: &str,
    target: &ModelTarget,
    request: &InferenceRequest,
    default_status: UpstreamActivityStatus,
) -> Option<UpstreamToolActivity> {
    let status = match string(item, "status").as_deref() {
        Some("queued") => UpstreamActivityStatus::Started,
        Some("in_progress") | Some("searching") => UpstreamActivityStatus::InProgress,
        Some("completed") => UpstreamActivityStatus::Completed,
        Some("failed") => UpstreamActivityStatus::Failed,
        _ => default_status,
    };
    let support = target.upstream_tool_for_activity(item_type)?;
    let tool_name = request
        .tools
        .iter()
        .find(|tool| tool.upstream_kind() == Some(&support.kind))
        .map(|tool| tool.name().to_owned())
        .unwrap_or_else(|| support.name.clone());
    let arguments = upstream_arguments(item);
    let action = upstream_action(item);
    tracing::debug!(
        item_type,
        activity_id = string(item, "id").unwrap_or_default(),
        status = ?status,
        action = ?action.as_ref(),
        "upstream activity decoded"
    );
    Some(UpstreamToolActivity {
        activity_id: string(item, "id").unwrap_or_else(|| support.name.clone()),
        tool_name,
        kind: support.kind.clone(),
        status,
        arguments,
        action,
    })
}

/// Capture the raw provider-echoed arguments (e.g. `action` / `input`). The
/// provider-specific shape is interpreted later by a typed view.
fn upstream_arguments(item: &Value) -> Option<Value> {
    item.get("action")
        .or_else(|| item.get("input"))
        .or_else(|| item.get("query"))
        .cloned()
}

/// Typed, cleaned action for a known upstream tool. Strips provider-internal
/// markers (`ws_call_id=` query / URL fragment) so consumers read user-facing
/// values. Returns `None` for unknown action types, which stay opaque.
fn upstream_action(item: &Value) -> Option<UpstreamAction> {
    let action = item.get("action")?;
    match string(action, "type").as_deref()? {
        "search" => {
            let queries = action
                .get("queries")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .filter(|q| !q.starts_with("ws_call_id="))
                        .collect()
                })
                .unwrap_or_default();
            Some(UpstreamAction::Search { queries })
        }
        "open_page" => {
            let url = string(action, "url")?
                .split('#')
                .next()
                .unwrap_or_default()
                .to_string();
            Some(UpstreamAction::OpenPage { url })
        }
        _ => None,
    }
}
