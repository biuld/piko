//! Structured tool-call body for ConversationBlock (F-45).

use piko_client_core::timeline::ToolStatus;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolBodyKind {
    PrettyJson(String),
    KeyRows(Vec<(String, String)>),
    Plain(String),
}

pub fn format_tool_body(
    name: &str,
    status: ToolStatus,
    args: &Value,
    result: Option<&Value>,
    result_text: &str,
    partial_json: Option<&str>,
) -> Vec<(String, ToolBodyKind)> {
    let args = resolved_args(args, partial_json);
    if name == "exec_command" || name == "write_stdin" {
        return format_exec(name, status, &args, result, result_text);
    }
    let mut sections = Vec::new();
    if status == ToolStatus::Running {
        if let Some(partial) = partial_json {
            match serde_json::from_str::<Value>(partial) {
                Ok(value) => push_args(&mut sections, &value),
                Err(_) => {
                    sections.push(("Arguments".into(), ToolBodyKind::Plain(partial.to_string())))
                }
            }
        }
        return sections;
    }
    push_args(&mut sections, &args);
    if let Some(result) = result {
        push_named(&mut sections, "Result", result);
    } else if !result_text.is_empty() {
        sections.push((
            "Result".into(),
            ToolBodyKind::Plain(result_text.to_string()),
        ));
    }
    sections
}

/// One-line preview for a tool header (exec: `$ cmd`). None if unknown.
pub fn tool_primary_line(name: &str, args: &Value, partial_json: Option<&str>) -> Option<String> {
    let args = resolved_args(args, partial_json);
    if name == "exec_command" {
        return command_from_args(&args).map(|cmd| format!("$ {cmd}"));
    }
    None
}

fn format_exec(
    name: &str,
    status: ToolStatus,
    args: &Value,
    result: Option<&Value>,
    result_text: &str,
) -> Vec<(String, ToolBodyKind)> {
    let mut sections = Vec::new();
    if name == "exec_command" {
        if let Some(cmd) = command_from_args(args) {
            sections.push(("Command".into(), ToolBodyKind::Plain(format!("$ {cmd}"))));
        } else {
            push_args(&mut sections, args);
        }
        if let Some(wd) = nonempty_field(args, "workdir") {
            sections.push((
                "Working directory".into(),
                ToolBodyKind::Plain(wd.to_string()),
            ));
        }
    } else {
        push_args(&mut sections, args);
    }
    if status == ToolStatus::Running && sections.is_empty() {
        return sections;
    }
    if let Some(result) = result {
        if let Some(output) = result.get("output").and_then(Value::as_str) {
            if !output.is_empty() {
                sections.push(("Output".into(), ToolBodyKind::Plain(output.to_string())));
            }
        } else {
            push_named(&mut sections, "Result", result);
        }
    } else if !result_text.is_empty() {
        sections.push((
            "Output".into(),
            ToolBodyKind::Plain(result_text.to_string()),
        ));
    }
    sections
}

fn resolved_args(args: &Value, partial_json: Option<&str>) -> Value {
    let source = if args.is_null() {
        partial_json
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or(Value::Null)
    } else {
        args.clone()
    };
    coerce_args(source)
}

fn coerce_args(value: Value) -> Value {
    match value {
        Value::String(raw) => serde_json::from_str(&raw).unwrap_or(Value::String(raw)),
        other => other,
    }
}

fn command_from_args(args: &Value) -> Option<String> {
    for key in ["cmd", "command"] {
        if let Some(text) = args
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            return Some(text.to_string());
        }
        if let Some(parts) = args.get(key).and_then(Value::as_array) {
            let joined = parts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            if !joined.is_empty() {
                return Some(joined);
            }
        }
    }
    None
}

fn push_args(sections: &mut Vec<(String, ToolBodyKind)>, args: &Value) {
    push_named(sections, "Arguments", args);
}

fn push_named(sections: &mut Vec<(String, ToolBodyKind)>, heading: &str, value: &Value) {
    if value.is_null() || value.as_object().is_some_and(serde_json::Map::is_empty) {
        return;
    }
    sections.push((heading.to_string(), object_or_pretty(value)));
}

fn nonempty_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

fn object_or_pretty(value: &Value) -> ToolBodyKind {
    if let Some(map) = value.as_object()
        && map.len() <= 12
    {
        return ToolBodyKind::KeyRows(
            map.iter()
                .map(|(k, v)| (k.clone(), display_value(v)))
                .collect(),
        );
    }
    match value {
        Value::String(text) => ToolBodyKind::Plain(text.clone()),
        Value::Null => ToolBodyKind::Plain(String::new()),
        other => ToolBodyKind::PrettyJson(pretty(other)),
    }
}

fn display_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => pretty(other),
    }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;



    #[test]
    fn running_without_partial_is_empty() {
        let sections = format_tool_body("read", ToolStatus::Running, &json!({}), None, "", None);
        assert!(sections.is_empty());
    }

    #[test]
    fn small_object_is_key_rows() {
        let kind = object_or_pretty(&json!({"path": "a.rs"}));
        assert!(
            matches!(kind, ToolBodyKind::KeyRows(rows) if rows[0] == ("path".into(), "a.rs".into()))
        );
    }

    #[test]
    fn arrays_are_pretty_json() {
        assert!(matches!(
            object_or_pretty(&json!([1, 2, 3])),
            ToolBodyKind::PrettyJson(_)
        ));
    }

    #[test]
    fn null_args_are_omitted() {
        let sections = format_tool_body(
            "read",
            ToolStatus::Completed,
            &Value::Null,
            Some(&json!({"ok": true})),
            "",
            None,
        );
        assert!(sections.iter().all(|(h, _)| h != "Arguments"));
    }

    #[test]
    fn exec_shows_command_and_output_not_json_null() {
        let sections = format_tool_body(
            "exec_command",
            ToolStatus::Completed,
            &json!({"cmd": "rg foo"}),
            Some(&json!({
                "exit_code": 127,
                "output": "fish: Unknown command: rg\n",
                "state": "exited"
            })),
            "",
            None,
        );
        assert_eq!(
            sections[0],
            ("Command".into(), ToolBodyKind::Plain("$ rg foo".into()))
        );
        assert!(
            matches!(&sections[1], (h, ToolBodyKind::Plain(body)) if h == "Output" && body.contains("Unknown command"))
        );
        assert!(sections.iter().all(|(h, _)| h != "Arguments"));
    }

    #[test]
    fn exec_json_string_args_show_command() {
        let args = Value::String(r#"{"cmd":"rg foo"}"#.into());
        assert_eq!(
            tool_primary_line("exec_command", &args, None).as_deref(),
            Some("$ rg foo")
        );
        let sections = format_tool_body(
            "exec_command",
            ToolStatus::Completed,
            &args,
            Some(&json!({"output": "ok", "exit_code": 0})),
            "",
            None,
        );
        assert_eq!(
            sections[0],
            ("Command".into(), ToolBodyKind::Plain("$ rg foo".into()))
        );
    }

    #[test]
    fn exec_null_args_do_not_render_json_fence() {
        let sections = format_tool_body(
            "exec_command",
            ToolStatus::Completed,
            &Value::Null,
            Some(&json!({"output": "ok", "exit_code": 0})),
            "",
            None,
        );
        assert!(sections.iter().all(|(h, k)| {
            h != "Arguments" && !matches!(k, ToolBodyKind::PrettyJson(s) if s == "null")
        }));
        assert_eq!(
            sections,
            vec![("Output".into(), ToolBodyKind::Plain("ok".into()))]
        );
    }
}
