use piko_protocol::{
    AgentWorkOutcome, HistoryAvailability, HistoryItemContent, HistoryItemDetail,
    HistoryProvenance, TrajectoryRecord, Usage,
};

use super::content::{fields, message_content, message_lines, section, tree_lines};
use super::labels::{
    block_kind_word, origin_word, outcome_color, outcome_word, step_outcome_word, terminal_word,
    tool_status_word,
};
use super::paint::{kv, plain, wrapped};
use crate::theme::Theme;
use ratatui::text::Line;

pub(crate) fn detail_lines(
    detail: &HistoryItemDetail,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    let mut lines = vec![header_line(detail, theme, width)];
    match &detail.availability {
        HistoryAvailability::Unavailable { reason } => {
            lines.push(plain("unavailable", theme.warning, width));
            lines.extend(wrapped(reason, theme.muted, width));
            return lines;
        }
        HistoryAvailability::Available => {}
    }
    lines.push(plain(
        format!("Snapshot revision {}", detail.item_ref.revision),
        theme.muted,
        width,
    ));
    lines.push(plain("", theme.text, width));
    match detail.content.as_ref() {
        Some(HistoryItemContent::Input { input }) => {
            lines.push(plain(origin_word(input.origin), theme.accent_user, width));
            lines.extend(message_content(&input.content, theme, width));
        }
        Some(HistoryItemContent::Message {
            message_id,
            message,
        }) => {
            lines.extend(message_lines(message, theme, width));
            lines.extend(section("Message ID", message_id, theme, width));
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
            lines.extend(section(
                "Ordered tool declarations",
                &boundary.tool_call_message_ids.join("\n"),
                theme,
                width,
            ));
            lines.push(kv(
                "outcome",
                step_outcome_word(boundary.outcome),
                theme,
                width,
            ));
            lines.push(kv(
                "step",
                (boundary.step_index + 1).to_string(),
                theme,
                width,
            ));
            if let Some(clock) = super::labels::format_clock(boundary.started_at) {
                lines.push(kv("started", clock, theme, width));
            }
        }
        Some(HistoryItemContent::Usage { usage }) => {
            lines.extend(usage_lines(usage, theme, width));
        }
        Some(HistoryItemContent::Report { report }) => {
            lines.push(plain(
                outcome_word(report.outcome.status()),
                outcome_color(report.outcome.status(), theme),
                width,
            ));
            if !report.summary.is_empty() {
                lines.extend(wrapped(&report.summary, theme.text, width));
            }
            if let AgentWorkOutcome::Failed { error } = &report.outcome {
                lines.extend(wrapped(error, theme.error, width));
            }
            lines.extend(usage_lines(&report.usage, theme, width));
        }
        Some(HistoryItemContent::TreeEntry { entry }) => {
            lines.extend(tree_lines(entry, theme, width));
        }
        Some(HistoryItemContent::PromptAssembly { assembly }) => {
            lines.push(kv(
                "blocks",
                assembly.prompt.blocks.len().to_string(),
                theme,
                width,
            ));
            lines.push(kv(
                "tools",
                assembly.tool_catalog.tools.len().to_string(),
                theme,
                width,
            ));
            lines.extend(section("Digest", &assembly.prompt_digest, theme, width));
            for block in &assembly.prompt.blocks {
                let label = format!("{} · {}", block_kind_word(block.kind), block.source.locator);
                lines.extend(section(&label, &block.content, theme, width));
                let mut metadata = serde_json::to_value(block).unwrap_or_default();
                if let Some(object) = metadata.as_object_mut() {
                    object.remove("content");
                }
                lines.extend(fields(&metadata, theme, width));
            }
            lines.push(plain("Recorded tool catalog", theme.accent, width));
            lines.extend(fields(
                &serde_json::to_value(&assembly.tool_catalog).unwrap_or_default(),
                theme,
                width,
            ));
        }
        Some(HistoryItemContent::DiagnosticRecord { record }) => {
            lines.extend(diagnostic_lines(record, theme, width));
        }
        Some(HistoryItemContent::Structured { value }) => {
            lines.push(plain("Recorded fields", theme.muted, width));
            lines.extend(fields(value, theme, width));
        }
        None => lines.push(plain(
            "No structured body for this item.",
            theme.muted,
            width,
        )),
    }
    lines
}

