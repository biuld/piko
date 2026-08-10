//! Todo, user interaction, environment, and generic tool layouts.

use serde_json::Value;

use crate::ui::components::feedback::{DISCLOSURE_COLLAPSED, SUCCESS_GLYPH};

use super::model::{BodyLine, LineKind, ToolBody, ToolPresentation};
use super::util::{
    PREVIEW_CHARS, body_lines, clip, display_scalar, object_entries, primary_string_preview,
    single_line, str_field,
};

pub(super) fn present_todo(
    name: &str,
    args: Option<&Value>,
    result: Option<&Value>,
) -> ToolPresentation {
    let todos = args
        .and_then(|a| a.get("todos"))
        .or_else(|| result.and_then(|r| r.get("todos")));

    let items = todos.and_then(Value::as_array);
    let (done, active, pending, total) = count_todos(items);

    let title = if total == 0 {
        if name == "todo_read" {
            "empty".into()
        } else {
            "0 items".into()
        }
    } else {
        format!("{done}/{total} done")
    };

    let mut blocks = Vec::new();
    if total > 0 {
        // Compact progress strip on its own meta row.
        blocks.push(BodyLine::meta(
            "progress",
            format!("✓{done}  ~{active}  ·{pending}"),
        ));
        blocks.push(BodyLine::Gap);
    }

    if let Some(items) = items {
        if items.is_empty() {
            blocks.push(BodyLine::dim("(empty todo list)"));
        }
        for item in items {
            let status = str_field(item, "status").unwrap_or("pending");
            // Doing uses disclosure-collapsed ▸ (pair of ▾), not ▶.
            let (kind, mark) = match status {
                "completed" => (LineKind::TodoDone, SUCCESS_GLYPH),
                "in_progress" => (LineKind::TodoActive, DISCLOSURE_COLLAPSED),
                _ => (LineKind::TodoPending, "·"),
            };
            // Content only — no serial id. Paint applies strikethrough to the
            // content span alone (not mark / trailing pad).
            let content = str_field(item, "content").unwrap_or("");
            blocks.push(BodyLine::todo(kind, format!("{mark} {content}")));
        }
    }

    ToolPresentation::with_meta(
        title,
        if blocks.is_empty() {
            ToolBody::Empty
        } else {
            ToolBody::Blocks(blocks)
        },
    )
}

fn count_todos(items: Option<&Vec<Value>>) -> (usize, usize, usize, usize) {
    let Some(items) = items else {
        return (0, 0, 0, 0);
    };
    let mut done = 0usize;
    let mut active = 0usize;
    let mut pending = 0usize;
    for item in items {
        match str_field(item, "status").unwrap_or("pending") {
            "completed" => done += 1,
            "in_progress" => active += 1,
            _ => pending += 1,
        }
    }
    (done, active, pending, items.len())
}

pub(super) fn present_ask_user(args: Option<&Value>, result: Option<&Value>) -> ToolPresentation {
    let question = args
        .and_then(|a| str_field(a, "question"))
        .unwrap_or("question");
    let title = clip(single_line(question), 72);

    let mut blocks = vec![BodyLine::quote(question.to_string())];
    // Multi-line question: quote each line
    if question.contains('\n') {
        blocks = body_lines(question)
            .into_iter()
            .map(BodyLine::quote)
            .collect();
    }

    if let Some(result) = result {
        if let Some(answer) = str_field(result, "answer")
            .or_else(|| str_field(result, "response"))
            .or_else(|| str_field(result, "text"))
        {
            blocks.push(BodyLine::Gap);
            blocks.push(BodyLine::meta("answer", answer));
        } else if let Some(msg) = str_field(result, "message") {
            blocks.push(BodyLine::Gap);
            blocks.push(BodyLine::dim(msg));
        }
    }

    ToolPresentation::with_meta(title, ToolBody::Blocks(blocks))
}

