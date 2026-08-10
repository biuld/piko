//! Multi-agent tool presentation (spawn, message, list, …).
//!
//! Title meta is plain text; expanded body is typed `BodyLine` rows
//! (meta strip + quote prose), not ASCII diagrams.

mod diagrams;
mod lists;
mod preview;

use serde_json::Value;

use super::model::BodyLine;

pub(super) use preview::{agent_args_preview, agent_result_preview};

pub(super) fn is_agent_tool(name: &str) -> bool {
    matches!(
        name,
        "spawn_agent"
            | "spawn_agent_detached"
            | "message_agent"
            | "close_agent"
            | "reopen_agent"
            | "interrupt_agent"
            | "wait_agent"
            | "list_agents"
            | "list_agent_specs"
            | "collect_agent_reports"
    )
}

pub(super) fn agent_body_lines(
    name: &str,
    args: Option<&Value>,
    result: Option<&Value>,
) -> Vec<BodyLine> {
    // Prefer the result view when present (spawn finished, etc.); args alone
    // for in-flight / detached-accepted cards.
    if let Some(result) = result {
        let mut lines = agent_result_body(name, result);
        // In-flight args only when the result has no prose of its own.
        let has_prose = lines.iter().any(|l| {
            matches!(
                l,
                BodyLine::Text {
                    kind: super::model::LineKind::Quote,
                    ..
                }
            )
        });
        if !has_prose
            && let Some(args) = args
            && matches!(
                name,
                "spawn_agent" | "spawn_agent_detached" | "message_agent"
            )
        {
            let arg_lines = agent_args_body(name, args);
            if !arg_lines.is_empty() {
                if !lines.is_empty() {
                    lines.push(BodyLine::Gap);
                }
                lines.extend(arg_lines);
            }
        }
        return lines;
    }
    args.map(|a| agent_args_body(name, a)).unwrap_or_default()
}

fn agent_args_body(name: &str, args: &Value) -> Vec<BodyLine> {
    match name {
        "spawn_agent" => diagrams::spawn_args_body(args, false),
        "spawn_agent_detached" => diagrams::spawn_args_body(args, true),
        "message_agent" => diagrams::message_args_body(args),
        "close_agent" => diagrams::lifecycle_args_body(args, "close"),
        "reopen_agent" => diagrams::lifecycle_args_body(args, "reopen"),
        "interrupt_agent" => diagrams::lifecycle_args_body(args, "interrupt"),
        "wait_agent" => diagrams::wait_args_body(args),
        _ => Vec::new(),
    }
}

fn agent_result_body(name: &str, result: &Value) -> Vec<BodyLine> {
    match name {
        "spawn_agent" | "spawn_agent_detached" => diagrams::spawn_result_body(result),
        "message_agent" => diagrams::message_result_body(result),
        "list_agents" => lists::list_agents_body(result),
        "list_agent_specs" => lists::list_specs_body(result),
        "collect_agent_reports" => lists::collect_reports_body(result),
        "close_agent" | "reopen_agent" | "interrupt_agent" | "wait_agent" => {
            diagrams::lifecycle_result_body(name, result)
        }
        _ => Vec::new(),
    }
}
