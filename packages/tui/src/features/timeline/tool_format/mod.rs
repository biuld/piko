//! Per-tool presentation for timeline tool blocks.
//!
//! Layout contract:
//! - Title: `▾ name  <title_meta>` — one scannable primary fact
//! - Body: typed blocks (diff, code listing, terminal, checklist, diagram)
//! - Avoid raw JSON dumps; use gutters, glyphs, and hierarchy

mod agents;
mod diff;
mod misc;
mod model;
mod shell;
mod util;
mod workspace;

#[cfg(test)]
mod tests;

use serde_json::Value;

use model::ToolPresentation;
use util::parse_json;

pub use diff::{DiffRow, DiffView};
pub use model::{BadgeTone, BodyLine, CodeView, LineKind, TitleBadge, ToolBody};

/// Serialize a tool JSON value for storage on `ToolEntry` without truncating.
pub fn json_for_entry(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

pub fn present_tool(
    name: &str,
    args: &str,
    result: Option<&str>,
    details: Option<&str>,
) -> ToolPresentation {
    let args_value = parse_json(args);
    let result_value = result.and_then(parse_json);
    let details_value = details.and_then(parse_json);

    match name {
        "read" => workspace::present_read(args_value.as_ref(), result_value.as_ref(), result),
        "write" => {
            workspace::present_write(args_value.as_ref(), result_value.as_ref(), details_value.as_ref())
        }
        "edit" => {
            workspace::present_edit(args_value.as_ref(), result_value.as_ref(), details_value.as_ref())
        }
        "exec_command" => {
            shell::present_exec(args_value.as_ref(), result_value.as_ref(), result)
        }
        "write_stdin" => {
            shell::present_write_stdin(args_value.as_ref(), result_value.as_ref(), result)
        }
        "todo_write" | "todo_read" => {
            misc::present_todo(name, args_value.as_ref(), result_value.as_ref())
        }
        "ask_user" => misc::present_ask_user(args_value.as_ref(), result_value.as_ref()),
        "request_user_input" => {
            misc::present_request_user_input(args_value.as_ref(), result_value.as_ref())
        }
        "environment" => misc::present_environment(result_value.as_ref()),
        _ => misc::present_generic(
            name,
            args_value.as_ref(),
            result_value.as_ref(),
            args,
            result,
        ),
    }
}
