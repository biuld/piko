//! Workspace file tools: read / write / edit.

use serde_json::Value;

use super::diff::{extract_file_change, present_edit_diff};
use super::model::{BodyLine, CodeView, ToolBody, ToolPresentation};
use super::util::{MAX_BODY_LINES, body_lines, str_field};

fn code_language(path: &str) -> Option<String> {
    super::super::highlight::language_from_path(path).map(str::to_string)
}

pub(super) fn present_read(
    args: Option<&Value>,
    result: Option<&Value>,
    result_raw: Option<&str>,
) -> ToolPresentation {
    let path = args.and_then(|a| str_field(a, "path")).unwrap_or("file");
    let offset = args
        .and_then(|a| a.get("offset"))
        .and_then(Value::as_u64)
        .unwrap_or(1) as usize;
    let limit = args.and_then(|a| a.get("limit")).and_then(Value::as_u64);

    let mut meta = path.to_string();
    if let Some(limit) = limit {
        let end = offset.saturating_add(limit as usize).saturating_sub(1);
        meta = format!("{path}  L{offset}–{end}");
    } else if offset > 1 {
        meta = format!("{path}  L{offset}+");
    }

    // Prefer structured result content.
    if let Some(result) = result {
        if let Some(content) = str_field(result, "content") {
            let lines: Vec<String> = content.lines().map(str::to_string).collect();
            let lines_read = result
                .get("linesRead")
                .and_then(Value::as_u64)
                .map(|n| n as usize)
                .unwrap_or(lines.len());
            let total = result
                .get("totalLines")
                .and_then(Value::as_u64)
                .map(|n| n as usize);
            let mut view_lines = lines;
            if view_lines.len() > MAX_BODY_LINES {
                let omitted = view_lines.len() - MAX_BODY_LINES;
                view_lines.truncate(MAX_BODY_LINES);
                view_lines.push(format!("… ({omitted} more)"));
            }
            let footer = total.map(|t| {
                if lines_read < t {
                    format!("  {lines_read} of {t} lines")
                } else {
                    format!("  {t} lines")
                }
            });
            // Title can include total when known.
            if let Some(t) = total {
                meta = if limit.is_some() || offset > 1 {
                    format!("{meta}  · {t} total")
                } else {
                    format!("{path}  {t} lines")
                };
            }
            return ToolPresentation::with_meta(
                meta,
                ToolBody::Code(CodeView {
                    start_line: offset.max(1),
                    lines: view_lines,
                    language: code_language(path),
                    footer,
                }),
            );
        }
        if let Some(err) = str_field(result, "message") {
            return ToolPresentation::with_meta(path, ToolBody::Blocks(vec![BodyLine::error(err)]));
        }
    }

    if let Some(raw) = result_raw.filter(|t| !t.trim().is_empty()) {
        return ToolPresentation::with_meta(
            meta,
            ToolBody::Code(CodeView {
                start_line: offset.max(1),
                lines: body_lines(raw),
                language: code_language(path),
                footer: None,
            }),
        );
    }

    // Still running — show range intent on title only.
    ToolPresentation::with_meta(meta, ToolBody::Empty)
}

pub(super) fn present_write(
    args: Option<&Value>,
    result: Option<&Value>,
    details: Option<&Value>,
) -> ToolPresentation {
    let path = args
        .and_then(|a| str_field(a, "path"))
        .or_else(|| {
            details
                .and_then(extract_file_change)
                .and_then(|c| str_field(c, "path"))
        })
        .unwrap_or("file");

    // Prefer full-file diff when durable change is present.
    if let Some(change) = details
        .and_then(extract_file_change)
        .or_else(|| result.and_then(extract_file_change))
    {
        let before = str_field(change, "before");
        let after = str_field(change, "after");
        if let (Some(before), Some(after)) = (before, after) {
            let view = super::diff::file_contents_as_diff_view(path, before, after);
            let meta = if view.stats.is_empty() {
                path.to_string()
            } else {
                format!("{path}  {}", view.stats)
            };
            return ToolPresentation::with_meta(meta, ToolBody::Diff(view));
        }
        if before.is_none() && after.is_some() {
            // Create: show as + only code view from after content.
            let content = after.unwrap_or("");
            let lines: Vec<String> = content.lines().map(str::to_string).collect();
            let n = lines.len();
            let mut view_lines = lines;
            if view_lines.len() > MAX_BODY_LINES {
                let omitted = view_lines.len() - MAX_BODY_LINES;
                view_lines.truncate(MAX_BODY_LINES);
                view_lines.push(format!("… ({omitted} more)"));
            }
            return ToolPresentation::with_meta(
                format!("{path}  +{n}"),
                ToolBody::Code(CodeView {
                    start_line: 1,
                    lines: view_lines,
                    language: code_language(path),
                    footer: Some(format!("  created · {n} lines")),
                }),
            );
        }
    }

    // Running / no details: show path + content preview from args.
    let content = args.and_then(|a| str_field(a, "content")).unwrap_or("");
    if content.is_empty() {
        let written = result
            .and_then(|r| r.get("written"))
            .and_then(Value::as_bool)
            == Some(true);
        let meta = if written {
            format!("{path}  ✓")
        } else {
            path.to_string()
        };
        return ToolPresentation::with_meta(meta, ToolBody::Empty);
    }

    let lines: Vec<String> = content.lines().map(str::to_string).collect();
    let n = lines.len();
    let mut view_lines = lines;
    if view_lines.len() > MAX_BODY_LINES {
        let omitted = view_lines.len() - MAX_BODY_LINES;
        view_lines.truncate(MAX_BODY_LINES);
        view_lines.push(format!("… ({omitted} more)"));
    }
    ToolPresentation::with_meta(
        format!("{path}  {n} lines"),
        ToolBody::Code(CodeView {
            start_line: 1,
            lines: view_lines,
            language: code_language(path),
            footer: None,
        }),
    )
}

pub(super) fn present_edit(
    args: Option<&Value>,
    result: Option<&Value>,
    details: Option<&Value>,
) -> ToolPresentation {
    if let Some(diff) = present_edit_diff(args, result, details) {
        let meta = if diff.stats.is_empty() {
            diff.path.clone()
        } else {
            format!("{}  {}", diff.path, diff.stats)
        };
        return ToolPresentation::with_meta(meta, ToolBody::Diff(diff));
    }

    let path = args.and_then(|a| str_field(a, "path")).unwrap_or("file");
    let n = args
        .and_then(|a| a.get("edits"))
        .and_then(Value::as_array)
        .map(|e| e.len())
        .unwrap_or(0);
    let meta = if n == 0 {
        path.to_string()
    } else {
        format!("{path}  {n} edit{}", if n == 1 { "" } else { "s" })
    };
    ToolPresentation::with_meta(meta, ToolBody::Empty)
}
