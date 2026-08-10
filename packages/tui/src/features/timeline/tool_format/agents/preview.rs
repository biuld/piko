//! Title-row previews for multi-agent tools.
//!
//! **ASCII only** — CJK terminals treat many Unicode symbols as double-width
//! while `unicode-width` counts them as 1, which overfills the title row and
//! clips the right-side token unit (`k` on `~1.2k`).

use serde_json::Value;

use super::super::util::{PREVIEW_CHARS, clip, short_id, single_line, str_field};

pub(in super::super) fn agent_args_preview(name: &str, args: &Value) -> Option<String> {
    let preview = match name {
        "spawn_agent" => {
            let spec = str_field(args, "agent_spec_id").unwrap_or("default");
            let prompt = str_field(args, "prompt").unwrap_or("");
            if prompt.is_empty() {
                spec.to_string()
            } else {
                format!("{spec}: {}", clip(single_line(prompt), 48))
            }
        }
        "spawn_agent_detached" => {
            let spec = str_field(args, "agent_spec_id").unwrap_or("default");
            let prompt = str_field(args, "prompt").unwrap_or("");
            if prompt.is_empty() {
                format!("detach {spec}")
            } else {
                format!("detach {spec}: {}", clip(single_line(prompt), 40))
            }
        }
        "message_agent" => {
            let id = short_id(str_field(args, "agent_instance_id").unwrap_or("agent"));
            let when = str_field(args, "when").unwrap_or("queue");
            let message = str_field(args, "message").unwrap_or("");
            if message.is_empty() {
                format!("{when} -> {id}")
            } else {
                format!("{when} -> {id}: {}", clip(single_line(message), 40))
            }
        }
        "close_agent" => {
            let id = short_id(str_field(args, "agent_instance_id").unwrap_or("agent"));
            format!("close {id}")
        }
        "reopen_agent" => {
            let id = short_id(str_field(args, "agent_instance_id").unwrap_or("agent"));
            format!("reopen {id}")
        }
        "interrupt_agent" => {
            let id = short_id(str_field(args, "agent_instance_id").unwrap_or("agent"));
            format!("interrupt {id}")
        }
        "wait_agent" => {
            let timeout = args
                .get("timeout_ms")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "?".into());
            if let Some(id) = str_field(args, "agent_instance_id") {
                format!("wait {} {timeout}ms", short_id(id))
            } else {
                format!("wait any {timeout}ms")
            }
        }
        "list_agents" | "list_agent_specs" | "collect_agent_reports" => return None,
        _ => return None,
    };
    if preview.trim().is_empty() {
        None
    } else {
        Some(clip(preview, PREVIEW_CHARS))
    }
}

pub(in super::super) fn agent_result_preview(name: &str, result: &Value) -> Option<String> {
    let preview = match name {
        "spawn_agent" | "spawn_agent_detached" => {
            let id = str_field(result, "agent_instance_id").map(short_id);
            let summary = str_field(result, "summary")
                .filter(|s| !s.is_empty())
                .map(|s| clip(single_line(s), 40));
            let status = str_field(result, "status");
            let outcome = result.get("outcome").and_then(|v| match v {
                Value::String(s) => Some(s.as_str()),
                _ => None,
            });
            match (id, summary, status, outcome) {
                (Some(id), Some(summary), _, _) => format!("{id}: {summary}"),
                (Some(id), None, Some(status), _) => format!("{id} {status}"),
                (Some(id), None, None, Some(outcome)) => format!("{id} {outcome}"),
                (Some(id), _, _, _) => id,
                _ => return None,
            }
        }
        "message_agent" => {
            let id = short_id(str_field(result, "agent_instance_id").unwrap_or("agent"));
            let when = str_field(result, "when").unwrap_or("queue");
            let disposition = str_field(result, "disposition").unwrap_or("ok");
            format!("{when} -> {id}: {disposition}")
        }
        "list_agents" => {
            let n = result
                .get("agents")
                .and_then(Value::as_array)
                .map(|a| a.len())
                .unwrap_or(0);
            format!("{n} agent{}", if n == 1 { "" } else { "s" })
        }
        "list_agent_specs" => {
            let n = result
                .get("specs")
                .and_then(Value::as_array)
                .map(|a| a.len())
                .unwrap_or(0);
            format!("{n} spec{}", if n == 1 { "" } else { "s" })
        }
        "collect_agent_reports" => {
            let n = result
                .get("reports")
                .or_else(|| result.get("items"))
                .or_else(|| result.get("consumed"))
                .and_then(Value::as_array)
                .map(|a| a.len())
                .or_else(|| result.get("count").and_then(Value::as_u64).map(|n| n as usize))
                .unwrap_or(0);
            format!("{n} report{}", if n == 1 { "" } else { "s" })
        }
        "close_agent" | "reopen_agent" | "interrupt_agent" | "wait_agent" => {
            match str_field(result, "agent_instance_id") {
                Some(id) => short_id(id),
                None => return None,
            }
        }
        _ => return None,
    };
    if preview.trim().is_empty() {
        None
    } else {
        Some(clip(preview, PREVIEW_CHARS))
    }
}
