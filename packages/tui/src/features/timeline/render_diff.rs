//! Structured tool-body paint: IDE diffs, code listings, typed body lines.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::theme::Theme;
use crate::ui::line_layout::{filled_line, pad_spans};

use super::tool_format::{BodyLine, CodeView, DiffRow, DiffView, LineKind, ToolBody};

pub(super) fn render_tool_body(
    body: &ToolBody,
    theme: &Theme,
    card_bg: Color,
    width: u16,
) -> Vec<Line<'static>> {
    match body {
        ToolBody::Empty => Vec::new(),
        ToolBody::Diff(diff) => render_diff_view(diff, theme, card_bg, width),
        ToolBody::Code(code) => render_code_view(code, theme, card_bg, width),
        ToolBody::Blocks(lines) => lines
            .iter()
            .map(|line| render_body_line(line, theme, card_bg, width))
            .collect(),
    }
}

/// IDE-style inline diff body (path/stats live on the tool title row).
fn render_diff_view(
    diff: &DiffView,
    theme: &Theme,
    card_bg: Color,
    width: u16,
) -> Vec<Line<'static>> {
    let gutter_w = diff.gutter_width();
    diff.rows
        .iter()
        .map(|row| render_diff_row(row, gutter_w, theme, card_bg, width))
        .collect()
}

fn render_diff_row(
    row: &DiffRow,
    gutter_w: usize,
    theme: &Theme,
    card_bg: Color,
    width: u16,
) -> Line<'static> {
    let (line_no, sign, text, row_bg, text_fg, gutter_fg) = match row {
        DiffRow::Context { new_no, text, .. } => (
            Some(*new_no),
            " ",
            text.as_str(),
            card_bg,
            theme.diff_equal_fg,
            theme.diff_gutter_fg,
        ),
        DiffRow::Delete { old_no, text } => (
            Some(*old_no),
            "−",
            text.as_str(),
            theme.diff_delete_bg,
            theme.diff_delete_fg,
            theme.diff_delete_fg,
        ),
        DiffRow::Insert { new_no, text } => (
            Some(*new_no),
            "+",
            text.as_str(),
            theme.diff_insert_bg,
            theme.diff_insert_fg,
            theme.diff_insert_fg,
        ),
        DiffRow::Ellipsis { omitted } => {
            let label = if *omitted == 0 {
                " ···".to_string()
            } else {
                format!(" ··· {omitted} lines")
            };
            return filled_line(
                label,
                Style::default().fg(theme.diff_gutter_fg).bg(card_bg),
                width,
            );
        }
    };

    let num = match line_no {
        Some(n) => format!("{n:>gutter_w$}"),
        None => format!("{:>gutter_w$}", ""),
    };
    let gutter = Style::default().fg(gutter_fg).bg(row_bg);
    let body = Style::default().fg(text_fg).bg(row_bg);
    let sign_style = Style::default().fg(text_fg).bg(row_bg);

    pad_spans(
        vec![
            Span::styled(" ", gutter),
            Span::styled(num, gutter),
            Span::styled(format!(" {sign} "), sign_style),
            Span::styled(text.to_string(), body),
        ],
        body,
        width,
    )
}

fn render_code_view(
    code: &CodeView,
    theme: &Theme,
    card_bg: Color,
    width: u16,
) -> Vec<Line<'static>> {
    let gutter_w = code.gutter_width();
    let gutter = Style::default().fg(theme.diff_gutter_fg).bg(card_bg);
    let body = Style::default().fg(theme.tool_output).bg(card_bg);
    let mut lines = Vec::with_capacity(code.lines.len() + 1);
    for (i, text) in code.lines.iter().enumerate() {
        let no = code.start_line.saturating_add(i);
        let num = format!("{no:>gutter_w$}");
        lines.push(pad_spans(
            vec![
                Span::styled(" ", gutter),
                Span::styled(num, gutter),
                Span::styled(" │ ", gutter),
                Span::styled(text.clone(), body),
            ],
            body,
            width,
        ));
    }
    if let Some(footer) = &code.footer {
        lines.push(filled_line(
            footer.clone(),
            Style::default().fg(theme.dim).bg(card_bg),
            width,
        ));
    }
    lines
}

/// Shared left inset with tool title content (` " ▸ name"` starts with one space).
const TOOL_BODY_INSET: &str = " ";

