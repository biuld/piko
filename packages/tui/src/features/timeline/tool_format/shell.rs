//! Process tools: exec_command / write_stdin — terminal panel layout.
//!
//! Title: `$ cmd` (or session action). Right badge: `exit N` / `running` —
//! command semantics, not tool-protocol success.

use serde_json::Value;

use super::model::{BadgeTone, BodyLine, LineKind, TitleBadge, ToolBody, ToolPresentation};
use super::util::{MAX_BODY_LINES, clip, single_line, str_field};

pub(super) fn present_exec(
    args: Option<&Value>,
    result: Option<&Value>,
    result_raw: Option<&str>,
) -> ToolPresentation {
    let cmd = args.and_then(|a| str_field(a, "cmd")).unwrap_or("");
    let workdir = args.and_then(|a| str_field(a, "workdir"));

    let title_meta = if cmd.is_empty() {
        String::new()
    } else {
        format!("$ {}", single_line(cmd))
    };

    let (badge, mut blocks) = if let Some(result) = result {
        let (badge, blocks) = terminal_body(result, cmd, workdir);
        (Some(badge), blocks)
    } else if let Some(raw) = result_raw.filter(|t| !t.trim().is_empty()) {
        let mut blocks = Vec::new();
        if !cmd.is_empty() {
            blocks.push(BodyLine::prompt(format!("$ {cmd}")));
        }
        for line in raw.lines().take(MAX_BODY_LINES) {
            blocks.push(BodyLine::terminal(line));
        }
        (Some(TitleBadge::new("done", BadgeTone::Neutral)), blocks)
    } else {
        let mut blocks = Vec::new();
        if let Some(wd) = workdir.filter(|s| !s.is_empty()) {
            blocks.push(BodyLine::meta("cwd", wd));
        }
        if !cmd.is_empty() {
            blocks.push(BodyLine::prompt(format!("$ {cmd}")));
        }
        (Some(TitleBadge::new("running", BadgeTone::Running)), blocks)
    };

    if let Some(args) = args {
        append_exec_meta(&mut blocks, args, result);
    }

    let body = if blocks.is_empty() {
        ToolBody::Empty
    } else {
        ToolBody::Blocks(blocks)
    };

    let meta = if title_meta.is_empty() {
        badge
            .as_ref()
            .map(|b| b.text.clone())
            .unwrap_or_else(|| "exec".into())
    } else {
        title_meta
    };

    match badge {
        Some(badge) => ToolPresentation::with_meta_badge(meta, badge, body),
        None => ToolPresentation::with_meta(meta, body),
    }
}