pub(super) fn present_request_user_input(
    args: Option<&Value>,
    result: Option<&Value>,
) -> ToolPresentation {
    let title = args
        .and_then(|a| str_field(a, "title"))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            args.and_then(|a| a.get("questions"))
                .and_then(Value::as_array)
                .and_then(|q| q.first())
                .and_then(|q| {
                    str_field(q, "prompt")
                        .or_else(|| str_field(q, "header"))
                        .map(|s| clip(single_line(s), 64))
                })
        })
        .unwrap_or_else(|| "questions".into());

    let mut blocks = Vec::new();
    if let Some(args) = args {
        if let Some(title) = str_field(args, "title").filter(|s| !s.is_empty()) {
            blocks.push(BodyLine::meta("title", title));
        }
        if let Some(questions) = args.get("questions").and_then(Value::as_array) {
            for (i, q) in questions.iter().enumerate() {
                if i > 0 {
                    blocks.push(BodyLine::Gap);
                }
                let header = str_field(q, "header").unwrap_or("Q");
                let prompt = str_field(q, "prompt").unwrap_or("");
                blocks.push(BodyLine::Text {
                    kind: LineKind::Plain,
                    text: format!("▸ {header}"),
                });
                if !prompt.is_empty() {
                    for line in body_lines(prompt) {
                        blocks.push(BodyLine::quote(line));
                    }
                }
                if let Some(choices) = q.get("choices").and_then(Value::as_array) {
                    for choice in choices {
                        let label = str_field(choice, "label")
                            .or_else(|| str_field(choice, "id"))
                            .unwrap_or("?");
                        blocks.push(BodyLine::dim(format!("  · {label}")));
                    }
                }
            }
        }
    }

    if let Some(result) = result
        && let Ok(pretty) = serde_json::to_string_pretty(result)
    {
        blocks.push(BodyLine::Gap);
        blocks.push(BodyLine::meta("response", ""));
        for line in pretty.lines().take(40) {
            blocks.push(BodyLine::dim(line));
        }
    }

    ToolPresentation::with_meta(
        title,
        if blocks.is_empty() {
            ToolBody::Empty
        } else {
            ToolBody::Blocks(blocks)
        },
    )
}

pub(super) fn present_environment(result: Option<&Value>) -> ToolPresentation {
    let Some(result) = result else {
        return ToolPresentation::with_meta("…", ToolBody::Empty);
    };

    // Prefer a short title from cwd/os.
    let cwd = str_field(result, "cwd")
        .or_else(|| str_field(result, "workingDirectory"))
        .unwrap_or("");
    let os = str_field(result, "os").or_else(|| str_field(result, "platform"));
    let title = if !cwd.is_empty() {
        clip(single_line(cwd), 72)
    } else if let Some(os) = os {
        os.to_string()
    } else {
        "env".into()
    };

    let mut blocks = Vec::new();
    // Curated keys first for scannability.
    const PREFERRED: &[&str] = &[
        "cwd",
        "workingDirectory",
        "os",
        "platform",
        "shell",
        "arch",
        "hostname",
        "user",
        "home",
        "network",
    ];
    let mut seen = std::collections::HashSet::new();
    for key in PREFERRED {
        if let Some(value) = result.get(*key) {
            seen.insert(*key);
            let text = match value {
                Value::String(s) => s.clone(),
                other => display_scalar(other),
            };
            if !text.is_empty() {
                blocks.push(BodyLine::meta(*key, text));
            }
        }
    }
    for (key, value) in object_entries(result) {
        if seen.contains(key) {
            continue;
        }
        match value {
            Value::String(s) if !s.is_empty() && s.len() < 120 && !s.contains('\n') => {
                blocks.push(BodyLine::meta(key, s));
            }
            Value::Bool(_) | Value::Number(_) => {
                blocks.push(BodyLine::meta(key, display_scalar(value)));
            }
            Value::Array(items) if !items.is_empty() && items.len() <= 12 => {
                let joined = items
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                if !joined.is_empty() {
                    blocks.push(BodyLine::meta(key, joined));
                } else {
                    blocks.push(BodyLine::meta(key, format!("[{}]", items.len())));
                }
            }
            Value::Object(map) if !map.is_empty() => {
                blocks.push(BodyLine::meta(key, format!("{{{} keys}}", map.len())));
            }
            _ => {}
        }
    }

    ToolPresentation::with_meta(
        title,
        if blocks.is_empty() {
            ToolBody::Empty
        } else {
            ToolBody::Blocks(blocks)
        },
    )
}

