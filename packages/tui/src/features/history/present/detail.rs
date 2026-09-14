use piko_protocol::{HistoryAvailability, HistoryItemContent, HistoryItemDetail};

use super::super::{DetailTab, HistoryRow};
use super::content::{fields, message_content, message_lines, section};
use super::context::row_context;
use super::labels::{step_outcome_word, terminal_word, tool_status_word};
use super::paint::{field_lines, kv, plain, wrapped};
use crate::theme::Theme;
use ratatui::text::Line;

/// Lines for one detail tab. Tabs with no recorded content explain their
/// absence instead of rendering empty space.
pub(crate) fn tab_lines(
    tab: DetailTab,
    detail: Option<&HistoryItemDetail>,
    row: Option<&HistoryRow>,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    match tab {
        DetailTab::Summary => summary_lines(detail, row, theme, width),
        DetailTab::Payload => payload_lines(detail, row, theme, width),
        DetailTab::Result => result_lines(detail, row, theme, width),
        DetailTab::Timing => timing_lines(detail, row, theme, width),
    }
}

fn unavailable(theme: &Theme, width: u16, reason: &str) -> Vec<Line<'static>> {
    let mut lines = vec![plain("unavailable", theme.warning, width)];
    lines.extend(wrapped(reason, theme.muted, width));
    lines
}

fn summary_lines(
    detail: Option<&HistoryItemDetail>,
    row: Option<&HistoryRow>,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(row) = row {
        lines.extend(row_context(row, theme, width));
        if let HistoryRow::Stream(item) = row {
            lines.push(Line::from(""));
            lines.push(plain("Journal", theme.muted, width));
            lines.extend(field_lines(
                "Position",
                format!("revision {} · event {}", item.revision, item.event_index),
                theme,
                width,
            ));
            lines.extend(field_lines(
                "Committed",
                timestamp(item.committed_at),
                theme,
                width,
            ));
        }
        lines.push(Line::from(""));
    }
    match detail {
        Some(detail) => {
            lines.extend(field_lines(
                "Snapshot",
                format!("revision {}", detail.item_ref.revision),
                theme,
                width,
            ));
            match &detail.availability {
                HistoryAvailability::Unavailable { reason } => {
                    lines.extend(unavailable(theme, width, reason));
                }
                HistoryAvailability::Available => {}
            }
            if let Some(HistoryItemContent::ModelStep { boundary }) = detail.content.as_ref() {
                lines.push(kv(
                    "outcome",
                    step_outcome_word(boundary.outcome),
                    theme,
                    width,
                ));
                lines.push(kv(
                    "step",
                    // 1-based; matches the step id suffix (`step_6`).
                    boundary.step_index.to_string(),
                    theme,
                    width,
                ));
                lines.extend(section(
                    "Ordered tool declarations",
                    &boundary.tool_call_message_ids.join("\n"),
                    theme,
                    width,
                ));
            }
        }
        None => lines.extend(wrapped(
            "Open the row to inspect its recorded detail.",
            theme.muted,
            width,
        )),
    }
    lines
}

fn origin_role(origin: &piko_protocol::AgentInputOrigin) -> &'static str {
    match origin {
        piko_protocol::AgentInputOrigin::User => "user",
        piko_protocol::AgentInputOrigin::Agent => "agent",
        piko_protocol::AgentInputOrigin::System => "system",
    }
}

fn payload_lines(
    detail: Option<&HistoryItemDetail>,
    row: Option<&HistoryRow>,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    if let Some(detail) = detail
        && let Some(lines) = super::tool::tool_call_card(detail, false, theme, width)
    {
        return lines;
    }
    let mut lines = Vec::new();
    match detail.and_then(|detail| detail.content.as_ref()) {
        Some(HistoryItemContent::Input { input }) => {
            lines.push(plain(origin_role(&input.origin), theme.accent_user, width));
            lines.extend(message_content(&input.content, theme, width));
        }
        Some(HistoryItemContent::Message { message, .. }) => {
            lines.extend(message_lines(message, theme, width));
        }
        Some(HistoryItemContent::ModelStep { boundary }) => {
            lines.extend(section(
                "Model step ID",
                &boundary.model_step_id,
                theme,
                width,
            ));
            lines.extend(section(
                "Assistant message",
                &boundary.assistant_message_id,
                theme,
                width,
            ));
        }
        Some(HistoryItemContent::Structured { value }) => {
            lines.extend(fields(value, theme, width));
        }
        _ => lines.extend(absent_copy(detail, row, theme, width)),
    }
    lines
}