fn render_body_line(
    line: &BodyLine,
    theme: &Theme,
    card_bg: Color,
    width: u16,
) -> Line<'static> {
    match line {
        BodyLine::Gap => filled_line("", Style::default().bg(card_bg), width),
        BodyLine::Meta { key, value } => {
            let key_style = Style::default().fg(theme.dim).bg(card_bg);
            let val_style = Style::default().fg(theme.tool_output).bg(card_bg);
            if value.is_empty() {
                filled_line(format!("{TOOL_BODY_INSET}{key}"), key_style, width)
            } else {
                pad_spans(
                    vec![
                        Span::styled(format!("{TOOL_BODY_INSET}{key}  "), key_style),
                        Span::styled(value.clone(), val_style),
                    ],
                    val_style,
                    width,
                )
            }
        }
        BodyLine::Text { kind, text } => match kind {
            LineKind::Quote => {
                let style = line_kind_style(*kind, theme, card_bg);
                filled_line(format!("{TOOL_BODY_INSET}  {text}"), style, width)
            }
            LineKind::TodoDone | LineKind::TodoActive | LineKind::TodoPending => {
                todo_checklist_line(*kind, text, theme, card_bg, width)
            }
            _ => {
                let style = line_kind_style(*kind, theme, card_bg);
                filled_line(format!("{TOOL_BODY_INSET}{text}"), style, width)
            }
        },
    }
}

/// Todo row: `mark content`. Strikethrough applies only to the content span
/// (not the mark, not trailing pad spaces).
fn todo_checklist_line(
    kind: LineKind,
    text: &str,
    theme: &Theme,
    card_bg: Color,
    width: u16,
) -> Line<'static> {
    let base = Style::default().bg(card_bg);
    let (mark_style, content_style) = match kind {
        LineKind::TodoDone => (
            base.fg(theme.dim),
            base.fg(theme.dim).add_modifier(Modifier::CROSSED_OUT),
        ),
        LineKind::TodoActive => (
            base.fg(theme.warning).add_modifier(Modifier::BOLD),
            base.fg(theme.warning).add_modifier(Modifier::BOLD),
        ),
        LineKind::TodoPending => (base.fg(theme.dim), base.fg(theme.dim)),
        _ => (base.fg(theme.dim), base.fg(theme.dim)),
    };

    // text is "{mark} {content}" from present_todo.
    let (mark, content) = match text.split_once(' ') {
        Some((m, rest)) => (m, rest),
        None => (text, ""),
    };
    let mark_part = format!("{TOOL_BODY_INSET}{mark}");
    let content_part = if content.is_empty() {
        String::new()
    } else {
        format!(" {content}")
    };
    pad_spans(
        vec![
            Span::styled(mark_part, mark_style),
            Span::styled(content_part, content_style),
        ],
        base.fg(theme.dim),
        width,
    )
}

fn line_kind_style(kind: LineKind, theme: &Theme, card_bg: Color) -> Style {
    let base = Style::default().bg(card_bg);
    match kind {
        LineKind::Plain | LineKind::Terminal => base.fg(theme.tool_output),
        LineKind::Dim => base.fg(theme.dim),
        LineKind::Prompt => base.fg(theme.command).add_modifier(Modifier::BOLD),
        LineKind::Success => base.fg(theme.success),
        LineKind::TodoDone => base.fg(theme.dim).add_modifier(Modifier::CROSSED_OUT),
        LineKind::Error => base.fg(theme.error),
        LineKind::Quote => base.fg(theme.text_secondary),
        LineKind::TodoActive => base.fg(theme.warning).add_modifier(Modifier::BOLD),
        LineKind::TodoPending => base.fg(theme.dim),
    }
}

/// Color plain tool body markers (legacy non-structured fallbacks).
#[allow(dead_code)]
pub(super) fn tool_body_style(
    line: &str,
    default: Style,
    muted: Style,
    theme: &Theme,
    bg: Color,
) -> Style {
    let trimmed = line.trim_start();
    if (trimmed.starts_with('+') && !trimmed.starts_with("+++")) || trimmed == "+" {
        Style::default().fg(theme.diff_insert_fg).bg(bg)
    } else if (trimmed.starts_with('-') && !trimmed.starts_with("---")) || trimmed == "-" {
        Style::default().fg(theme.diff_delete_fg).bg(bg)
    } else if trimmed.starts_with("@@") {
        Style::default().fg(theme.info).bg(bg)
    } else if trimmed.starts_with("---") || trimmed.starts_with("+++") {
        muted
    } else {
        default
    }
}