pub(super) fn present_generic(
    name: &str,
    args: Option<&Value>,
    result: Option<&Value>,
    args_raw: &str,
    result_raw: Option<&str>,
) -> ToolPresentation {
    if super::agents::is_agent_tool(name) {
        return present_agent(name, args, result);
    }

    let title = args
        .and_then(primary_string_preview)
        .or_else(|| result.and_then(primary_string_preview))
        .unwrap_or_default();

    let mut blocks = Vec::new();
    if let Some(args) = args {
        blocks.extend(pretty_blocks(args, "args"));
    } else if !args_raw.trim().is_empty() {
        for line in body_lines(args_raw) {
            blocks.push(BodyLine::dim(line));
        }
    }
    if let Some(result) = result {
        if !blocks.is_empty() {
            blocks.push(BodyLine::Gap);
        }
        blocks.extend(pretty_blocks(result, "result"));
    } else if let Some(raw) = result_raw.filter(|t| !t.trim().is_empty()) {
        if !blocks.is_empty() {
            blocks.push(BodyLine::Gap);
        }
        for line in body_lines(raw) {
            blocks.push(BodyLine::plain(line));
        }
    }

    if title.is_empty() {
        ToolPresentation::with_preview(
            clip(name.to_string(), PREVIEW_CHARS),
            if blocks.is_empty() {
                ToolBody::Empty
            } else {
                ToolBody::Blocks(blocks)
            },
        )
    } else {
        ToolPresentation::with_meta(
            title,
            if blocks.is_empty() {
                ToolBody::Empty
            } else {
                ToolBody::Blocks(blocks)
            },
        )
    }
}

fn present_agent(
    name: &str,
    args: Option<&Value>,
    result: Option<&Value>,
) -> ToolPresentation {
    let title = args
        .and_then(|a| super::agents::agent_args_preview(name, a))
        .or_else(|| result.and_then(|r| super::agents::agent_result_preview(name, r)))
        .unwrap_or_else(|| name.to_string());

    let blocks = super::agents::agent_body_lines(name, args, result);
    let body = if blocks.is_empty() {
        ToolBody::Empty
    } else {
        ToolBody::Blocks(blocks)
    };
    ToolPresentation::with_meta(title, body)
}

fn pretty_blocks(value: &Value, _section: &str) -> Vec<BodyLine> {
    match value {
        Value::Object(map) if map.is_empty() => vec![BodyLine::dim("{}")],
        Value::Object(map) => {
            let mut lines = Vec::new();
            for (key, field) in map {
                match field {
                    Value::String(s) if s.contains('\n') || s.chars().count() > 72 => {
                        lines.push(BodyLine::meta(key, ""));
                        for line in body_lines(s) {
                            lines.push(BodyLine::plain(format!("  {line}")));
                        }
                    }
                    Value::Object(_) | Value::Array(_) => {
                        if let Ok(pretty) = serde_json::to_string_pretty(field) {
                            lines.push(BodyLine::meta(key, ""));
                            for line in pretty.lines().take(24) {
                                lines.push(BodyLine::dim(format!("  {line}")));
                            }
                        }
                    }
                    other => {
                        let text = match other {
                            Value::String(s) => s.clone(),
                            v => display_scalar(v),
                        };
                        if !text.is_empty() {
                            lines.push(BodyLine::meta(key, text));
                        }
                    }
                }
            }
            lines
        }
        Value::String(s) => body_lines(s).into_iter().map(BodyLine::plain).collect(),
        other => {
            if let Ok(pretty) = serde_json::to_string_pretty(other) {
                pretty.lines().map(BodyLine::dim).collect()
            } else {
                vec![BodyLine::dim(other.to_string())]
            }
        }
    }
}