fn result_lines(
    detail: Option<&HistoryItemDetail>,
    row: Option<&HistoryRow>,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    if let Some(detail) = detail
        && let Some(lines) = super::tool::tool_call_card(detail, true, theme, width)
    {
        return lines;
    }
    if let Some(HistoryItemContent::Message {
        message:
            piko_protocol::Message::ToolResult {
                tool_name,
                content,
                is_error,
                details,
                ..
            },
        ..
    }) = detail.and_then(|detail| detail.content.as_ref())
    {
        let mut lines = section(
            "Tool result",
            tool_name.as_deref().unwrap_or("tool"),
            theme,
            width,
        );
        if *is_error == Some(true) {
            lines.push(plain("Failed", theme.error, width));
        }
        lines.extend(super::content::block_lines(content, theme, width));
        if let Some(details) = details {
            lines.push(plain("Recorded result details", theme.accent, width));
            lines.extend(fields(details, theme, width));
        }
        return lines;
    }
    if let Some(piko_protocol::TrajectoryRecord::ToolCall(value)) =
        detail.and_then(|detail| detail.diagnostic.as_deref())
    {
        let mut lines = section(
            "Tool observation",
            &format!("{} · {}", value.tool_name, tool_status_word(value.status)),
            theme,
            width,
        );
        if let Some(error) = &value.error {
            lines.extend(section("Error", error, theme, width));
        }
        if let Some(result) = &value.result {
            lines.push(plain("Result", theme.accent, width));
            lines.extend(fields(result, theme, width));
        }
        return lines;
    }
    absent_copy(detail, row, theme, width)
}

fn timing_lines(
    detail: Option<&HistoryItemDetail>,
    row: Option<&HistoryRow>,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let record = detail.and_then(|detail| detail.diagnostic.as_deref());
    match record {
        Some(piko_protocol::TrajectoryRecord::ModelStep(value)) => {
            lines.push(kv(
                "model",
                format!("{} / {}", value.provider, value.model),
                theme,
                width,
            ));
            lines.extend(clock_lines(
                Some(value.started_at),
                value.finished_at,
                value.duration_ms,
                theme,
                width,
            ));
            for retry in &value.retries {
                lines.extend(section(
                    &format!("Retry {} · {} ms", retry.attempt, retry.delay_ms),
                    &retry.error,
                    theme,
                    width,
                ));
            }
            if let Some(fallback) = &value.fallback {
                lines.extend(section(
                    "Fallback",
                    &format!(
                        "{} / {} → {} / {}\n{}",
                        fallback.from_provider,
                        fallback.from_model,
                        fallback.to_provider,
                        fallback.to_model,
                        fallback.reason
                    ),
                    theme,
                    width,
                ));
            }
            lines
        }
        Some(piko_protocol::TrajectoryRecord::ToolCall(value)) => {
            lines.extend(clock_lines(
                Some(value.started_at),
                value.finished_at,
                value.duration_ms,
                theme,
                width,
            ));
            lines
        }
        Some(record) => match record {
            piko_protocol::TrajectoryRecord::Terminal(value) => {
                vec![kv("terminal", terminal_word(value.kind), theme, width)]
            }
            _ => absent_copy(detail, row, theme, width),
        },
        None => vec![plain(
            "diagnostic timing was not recorded",
            theme.muted,
            width,
        )],
    }
}

fn clock_lines(
    started_at: Option<i64>,
    finished_at: Option<i64>,
    duration_ms: Option<u64>,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(start) = started_at {
        lines.push(kv("started", timestamp(start), theme, width));
    } else {
        lines.push(kv("started", "unavailable", theme, width));
    }
    if let Some(finish) = finished_at {
        lines.push(kv("finished", timestamp(finish), theme, width));
    }
    match duration_ms {
        Some(duration) => lines.push(kv("duration", format!("{duration} ms"), theme, width)),
        None => lines.push(kv(
            "duration",
            "diagnostic timing was not recorded",
            theme,
            width,
        )),
    }
    lines
}

fn absent_copy(
    detail: Option<&HistoryItemDetail>,
    row: Option<&HistoryRow>,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    // Summary-only inspection (`i`) has not fetched the row body yet; say so
    // instead of implying the content was never recorded.
    if detail.is_none()
        && matches!(
            row,
            Some(HistoryRow::Stream(item)) if item.has_detail
        )
    {
        return vec![plain(
            "Summary only · open the row with Enter to fetch its recorded content.",
            theme.muted,
            width,
        )];
    }
    vec![plain(
        "No recorded content for this tab.",
        theme.muted,
        width,
    )]
}

fn timestamp(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S %:z")
                .to_string()
        })
        .unwrap_or_else(|| "unavailable".into())
}
