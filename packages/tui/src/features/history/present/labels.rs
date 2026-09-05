use piko_protocol::{
    AgentInputOrigin, AgentInstanceLifecycle, AgentWorkProcessingStatus, HistoryProvenance,
    ModelStepOutcome, PromptBlockKind, TrajectoryTerminalKind, TrajectoryToolCallStatus, Usage,
};

use crate::theme::Theme;
use ratatui::style::Color;

pub(super) const KIND_COLS: usize = 10;

pub(super) fn kind_label(kind: &str) -> String {
    match kind {
        "input" => "Input".into(),
        "message" => "Message".into(),
        "model_step" => "Step".into(),
        "tool_call" => "Tool".into(),
        "usage" => "Usage".into(),
        "report" => "Outcome".into(),
        "tree_entry" => "Tree".into(),
        "prompt_assembly" => "Prompt".into(),
        "agent_origin" => "Origin".into(),
        "diagnostic" => "Trace".into(),
        "branch_selected" => "Branch".into(),
        other => other
            .split('_')
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

pub(super) fn kind_color(kind: &str, provenance: HistoryProvenance, theme: &Theme) -> Color {
    if provenance == HistoryProvenance::Diagnostic {
        return theme.muted;
    }
    match kind {
        "input" | "message" => theme.accent_user,
        "model_step" => theme.accent_assistant,
        "tool_call" => theme.accent_tool,
        "report" => theme.accent_success,
        "usage" => theme.info,
        "prompt_assembly" | "diagnostic" => theme.muted,
        _ => theme.text_secondary,
    }
}

pub(super) fn pad_kind(label: String) -> String {
    format!("{label:<KIND_COLS$}  ")
}

pub(super) fn origin_word(origin: AgentInputOrigin) -> &'static str {
    match origin {
        AgentInputOrigin::User => "user",
        AgentInputOrigin::Agent => "agent",
        AgentInputOrigin::System => "system",
    }
}

pub(super) fn outcome_word(status: AgentWorkProcessingStatus) -> &'static str {
    match status {
        AgentWorkProcessingStatus::Accepted => "accepted",
        AgentWorkProcessingStatus::Running => "running",
        AgentWorkProcessingStatus::Succeeded => "succeeded",
        AgentWorkProcessingStatus::Failed => "failed",
        AgentWorkProcessingStatus::Cancelled => "cancelled",
    }
}

pub(super) fn outcome_color(status: AgentWorkProcessingStatus, theme: &Theme) -> Color {
    match status {
        AgentWorkProcessingStatus::Succeeded | AgentWorkProcessingStatus::Accepted => theme.success,
        AgentWorkProcessingStatus::Failed => theme.error,
        AgentWorkProcessingStatus::Cancelled => theme.warning,
        AgentWorkProcessingStatus::Running => theme.accent_running,
    }
}

pub(super) fn lifecycle_label(lifecycle: AgentInstanceLifecycle) -> &'static str {
    match lifecycle {
        AgentInstanceLifecycle::Open => "open",
        AgentInstanceLifecycle::Closed => "closed",
        AgentInstanceLifecycle::Terminated => "ended",
        AgentInstanceLifecycle::Unavailable => "unavailable",
    }
}

pub(super) fn lifecycle_color(lifecycle: AgentInstanceLifecycle, theme: &Theme) -> Color {
    match lifecycle {
        AgentInstanceLifecycle::Open => theme.success,
        AgentInstanceLifecycle::Closed => theme.muted,
        AgentInstanceLifecycle::Terminated => theme.warning,
        AgentInstanceLifecycle::Unavailable => theme.error,
    }
}

pub(super) fn step_outcome_word(outcome: ModelStepOutcome) -> &'static str {
    match outcome {
        ModelStepOutcome::Completed => "completed",
        ModelStepOutcome::ToolCalls => "tool calls",
        ModelStepOutcome::Failed => "failed",
        ModelStepOutcome::Cancelled => "cancelled",
    }
}

pub(super) fn tool_status_word(status: TrajectoryToolCallStatus) -> &'static str {
    match status {
        TrajectoryToolCallStatus::Started => "started",
        TrajectoryToolCallStatus::Running => "running",
        TrajectoryToolCallStatus::AwaitingApproval => "awaiting approval",
        TrajectoryToolCallStatus::Completed => "completed",
        TrajectoryToolCallStatus::Failed => "failed",
        TrajectoryToolCallStatus::Cancelled => "cancelled",
    }
}

pub(super) fn terminal_word(kind: TrajectoryTerminalKind) -> &'static str {
    match kind {
        TrajectoryTerminalKind::Completed => "completed",
        TrajectoryTerminalKind::Failed => "failed",
        TrajectoryTerminalKind::Cancelled => "cancelled",
    }
}

pub(super) fn block_kind_word(kind: PromptBlockKind) -> &'static str {
    match kind {
        PromptBlockKind::Instruction => "instruction",
        PromptBlockKind::Context => "context",
        PromptBlockKind::Catalog => "catalog",
        PromptBlockKind::Environment => "environment",
    }
}

pub(super) fn producer_label(producer: &str) -> String {
    producer
        .rsplit([':', '/', '.'])
        .next()
        .unwrap_or(producer)
        .to_string()
}

pub(super) fn format_clock(ms: i64) -> Option<String> {
    use chrono::Datelike;
    if ms <= 0 {
        return None;
    }
    let utc = chrono::DateTime::from_timestamp_millis(ms)?;
    let local = utc.with_timezone(&chrono::Local);
    let now = chrono::Local::now();
    Some(if local.date_naive() == now.date_naive() {
        local.format("%H:%M").to_string()
    } else if local.year() == now.year() {
        local.format("%m-%d %H:%M").to_string()
    } else {
        local.format("%Y-%m-%d").to_string()
    })
}

pub(super) fn compact_usage(usage: &Usage) -> String {
    format!(
        "{}→{}",
        compact_count(usage.input),
        compact_count(usage.output)
    )
}

fn compact_count(n: u64) -> String {
    if n >= 10_000 {
        format!("{}k", n / 1000)
    } else if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}
