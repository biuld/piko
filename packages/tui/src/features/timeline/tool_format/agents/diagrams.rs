//! Expanded body layouts for multi-agent tools.
//!
//! Compact meta strip + prose (summary / prompt). No raw JSON dumps, no
//! box-drawing, no duplicated title fields.

use serde_json::Value;

use super::super::model::BodyLine;
use super::super::util::{body_lines, short_id, str_field};

pub(super) fn spawn_args_body(args: &Value, detached: bool) -> Vec<BodyLine> {
    let mut lines = Vec::new();
    if detached {
        lines.push(BodyLine::meta("mode", "detach"));
    }
    if let Some(spec) = str_field(args, "agent_spec_id") {
        lines.push(BodyLine::meta("spec", spec));
    }
    if let Some(prompt) = str_field(args, "prompt").filter(|s| !s.is_empty()) {
        lines.push(BodyLine::Gap);
        for line in body_lines(prompt) {
            lines.push(BodyLine::quote(line));
        }
    }
    lines
}

pub(super) fn spawn_result_body(result: &Value) -> Vec<BodyLine> {
    let mut lines = Vec::new();

    if let Some(id) = str_field(result, "agent_instance_id") {
        lines.push(BodyLine::meta("agent", id));
    }
    if let Some(spec) = str_field(result, "agent_spec_id") {
        lines.push(BodyLine::meta("spec", spec));
    }
    if let Some(status) = str_field(result, "status") {
        lines.push(BodyLine::meta("status", status));
    }
    if let Some(label) = outcome_label(result.get("outcome")) {
        lines.push(BodyLine::meta("outcome", label));
    }
    if let Some(usage) = usage_line(result) {
        lines.push(BodyLine::meta("usage", usage));
    }
    if let Some(artifacts) = result.get("artifacts").and_then(Value::as_array)
        && !artifacts.is_empty()
    {
        lines.push(BodyLine::meta("files", artifacts.len().to_string()));
    }

    if let Some(summary) = str_field(result, "summary").filter(|s| !s.is_empty()) {
        lines.push(BodyLine::Gap);
        for line in body_lines(summary) {
            lines.push(BodyLine::quote(line));
        }
    }
    lines
}

pub(super) fn message_args_body(args: &Value) -> Vec<BodyLine> {
    let mut lines = Vec::new();
    if let Some(id) = str_field(args, "agent_instance_id") {
        lines.push(BodyLine::meta("agent", id));
    }
    if let Some(when) = str_field(args, "when") {
        lines.push(BodyLine::meta("when", when));
    }
    if let Some(message) = str_field(args, "message").filter(|s| !s.is_empty()) {
        lines.push(BodyLine::Gap);
        for line in body_lines(message) {
            lines.push(BodyLine::quote(line));
        }
    }
    lines
}

pub(super) fn message_result_body(result: &Value) -> Vec<BodyLine> {
    let mut lines = Vec::new();
    if let Some(id) = str_field(result, "agent_instance_id") {
        lines.push(BodyLine::meta("agent", id));
    }
    if let Some(when) = str_field(result, "when") {
        lines.push(BodyLine::meta("when", when));
    }
    if let Some(disposition) = str_field(result, "disposition") {
        lines.push(BodyLine::meta("result", disposition));
    }
    lines
}

pub(super) fn lifecycle_args_body(args: &Value, verb: &str) -> Vec<BodyLine> {
    let mut lines = vec![BodyLine::meta("action", verb)];
    if let Some(id) = str_field(args, "agent_instance_id") {
        lines.push(BodyLine::meta("agent", id));
    }
    lines
}

pub(super) fn lifecycle_result_body(name: &str, result: &Value) -> Vec<BodyLine> {
    let mut lines = Vec::new();
    if let Some(id) = str_field(result, "agent_instance_id").or_else(|| str_field(result, "id")) {
        lines.push(BodyLine::meta("agent", id));
    }
    lines.push(BodyLine::meta("action", name.trim_end_matches("_agent")));
    if let Some(activity) =
        str_field(result, "previous_activity").or_else(|| str_field(result, "activity"))
    {
        lines.push(BodyLine::meta("activity", activity));
    }
    if let Some(status) = str_field(result, "status").or_else(|| str_field(result, "lifecycle")) {
        lines.push(BodyLine::meta("status", status));
    }
    for key in ["timeout_ms", "timed_out", "disposition"] {
        if let Some(value) = result.get(key) {
            let text = match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            if !text.is_empty() {
                lines.push(BodyLine::meta(key, text));
            }
        }
    }
    lines
}

pub(super) fn wait_args_body(args: &Value) -> Vec<BodyLine> {
    let mut lines = Vec::new();
    if let Some(timeout) = args.get("timeout_ms") {
        lines.push(BodyLine::meta("timeout", format!("{timeout}ms")));
    }
    if let Some(id) = str_field(args, "agent_instance_id") {
        lines.push(BodyLine::meta("agent", id));
    } else {
        lines.push(BodyLine::meta("agent", "any"));
    }
    lines
}

/// `succeeded` / `failed` / … — never the raw outcome object.
fn outcome_label(outcome: Option<&Value>) -> Option<String> {
    let outcome = outcome?;
    match outcome {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Object(map) => map
            .get("type")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| {
                // Failed { error } / Cancelled { reason }
                if let Some(err) = map.get("error").and_then(Value::as_str) {
                    return Some(format!("failed: {err}"));
                }
                if let Some(reason) = map.get("reason").and_then(Value::as_str) {
                    return Some(format!("cancelled: {reason}"));
                }
                None
            }),
        _ => None,
    }
}

fn usage_line(result: &Value) -> Option<String> {
    let usage = result
        .get("usage")
        .or_else(|| result.get("outcome").and_then(|o| o.get("usage")))?;
    if !usage.is_object() {
        return None;
    }
    let input = u64_field(usage, "input").unwrap_or(0);
    let output = u64_field(usage, "output").unwrap_or(0);
    let total = u64_field(usage, "totalTokens")
        .or_else(|| u64_field(usage, "total_tokens"))
        .unwrap_or(0);
    if total == 0 && input == 0 && output == 0 {
        return None;
    }
    let tok = |n: u64| piko_client_core::format_tokens(n);
    if total > 0 {
        Some(format!("{}  in {}  out {}", tok(total), tok(input), tok(output)))
    } else {
        Some(format!("in {}  out {}", tok(input), tok(output)))
    }
}

fn u64_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_i64().map(|n| n.max(0) as u64))
            .or_else(|| v.as_f64().map(|n| n.max(0.0) as u64))
    })
}

#[allow(dead_code)]
fn _short(id: &str) -> String {
    short_id(id)
}
