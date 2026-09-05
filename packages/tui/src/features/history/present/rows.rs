use piko_protocol::{
    HistoryAvailability, HistoryItemSummary, HistoryProvenance, HistoryWorkSummary,
};

use super::super::{HistoryLens, HistoryRow};
use super::labels::{
    compact_usage, format_clock, kind_color, kind_label, lifecycle_color, lifecycle_label,
    origin_word, outcome_color, outcome_word, pad_kind, producer_label,
};
use super::paint::scan_row;
use crate::features::short_id;
use crate::theme::Theme;
use ratatui::text::Line;

pub(crate) fn empty_copy(lens: HistoryLens) -> &'static str {
    match lens {
        HistoryLens::Work => "No root work in this session.",
        HistoryLens::Agents => "No agents recorded in this session.",
        HistoryLens::Transcript => "No transcript entries in this session.",
        HistoryLens::Journal => "No journal commits in this session.",
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
        HistoryRow::Work(work) => work_row(width, selected, work, theme),
        HistoryRow::Agent { agent, depth } => {
            let indent = "  ".repeat((*depth as usize).min(usize::from(width) / 10));
            let origin = match (&agent.origin, &agent.origin_availability) {
                (Some(_), _) => "spawned",
                (_, HistoryAvailability::Unavailable { .. }) => "origin unknown",
                _ => "root",
            };
            let right = format!("{} work · {origin}", agent.work_count);
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
        HistoryRow::Item { item, depth } => item_row(width, selected, theme, item, *depth),
        HistoryRow::Transcript(item) => {
            let indent = "  ".repeat((item.depth as usize).min(usize::from(width) / 10));
            let mark = if item.selected { "* " } else { "  " };
            let mut right = Vec::new();
            if item.off_branch {
                right.push("off branch");
            } else if item.selected {
                right.push("current");
            }
            let right = right.join(" · ");
            scan_row(
                width,
                selected,
                theme,
                vec![
                    (indent, theme.dim),
                    (mark.to_string(), theme.accent),
                    (
                        pad_kind(kind_label(&item.kind.0)),
                        kind_color(&item.kind.0, HistoryProvenance::Fact, theme),
                    ),
                    (item.summary.clone(), theme.text),
                ],
                (!right.is_empty()).then_some((right.as_str(), theme.muted)),
            )
        }
        HistoryRow::CommitHeader {
            revision,
            producer,
            events,
            committed_at,
        } => {
            let mut right = format!("{events} events");
            if let Some(clock) = format_clock(*committed_at) {
                right = format!("{right} · {clock}");
            }
            scan_row(
                width,
                selected,
                theme,
                vec![
                    (format!("r{revision}  "), theme.accent),
                    (producer_label(producer), theme.text_secondary),
                ],
                Some((right.as_str(), theme.muted)),
            )
        }
    }
}

fn work_row(width: u16, selected: bool, work: &HistoryWorkSummary, theme: &Theme) -> Line<'static> {
    let status = work.outcome.map(outcome_word).unwrap_or("unknown");
    let status_color = work
        .outcome
        .map(|outcome| outcome_color(outcome, theme))
        .unwrap_or(theme.muted);
    let mut counts = Vec::new();
    if work.step_count > 0 {
        counts.push(format!("{} steps", work.step_count));
    }
    if work.tool_count > 0 {
        counts.push(format!("{} tools", work.tool_count));
    }
    if work.message_count > 0 {
        counts.push(format!("{} msgs", work.message_count));
    }
    if let Some(usage) = &work.usage {
        counts.push(format!("{} tokens", compact_usage(usage)));
    }
    let right = counts.join(" · ");
    scan_row(
        width,
        selected,
        theme,
        vec![
            (format!("{status:<9} "), status_color),
            (format!("{}  ", origin_word(work.origin)), theme.muted),
            (work.input_preview.clone(), theme.text),
        ],
        (!right.is_empty()).then_some((right.as_str(), theme.muted)),
    )
}

fn item_row(
    width: u16,
    selected: bool,
    theme: &Theme,
    item: &HistoryItemSummary,
    depth: u32,
) -> Line<'static> {
    let indent = "  ".repeat((depth as usize).min(usize::from(width) / 10));
    let summary_color = match item.availability {
        HistoryAvailability::Unavailable { .. } => theme.warning,
        HistoryAvailability::Available if item.provenance == HistoryProvenance::Diagnostic => {
            theme.muted
        }
        HistoryAvailability::Available => theme.text,
    };
    let provenance = match item.provenance {
        HistoryProvenance::Fact => "fact",
        HistoryProvenance::Diagnostic => "diag",
    };
    let unavailable = matches!(item.availability, HistoryAvailability::Unavailable { .. });
    let right = match (item.provenance, &item.availability) {
        (_, HistoryAvailability::Unavailable { .. }) => String::new(),
        (HistoryProvenance::Diagnostic, _) => String::new(),
        (HistoryProvenance::Fact, _) => format!("r{}", item.revision),
    };
    scan_row(
        width,
        selected,
        theme,
        vec![
            (
                format!(
                    "{provenance}{} ",
                    if unavailable { " unavailable" } else { "" }
                ),
                summary_color,
            ),
            (indent, theme.dim),
            (
                format!("{} · ", kind_label(&item.kind.0)),
                kind_color(&item.kind.0, item.provenance, theme),
            ),
            (item.summary.clone(), summary_color),
        ],
        Some((right.as_str(), theme.dim)),
    )
}
