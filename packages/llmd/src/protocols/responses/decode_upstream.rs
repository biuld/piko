use serde_json::Value;

use super::support::string;
use crate::gateway::InferenceRequest;
use crate::target::ModelTarget;
use crate::tools::{UpstreamActivityStatus, UpstreamToolActivity};

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
    Some(UpstreamToolActivity {
        activity_id: string(item, "id").unwrap_or_else(|| support.name.clone()),
        tool_name,
        kind: support.kind.clone(),
        status,
    })
}
