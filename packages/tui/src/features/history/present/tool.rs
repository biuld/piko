use piko_protocol::{HistoryItemContent, HistoryItemDetail, TrajectoryRecord};
use ratatui::text::Line;

use crate::{
    app::ToolStatus,
    features::timeline::{ToolEntry, tool_lines},
    theme::Theme,
};

pub(super) fn tool_call_card(
    detail: &HistoryItemDetail,
    include_result: bool,
    theme: &Theme,
    width: u16,
) -> Option<Vec<Line<'static>>> {
    let HistoryItemContent::Message { message, .. } = detail.content.as_ref()? else {
        return None;
    };
    let piko_protocol::Message::ToolCall {
        id,
        name,
        arguments,
        ..
    } = message
    else {
        return None;
    };
    let observation = match detail.diagnostic.as_deref() {
        Some(TrajectoryRecord::ToolCall(record)) => Some(record),
        _ => None,
    };
    if include_result
        && observation.is_none_or(|record| record.result.is_none() && record.error.is_none())
    {
        return None;
    }

    let status = observation.map_or(ToolStatus::Completed, |record| match record.status {
        piko_protocol::TrajectoryToolCallStatus::Started
        | piko_protocol::TrajectoryToolCallStatus::Running
        | piko_protocol::TrajectoryToolCallStatus::AwaitingApproval => ToolStatus::Running,
        piko_protocol::TrajectoryToolCallStatus::Completed => ToolStatus::Completed,
        piko_protocol::TrajectoryToolCallStatus::Failed => ToolStatus::Failed,
        piko_protocol::TrajectoryToolCallStatus::Cancelled => ToolStatus::Cancelled,
    });
    let result = include_result
        .then(|| observation.and_then(|record| record.result.as_ref()))
        .flatten()
        .map(json_text);
    let mut entry = ToolEntry::new(
        id.clone(),
        name.clone(),
        status,
        json_text(arguments),
        result,
        None,
    );
    entry.result_details = include_result
        .then(|| observation.and_then(|record| record.error.clone()))
        .flatten();
    entry.expanded = true;
    Some(tool_lines(&entry, false, theme, width))
}

fn json_text(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}