fn header_line(detail: &HistoryItemDetail, theme: &Theme, width: u16) -> Line<'static> {
    let provenance = match detail.provenance {
        HistoryProvenance::Fact => "fact",
        HistoryProvenance::Diagnostic => "diagnostic",
    };
    let title = detail.content.as_ref().map(content_kind).unwrap_or("Item");
    plain(format!("{title} · {provenance}"), theme.accent, width)
}

fn content_kind(content: &HistoryItemContent) -> &'static str {
    match content {
        HistoryItemContent::Input { .. } => "Input",
        HistoryItemContent::Message { .. } => "Message",
        HistoryItemContent::ModelStep { .. } => "Model step",
        HistoryItemContent::Usage { .. } => "Usage",
        HistoryItemContent::Report { .. } => "Outcome",
        HistoryItemContent::TreeEntry { .. } => "Transcript",
        HistoryItemContent::PromptAssembly { .. } => "Prompt assembly",
        HistoryItemContent::DiagnosticRecord { .. } => "Diagnostic",
        HistoryItemContent::Structured { .. } => "Item",
    }
}

fn usage_lines(usage: &Usage, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    fields(
        &serde_json::to_value(usage).unwrap_or_default(),
        theme,
        width,
    )
}

fn diagnostic_lines(record: &TrajectoryRecord, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    match record {
        TrajectoryRecord::Assembly(record) => vec![kv(
            "assembly",
            format!(
                "v{} · {} blocks",
                record.assembly_version,
                record.prompt.blocks.len()
            ),
            theme,
            width,
        )],
        TrajectoryRecord::ModelStep(record) => {
            let mut lines = vec![kv(
                "model",
                format!("{} / {}", record.provider, record.model),
                theme,
                width,
            )];
            if let Some(duration) = record.duration_ms {
                lines.push(kv("duration", format!("{duration} ms"), theme, width));
            }
            if let Some(fallback) = &record.fallback {
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
            for retry in &record.retries {
                lines.extend(section(
                    &format!("Retry {} · {} ms", retry.attempt, retry.delay_ms),
                    &retry.error,
                    theme,
                    width,
                ));
            }
            if !record.retries.is_empty() {
                lines.push(kv(
                    "retries",
                    record.retries.len().to_string(),
                    theme,
                    width,
                ));
            }
            for (label, value) in [
                ("Request", Some(&record.request)),
                ("Options", Some(&record.options)),
                ("Response", record.response.as_ref()),
            ] {
                if let Some(value) = value {
                    lines.push(plain(label, theme.accent, width));
                    lines.extend(fields(value, theme, width));
                }
            }
            lines
        }
        TrajectoryRecord::ToolCall(record) => {
            let mut lines = section(
                "Tool observation",
                &format!("{} · {}", record.tool_name, tool_status_word(record.status)),
                theme,
                width,
            );
            if let Some(error) = &record.error {
                lines.extend(section("Error", error, theme, width));
            }
            for (label, value) in [("Arguments", &record.arguments), ("Result", &record.result)] {
                if let Some(value) = value {
                    lines.push(plain(label, theme.accent, width));
                    lines.extend(fields(value, theme, width));
                }
            }
            lines.extend(section("Call ID", &record.call_id, theme, width));
            lines
        }
        TrajectoryRecord::ChildRun(record) => {
            section("Child agent", &record.child_agent_instance_id, theme, width)
        }
        TrajectoryRecord::SystemNotification(record) => {
            vec![plain(record.summary.clone(), theme.text, width)]
        }
        TrajectoryRecord::Terminal(record) => {
            vec![kv("terminal", terminal_word(record.kind), theme, width)]
        }
    }
}
