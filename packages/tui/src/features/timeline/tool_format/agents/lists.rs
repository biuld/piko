//! List / collect layouts for multi-agent tools — plain rows, no tree art.

use serde_json::Value;

use super::super::model::BodyLine;
use super::super::util::{body_lines, clip, short_id, single_line, str_field};

pub(super) fn list_agents_body(result: &Value) -> Vec<BodyLine> {
    let Some(agents) = result.get("agents").and_then(Value::as_array) else {
        return Vec::new();
    };
    if agents.is_empty() {
        return vec![BodyLine::dim("(no agents)")];
    }

    let mut roots: Vec<&Value> = Vec::new();
    let mut by_parent: Vec<(String, Vec<&Value>)> = Vec::new();
    let ids: Vec<&str> = agents
        .iter()
        .filter_map(|a| str_field(a, "agent_instance_id"))
        .collect();

    for agent in agents {
        match str_field(agent, "parent_agent_instance_id") {
            Some(parent) if ids.contains(&parent) => {
                if let Some(entry) = by_parent.iter_mut().find(|(p, _)| p == parent) {
                    entry.1.push(agent);
                } else {
                    by_parent.push((parent.to_string(), vec![agent]));
                }
            }
            _ => roots.push(agent),
        }
    }
    if roots.is_empty() {
        roots = agents.iter().collect();
        by_parent.clear();
    }

    let mut lines = Vec::new();
    for root in &roots {
        push_agent_node(&mut lines, root, 0, &by_parent);
    }
    lines
}

fn push_agent_node(
    lines: &mut Vec<BodyLine>,
    agent: &Value,
    depth: usize,
    by_parent: &[(String, Vec<&Value>)],
) {
    if depth > 6 {
        lines.push(BodyLine::dim(format!("{}...", "  ".repeat(depth))));
        return;
    }
    let id = str_field(agent, "agent_instance_id").unwrap_or("?");
    let spec = str_field(agent, "agent_spec_id").unwrap_or("-");
    let activity = str_field(agent, "activity").unwrap_or("-");
    let lifecycle = agent
        .get("lifecycle")
        .map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| "-".into());
    let indent = "  ".repeat(depth);
    lines.push(BodyLine::plain(format!(
        "{indent}{id}  [{spec}]  {activity}  {lifecycle}"
    )));

    let children = by_parent
        .iter()
        .find(|(p, _)| p == id)
        .map(|(_, kids)| kids.as_slice())
        .unwrap_or(&[]);
    for child in children {
        push_agent_node(lines, child, depth + 1, by_parent);
    }
}

/// Catalog of spawn templates — compact meta strip per spec, description as quote.
///
/// ```text
///   default  general
///
///   general
///   name     General
///   role     assistant
///
///     Long description…
///
///   coder
///   role     implementer
///     …
/// ```
pub(super) fn list_specs_body(result: &Value) -> Vec<BodyLine> {
    let default_id = str_field(result, "default_spawn_spec_id");
    let mut lines = Vec::new();
    if let Some(default) = default_id {
        lines.push(BodyLine::meta("default", default));
    }

    let Some(specs) = result.get("specs").and_then(Value::as_array) else {
        return lines;
    };
    if specs.is_empty() {
        if lines.is_empty() {
            lines.push(BodyLine::dim("(no specs)"));
        } else {
            lines.push(BodyLine::Gap);
            lines.push(BodyLine::dim("(no specs)"));
        }
        return lines;
    }

    for (i, spec) in specs.iter().enumerate() {
        if i > 0 || default_id.is_some() {
            lines.push(BodyLine::Gap);
        }

        let id = str_field(spec, "id").unwrap_or("?");
        let name = str_field(spec, "name").unwrap_or("");
        let role = str_field(spec, "role").unwrap_or("");
        let is_default = default_id == Some(id);

        // Section head: template id (+ default mark). Keep on one plain row.
        if is_default {
            lines.push(BodyLine::plain(format!("{id}  (default)")));
        } else {
            lines.push(BodyLine::plain(id.to_string()));
        }
        if !name.is_empty() && name != id {
            lines.push(BodyLine::meta("name", name));
        }
        if !role.is_empty() && role != id && role != name {
            lines.push(BodyLine::meta("role", role));
        }
        if let Some(description) = str_field(spec, "description").filter(|s| !s.is_empty()) {
            for line in body_lines(description) {
                lines.push(BodyLine::quote(line));
            }
        }
    }
    lines
}

pub(super) fn collect_reports_body(result: &Value) -> Vec<BodyLine> {
    let reports = result
        .get("reports")
        .or_else(|| result.get("items"))
        .or_else(|| result.get("consumed"))
        .and_then(Value::as_array);
    let Some(reports) = reports else {
        return Vec::new();
    };
    if reports.is_empty() {
        return vec![BodyLine::dim("(no reports)")];
    }
    let mut lines = vec![BodyLine::meta(
        "reports",
        format!(
            "{}{}",
            reports.len(),
            if reports.len() == 1 { "" } else { "s" }
        ),
    )];
    for item in reports {
        let report = item.get("report").unwrap_or(item);
        let id = str_field(report, "agent_instance_id")
            .or_else(|| str_field(item, "agent_instance_id"))
            .map(short_id)
            .unwrap_or_else(|| "?".into());
        let outcome = str_field(report, "outcome")
            .or_else(|| {
                report
                    .get("outcome")
                    .and_then(|o| o.get("type"))
                    .and_then(Value::as_str)
            })
            .or_else(|| str_field(report, "status"))
            .unwrap_or("-");
        let summary = str_field(report, "summary")
            .filter(|s| !s.is_empty())
            .map(|s| clip(single_line(s), 48))
            .unwrap_or_default();
        if summary.is_empty() {
            lines.push(BodyLine::plain(format!("  {id}  {outcome}")));
        } else {
            lines.push(BodyLine::plain(format!("  {id}  {outcome}  {summary}")));
        }
    }
    lines
}
