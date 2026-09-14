use piko_protocol::{
    AgentInstanceLifecycle, ModelStepOutcome, TrajectoryTerminalKind, TrajectoryToolCallStatus,
};

use crate::theme::Theme;
use ratatui::style::Color;

/// Role badge for stream rows: inputs and messages carry a role word derived
/// from the durable summary ("user message committed", "input admitted as…").
pub(super) fn format_duration(duration_ms: u64) -> String {
    if duration_ms >= 10_000 {
        format!("{:.1}s", duration_ms as f64 / 1000.0)
    } else {
        format!("{duration_ms}ms")
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
