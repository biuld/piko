//! Evidence and relations already available in bounded list summaries.
use super::labels::lifecycle_label;
use super::paint::{field_lines, plain, wrapped};
use crate::features::history::HistoryRow;
use crate::theme::Theme;
use ratatui::text::Line;

pub(crate) fn row_context(row: &HistoryRow, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    if let HistoryRow::Stream(item) = row {
        let mut lines = vec![plain(
            format!("{} · {}", item.badge, item.kind.0.replace('_', " ")),
            theme.accent,
            width,
        )];
        lines.push(plain("Summary", theme.muted, width));
        lines.extend(wrapped(&item.summary, theme.text, width));
        if let Some(status) = &item.status {
            lines.extend(field_lines("Status", status, theme, width));
        }
        let relations = relations(&item.relation);
        if !relations.is_empty() {
            lines.push(Line::from(""));
            lines.push(plain("Relations", theme.muted, width));
            for (key, value) in relations {
                lines.extend(field_lines(key, value, theme, width));
            }
        }
        return lines;
    }
    let mut fields: Vec<(&str, String)> = Vec::new();
    match row {
        HistoryRow::Session(session) => {
            fields.push(("Session", session.session_id.clone()));
            fields.push(("Directory", session.cwd.clone()));
        }
        HistoryRow::Agent { agent, .. } => {
            fields.push(("Agent", agent.agent_spec_id.clone()));
            fields.push(("Identity", agent.agent_instance_id.clone()));
            fields.push(("Work count", agent.work_count.to_string()));
            fields.push(("Lifecycle", lifecycle_label(agent.lifecycle).into()));
            if let Some(parent) = &agent.parent_agent_instance_id {
                fields.push(("Parent agent", parent.clone()));
            }
        }
        HistoryRow::Stream(_) => unreachable!("stream rows return above"),
    }
    fields
        .into_iter()
        .flat_map(|(key, value)| field_lines(key, value, theme, width))
        .collect()
}

fn relations(relation: &piko_protocol::HistoryRelation) -> Vec<(&'static str, String)> {
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
