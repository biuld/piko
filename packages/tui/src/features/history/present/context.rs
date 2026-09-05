//! Evidence and relations already available in bounded list summaries.
use super::{
    labels::{lifecycle_label, origin_word, outcome_word},
    paint::wrapped,
};
use crate::{features::history::HistoryRow, theme::Theme};
use piko_protocol::{HistoryAvailability, HistoryRelation};
use ratatui::text::Line;

pub(crate) fn row_context(row: &HistoryRow, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    let mut fields: Vec<(&str, String)> = Vec::new();
    match row {
        HistoryRow::Session(session) => {
            fields.push(("Session", session.session_id.clone()));
            fields.push(("Directory", session.cwd.clone()));
        }
        HistoryRow::Work(work) => {
            fields.push(("Input", work.input_preview.clone()));
            fields.push((
                "Outcome",
                work.outcome
                    .map(outcome_word)
                    .unwrap_or("unavailable")
                    .into(),
            ));
            fields.push(("Agent", work.agent_instance_id.clone()));
            fields.push(("Root input", work.root_input_id.clone()));
            fields.push(("Origin", origin_word(work.origin).into()));
            fields.push((
                "Recorded counts",
                format!(
                    "{} steps · {} tools · {} messages",
                    work.step_count, work.tool_count, work.message_count
                ),
            ));
            fields.push(("Started", timestamp(work.started_at)));
            fields.push(("Finished", timestamp(work.finished_at)));
            if let Some(usage) = &work.usage {
                fields.push((
                    "Effective usage",
                    format!(
                        "{} input · {} output · {} cache read",
                        usage.input, usage.output, usage.cache_read
                    ),
                ));
            }
        }
        HistoryRow::Agent { agent, .. } => {
            fields.push(("Agent", agent.agent_spec_id.clone()));
            fields.push(("Identity", agent.agent_instance_id.clone()));
            fields.push(("Work count", agent.work_count.to_string()));
            fields.push(("Lifecycle", lifecycle_label(agent.lifecycle).into()));
            if let Some(parent) = &agent.parent_agent_instance_id {
                fields.push(("Parent agent", parent.clone()));
            }
            if let Some(origin) = &agent.origin {
                fields.push(("Parent root input", origin.parent_root_input_id.clone()));
                fields.push(("Origin step", origin.origin_model_step_id.clone()));
                fields.push(("Origin tool call", origin.origin_tool_call_id.clone()));
            }
            if let HistoryAvailability::Unavailable { reason } = &agent.origin_availability {
                fields.push(("Origin unavailable", reason.clone()));
            }
        }
        HistoryRow::Item { item, .. } => {
            fields.push(("Summary", item.summary.clone()));
            fields.push((
                "Journal position",
                format!("revision {} · event {}", item.revision, item.event_index),
            ));
            fields.push(("Committed", timestamp(Some(item.committed_at))));
            if let HistoryAvailability::Unavailable { reason } = &item.availability {
                fields.push(("Unavailable", reason.clone()));
            }
            fields.extend(relations(&item.relation));
        }
        HistoryRow::Transcript(item) => {
            fields.push(("Summary", item.summary.clone()));
            fields.push((
                "Branch",
                if item.off_branch {
                    "off branch"
                } else if item.selected {
                    "current position"
                } else {
                    "on branch"
                }
                .into(),
            ));
            for (key, value) in [
                ("Agent", &item.agent_instance_id),
                ("Parent", &item.parent_id),
                ("Root input", &item.root_input_id),
                ("Model step", &item.model_step_id),
            ] {
                if let Some(value) = value {
                    fields.push((key, value.clone()));
                }
            }
        }
        HistoryRow::CommitHeader {
            revision,
            producer,
            events,
            committed_at,
        } => {
            fields.push(("Commit revision", revision.to_string()));
            fields.push(("Producer", producer.clone()));
            fields.push(("Visible events", events.to_string()));
            fields.push(("Committed", timestamp(Some(*committed_at))));
        }
    }
    fields
        .into_iter()
        .flat_map(|(key, value)| wrapped(&format!("{key}  {value}"), theme.text, width))
        .collect()
}

fn relations(relation: &HistoryRelation) -> Vec<(&'static str, String)> {
    [
        ("Agent", &relation.agent_instance_id),
        ("Root input", &relation.root_input_id),
        ("Model step", &relation.model_step_id),
        ("Input ID", &relation.input_id),
        ("Message ID", &relation.message_id),
        ("Tool call", &relation.tool_call_id),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.as_ref().map(|value| (key, value.clone())))
    .collect()
}

fn timestamp(value: Option<i64>) -> String {
    value
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S %:z")
                .to_string()
        })
        .unwrap_or_else(|| "unavailable".into())
}
