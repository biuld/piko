use piko_protocol::{
    AgentWorkOutcome, ContentBlock, HistoryAvailability, HistoryItemContent, HistoryItemDetail,
    HistoryProvenance, Message, MessageContent, SessionTreeEntry, TrajectoryRecord, Usage,
};

use super::labels::{
    block_kind_word, origin_word, outcome_color, outcome_word, step_outcome_word, terminal_word,
    tool_status_word,
};
use super::paint::{kv, plain, wrapped};
use crate::features::short_id;
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
    lines.push(plain("", theme.text, width));
    match detail.content.as_ref() {
        Some(HistoryItemContent::Input { input }) => {
            lines.push(plain(origin_word(input.origin), theme.accent_user, width));
            lines.extend(wrapped(&content_preview(&input.content), theme.text, width));
        }
        Some(HistoryItemContent::Message { message, .. }) => {
            lines.extend(message_lines(message, theme, width));
        }
        Some(HistoryItemContent::ModelStep { boundary }) => {
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
            lines.push(plain(tree_entry_label(entry), theme.text_secondary, width));
            lines.extend(wrapped(&tree_entry_preview(entry), theme.text, width));
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
            lines.push(kv(
                "digest",
                short_id(&assembly.prompt_digest),
                theme,
                width,
            ));
            for block in assembly.prompt.blocks.iter().take(8) {
                let label = format!("{} · {}", block_kind_word(block.kind), block.source.locator);
                lines.push(plain(label, theme.muted, width));
            }
        }
        Some(HistoryItemContent::DiagnosticRecord { record }) => {
            lines.extend(diagnostic_lines(record, theme, width));
        }
        Some(HistoryItemContent::Structured { value }) => {
            lines.push(plain("unrecognized item", theme.muted, width));
            let json = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
            lines.extend(wrapped(&json, theme.dim, width));
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
    vec![kv(
        "tokens",
        format!(
            "{} in · {} out · {} cache",
            usage.input, usage.output, usage.cache_read
        ),
        theme,
        width,
    )]
}

fn message_lines(message: &Message, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    let (role, body, color) = match message {
        Message::User { content, .. } => ("user", content_preview(content), theme.accent_user),
        Message::Assistant { content, .. } => {
            ("assistant", blocks_preview(content), theme.accent_assistant)
        }
        Message::ToolCall {
            name, arguments, ..
        } => ("tool", format!("{name} {arguments}"), theme.accent_tool),
        Message::ToolResult {
            tool_name,
            content,
            is_error,
            ..
        } => {
            let color = if is_error.unwrap_or(false) {
                theme.error
            } else {
                theme.accent_tool
            };
            (
                "result",
                format!(
                    "{} {}",
                    tool_name.as_deref().unwrap_or("tool"),
                    blocks_preview(content)
                ),
                color,
            )
        }
        Message::Context { content, .. } => {
            ("context", content_preview(content), theme.accent_system)
        }
    };
    let mut lines = vec![plain(role, color, width)];
    lines.extend(wrapped(&body, theme.text, width));
    lines
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
            if !record.retries.is_empty() {
                lines.push(kv(
                    "retries",
                    record.retries.len().to_string(),
                    theme,
                    width,
                ));
            }
            lines
        }
        TrajectoryRecord::ToolCall(record) => vec![kv(
            "tool",
            format!("{} · {}", record.tool_name, tool_status_word(record.status)),
            theme,
            width,
        )],
        TrajectoryRecord::ChildRun(record) => vec![kv(
            "child",
            short_id(&record.child_agent_instance_id),
            theme,
            width,
        )],
        TrajectoryRecord::SystemNotification(record) => {
            vec![plain(record.summary.clone(), theme.text, width)]
        }
        TrajectoryRecord::Terminal(record) => {
            vec![kv("terminal", terminal_word(record.kind), theme, width)]
        }
    }
}

fn tree_entry_label(entry: &SessionTreeEntry) -> &'static str {
    match entry {
        SessionTreeEntry::Message(_) => "message",
        SessionTreeEntry::ToolCall(_) => "tool call",
        SessionTreeEntry::ThinkingLevelChange(_) => "thinking level",
        SessionTreeEntry::ModelChange(_) => "model change",
        SessionTreeEntry::ActiveToolsChange(_) => "tools change",
        SessionTreeEntry::Compaction(_) => "compaction",
        SessionTreeEntry::BranchSummary(_) => "branch summary",
        SessionTreeEntry::Custom(_) => "custom",
        SessionTreeEntry::CustomMessage(_) => "custom message",
        SessionTreeEntry::Label(_) => "label",
        SessionTreeEntry::SessionInfo(_) => "session info",
    }
}

fn tree_entry_preview(entry: &SessionTreeEntry) -> String {
    match entry {
        SessionTreeEntry::Message(entry) => message_preview(&entry.message),
        SessionTreeEntry::ToolCall(entry) => entry.tool_name.clone(),
        SessionTreeEntry::Compaction(_) => "compacted branch".into(),
        SessionTreeEntry::Label(entry) => entry.label.clone().unwrap_or_else(|| "label".into()),
        _ => tree_entry_label(entry).into(),
    }
}

fn message_preview(message: &Message) -> String {
    match message {
        Message::User { content, .. } | Message::Context { content, .. } => {
            content_preview(content)
        }
        Message::Assistant { content, .. } => blocks_preview(content),
        Message::ToolCall { name, .. } => name.clone(),
        Message::ToolResult { tool_name, .. } => {
            tool_name.clone().unwrap_or_else(|| "tool result".into())
        }
    }
}

fn content_preview(content: &MessageContent) -> String {
    match content {
        MessageContent::String(text) => text.clone(),
        MessageContent::Blocks(blocks) => blocks_preview(blocks),
    }
}

fn blocks_preview(blocks: &[ContentBlock]) -> String {
    let text = blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            ContentBlock::Thinking { thinking, .. } => Some(thinking.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        format!("{} blocks", blocks.len())
    } else {
        text
    }
}
