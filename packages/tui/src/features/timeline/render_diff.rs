//! Structured tool-body paint: IDE diffs, code listings, typed body lines.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::theme::Theme;
use crate::ui::line_layout::{filled_line, pad_spans, paint_cols, prefixed_wrap, wrap_spans};

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
            .flat_map(|line| render_body_line(line, theme, card_bg, width))
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
    let language = super::highlight::language_from_path(&diff.path);
    diff.rows
        .iter()
        .flat_map(|row| render_diff_row(row, gutter_w, language, theme, card_bg, width))
        .collect()
}

fn render_diff_row(
    row: &DiffRow,
    gutter_w: usize,
    language: Option<&str>,
    theme: &Theme,
    card_bg: Color,
    width: u16,
) -> Vec<Line<'static>> {
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
            return prefixed_wrap(
                Vec::new(),
                &label,
                Style::default().fg(theme.diff_gutter_fg).bg(card_bg),
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

    let prefix = vec![
        Span::styled(" ", gutter),
        Span::styled(num, gutter),
        Span::styled(format!(" {sign} "), sign_style),
    ];
    let spans = super::highlight::code_line_spans(text, language, theme, text_fg, row_bg);
    prefixed_styled_wrap(prefix, spans, body, width)
}

/// Styled counterpart of `prefixed_wrap`, used when syntax highlighting has
/// already split the code body into token spans.
fn prefixed_styled_wrap(
    prefix: Vec<Span<'static>>,
    spans: Vec<Span<'static>>,
    fill: Style,
    width: u16,
) -> Vec<Line<'static>> {
    let prefix_width = prefix
        .iter()
        .map(|span| paint_cols(span.content.as_ref()))
        .sum::<usize>();
    if prefix_width.saturating_add(1) >= usize::from(width) {
        return vec![pad_spans(prefix, fill, width)];
    }
    let body_width = usize::from(width).saturating_sub(prefix_width);
    let rows = wrap_spans(spans, body_width);
    let rows = if rows.is_empty() {
        vec![Line::default()]
    } else {
        rows
    };
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            let mut output = if index == 0 {
                prefix.clone()
            } else {
                vec![Span::styled(" ".repeat(prefix_width), fill)]
            };
            output.extend(row.spans);
            pad_spans(output, fill, width)
        })
        .collect()
}

fn render_code_view(
    code: &CodeView,
    theme: &Theme,
    card_bg: Color,
    width: u16,
) -> Vec<Line<'static>> {
    let mut lines = super::highlight::code_listing_lines(
        &code.lines.join("\n"),
        code.language.as_deref(),
        code.start_line,
        theme,
        card_bg,
        theme.tool_output,
        theme.diff_gutter_fg,
        width,
        true,
    );
    if let Some(footer) = &code.footer {
        lines.extend(prefixed_wrap(
            Vec::new(),
            footer,
            Style::default().fg(theme.dim).bg(card_bg),
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
) -> Vec<Line<'static>> {
    match line {
        BodyLine::Gap => vec![filled_line("", Style::default().bg(card_bg), width)],
        BodyLine::Meta { key, value } => {
            let key_style = Style::default().fg(theme.dim).bg(card_bg);
            let val_style = Style::default().fg(theme.tool_output).bg(card_bg);
            if value.is_empty() {
                prefixed_wrap(
                    vec![Span::styled(TOOL_BODY_INSET, key_style)],
                    key,
                    key_style,
                    key_style,
                    width,
                )
            } else {
                prefixed_wrap(
                    vec![Span::styled(format!("{TOOL_BODY_INSET}{key}  "), key_style)],
                    value,
                    val_style,
                    val_style,
                    width,
                )
            }
        }
        BodyLine::Text { kind, text } => match kind {
            LineKind::Quote => {
                let style = line_kind_style(*kind, theme, card_bg);
                prefixed_wrap(
                    vec![Span::styled(format!("{TOOL_BODY_INSET}  "), style)],
                    text,
                    style,
                    style,
                    width,
                )
            }
            LineKind::TodoDone | LineKind::TodoActive | LineKind::TodoPending => {
                todo_checklist_line(*kind, text, theme, card_bg, width)
            }
            _ => {
                let style = line_kind_style(*kind, theme, card_bg);
                prefixed_wrap(
                    vec![Span::styled(TOOL_BODY_INSET, style)],
                    text,
                    style,
                    style,
                    width,
                )
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
) -> Vec<Line<'static>> {
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
    let content_part = if content.is_empty() {
        String::new()
    } else {
        content.to_string()
    };
    let mark_part = format!("{TOOL_BODY_INSET}{mark} ");
    prefixed_wrap(
        vec![Span::styled(mark_part, mark_style)],
        &content_part,
        content_style,
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
