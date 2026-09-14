use super::super::HistoryRow;
use super::labels::{format_duration, lifecycle_color, lifecycle_label};
use super::paint::scan_row;
use crate::features::short_id;
use crate::theme::Theme;
use crate::ui::line_layout::{pad_spans, paint_cols, truncate_cols};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

pub(crate) fn empty_copy(agent_choosing: bool) -> &'static str {
    if agent_choosing {
        "No agents recorded in this session."
    } else {
        "No trajectory rows recorded for this agent."
    }
}

pub(crate) fn row_line(
    width: u16,
    selected: bool,
    row: &HistoryRow,
    theme: &Theme,
) -> Line<'static> {
    match row {
        HistoryRow::Session(session) => {
            let name = session.name.as_deref().filter(|name| !name.is_empty());
            scan_row(
                width,
                selected,
                theme,
                vec![
                    (name.unwrap_or("Unnamed session").to_string(), theme.text),
                    (format!("  {}", short_id(&session.session_id)), theme.dim),
                ],
                Some((session.cwd.as_str(), theme.muted)),
            )
        }
        HistoryRow::Agent { agent, depth } => {
            let indent = "  ".repeat((*depth as usize).min(usize::from(width) / 10));
            let right = format!(
                "{} work · {}",
                agent.work_count,
                lifecycle_label(agent.lifecycle)
            );
            scan_row(
                width,
                selected,
                theme,
                vec![
                    (indent, theme.dim),
                    (agent.agent_spec_id.clone(), theme.text),
                    (
                        format!("  {}", lifecycle_label(agent.lifecycle)),
                        lifecycle_color(agent.lifecycle, theme),
                    ),
                ],
                Some((right.as_str(), theme.muted)),
            )
        }
        HistoryRow::Stream(item) => stream_row(width, selected, theme, item),
    }
}

fn stream_row(
    width: u16,
    selected: bool,
    theme: &Theme,
    item: &piko_protocol::HistoryStreamItem,
) -> Line<'static> {
    if item.kind.0 == "model_step" {
        return step_ending_marker(width, selected, theme, item);
    }
    let color = badge_color(&item.badge, theme);
    let mut left = vec![(format!("{:<9}  ", item.badge), color)];
    left.push((item.summary.clone(), theme.text));
    // Duration rides on joined diagnostics; a bare journal revision stays in
    // the detail instead of cluttering every row.
    let right = item.duration_ms.map(format_duration);
    let right = right.as_deref().map(|text| (text, theme.dim));
    scan_row(width, selected, theme, left, right)
}

fn step_ending_marker(
    width: u16,
    selected: bool,
    theme: &Theme,
    item: &piko_protocol::HistoryStreamItem,
) -> Line<'static> {
    let fill = if selected {
        Style::default().bg(theme.bg_selected)
    } else {
        Style::default()
    };
    let marker = if selected { "› " } else { "  " };
    let step = item
        .badge
        .strip_prefix("STEP ")
        .map(|index| format!("Step {index}"))
        .unwrap_or_else(|| "Step".into());
    let duration = item
        .duration_ms
        .map(format_duration)
        .map(|duration| format!(" · {duration}"))
        .unwrap_or_default();
    let available = usize::from(width).saturating_sub(paint_cols(marker));
    let label = truncate_cols(
        &format!("── {step} ended · {}{duration} ", item.summary),
        available,
    );
    let rule = "─".repeat(available.saturating_sub(paint_cols(&label)));
    let label_style = if selected {
        fill.fg(theme.text_secondary).add_modifier(Modifier::BOLD)
    } else {
        fill.fg(theme.text_secondary)
    };
    pad_spans(
        vec![
            Span::styled(
                marker.to_string(),
                fill.fg(if selected { theme.accent } else { theme.dim }),
            ),
            Span::styled(label, label_style),
            Span::styled(rule, fill.fg(theme.border_muted)),
        ],
        fill,
        width,
    )
}

fn badge_color(badge: &str, theme: &Theme) -> ratatui::style::Color {
    match badge {
        "USER" | "CONTEXT" => theme.accent_user,
        "ASSISTANT" => theme.accent_assistant,
        "TOOL" | "RESULT" => theme.accent_tool,
        "AGENT" | "SYSTEM" => theme.info,
        _ => theme.text_secondary,
    }
}