pub(super) fn present_write_stdin(
    args: Option<&Value>,
    result: Option<&Value>,
    result_raw: Option<&str>,
) -> ToolPresentation {
    let session = args
        .and_then(|a| str_field(a, "session_id"))
        .unwrap_or("session");
    let short = session.chars().take(8).collect::<String>();
    let terminate = args
        .and_then(|a| a.get("terminate"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let chars = args.and_then(|a| str_field(a, "chars")).unwrap_or("");

    // ASCII-only title meta: U+2026 `…` / U+2190 `←` are East-Asian Ambiguous
    // (2 cells on many CJK terminals) while unicode-width counts 1 — that
    // mis-measures the title and clips the right token unit.
    let title = if terminate {
        format!("stop {short}")
    } else if !chars.is_empty() {
        format!("-> {short}  {}", clip(single_line(chars), 48))
    } else {
        format!("poll {short}")
    };

    let mut blocks = vec![BodyLine::meta("session", session)];
    if terminate {
        blocks.push(BodyLine::dim("terminate"));
    } else if !chars.is_empty() {
        blocks.push(BodyLine::prompt(format!("-> {chars}")));
    } else {
        blocks.push(BodyLine::dim("poll"));
    }

    let badge = if let Some(result) = result {
        let (badge, mut out) = terminal_body(result, "", None);
        out.retain(|line| {
            !matches!(
                line,
                BodyLine::Text {
                    kind: LineKind::Prompt,
                    ..
                }
            )
        });
        if !out.is_empty() {
            blocks.push(BodyLine::Gap);
            blocks.append(&mut out);
        }
        Some(badge)
    } else if let Some(raw) = result_raw.filter(|t| !t.trim().is_empty()) {
        blocks.push(BodyLine::Gap);
        for line in raw.lines().take(MAX_BODY_LINES) {
            blocks.push(BodyLine::terminal(line));
        }
        Some(TitleBadge::new("done", BadgeTone::Neutral))
    } else {
        Some(TitleBadge::new("running", BadgeTone::Running))
    };

    let body = ToolBody::Blocks(blocks);
    match badge {
        Some(badge) => ToolPresentation::with_meta_badge(title, badge, body),
        None => ToolPresentation::with_meta(title, body),
    }
}

/// Build expanded body + command-outcome badge. Does **not** repeat exit status
/// as a body headline — that lives on the title right zone.
fn terminal_body(result: &Value, cmd: &str, workdir: Option<&str>) -> (TitleBadge, Vec<BodyLine>) {
    let state = str_field(result, "state").unwrap_or("");
    let exit = result.get("exit_code").and_then(Value::as_i64);
    let output = str_field(result, "output").unwrap_or("");
    let session = str_field(result, "session_id");
    let truncated = result
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let duration = format_duration(result.get("wall_time_seconds").and_then(Value::as_f64));
    let badge = match (state, exit) {
        (_, Some(0)) => TitleBadge::new("exit 0", BadgeTone::Success),
        (_, Some(code)) => TitleBadge::new(format!("exit {code}"), BadgeTone::Error),
        ("running", _) => TitleBadge::new("running", BadgeTone::Running),
        ("timed_out", _) => TitleBadge::new("timeout", BadgeTone::Error),
        ("cancelled", _) => TitleBadge::new("cancelled", BadgeTone::Warning),
        ("exited", _) => TitleBadge::new("exited", BadgeTone::Neutral),
        (s, _) if !s.is_empty() => TitleBadge::new(s, BadgeTone::Neutral),
        _ => TitleBadge::new("done", BadgeTone::Neutral),
    }
    .with_duration(duration);

    let mut blocks = Vec::new();
    if let Some(wd) = workdir.filter(|s| !s.is_empty()) {
        blocks.push(BodyLine::meta("cwd", wd));
    }
    if !cmd.is_empty() {
        blocks.push(BodyLine::prompt(format!("$ {cmd}")));
    }
    if let Some(sid) = session.filter(|s| !s.is_empty()) {
        blocks.push(BodyLine::meta("session", sid));
    }

    if !output.is_empty() {
        if !blocks.is_empty() {
            blocks.push(BodyLine::Gap);
        }
        for (n, line) in output.lines().enumerate() {
            if n >= MAX_BODY_LINES {
                let rest = output.lines().count().saturating_sub(MAX_BODY_LINES);
                blocks.push(BodyLine::dim(format!("… ({rest} more lines)")));
                break;
            }
            blocks.push(BodyLine::terminal(line));
        }
        if truncated {
            blocks.push(BodyLine::dim("truncated"));
        }
    }

    (badge, blocks)
}

/// Compact wall-clock for the title right zone.
fn format_duration(secs: Option<f64>) -> Option<String> {
    let secs = secs.filter(|s| *s >= 0.0 && s.is_finite())?;
    if secs < 0.001 {
        return None;
    }
    if secs < 1.0 {
        Some(format!("{:.0}ms", secs * 1000.0))
    } else if secs < 60.0 {
        Some(format!("{secs:.2}s"))
    } else {
        let mins = (secs / 60.0).floor() as u64;
        let rem = secs - (mins as f64 * 60.0);
        Some(format!("{mins}m{rem:04.1}s"))
    }
}

fn append_exec_meta(blocks: &mut Vec<BodyLine>, args: &Value, result: Option<&Value>) {
    let has_cwd = blocks
        .iter()
        .any(|b| matches!(b, BodyLine::Meta { key, .. } if key == "cwd"));
    if !has_cwd && let Some(wd) = str_field(args, "workdir").filter(|s| !s.is_empty()) {
        blocks.insert(0, BodyLine::meta("cwd", wd));
    }
    if result.is_none() {
        if let Some(tty) = args.get("tty").and_then(Value::as_bool)
            && tty
        {
            blocks.push(BodyLine::meta("tty", "yes"));
        }
        if let Some(perm) =
            str_field(args, "sandbox_permissions").filter(|s| !s.is_empty() && *s != "use_default")
        {
            blocks.push(BodyLine::meta("sandbox", perm));
        }
        if let Some(just) = str_field(args, "justification").filter(|s| !s.is_empty()) {
            blocks.push(BodyLine::dim(format!("why  {just}")));
        }
    }
}
