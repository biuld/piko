//! Stream body presentation: markdown wrapping, plain-body wrapping, and the
//! timestamp chrome that pins the clock to the first row.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::{
    features::timeline::{
        component::{CustomMessageComponent, ErrorComponent},
        markdown::parse_markdown,
    },
    theme::Theme,
    ui::line_layout::{
        BodyWithTrailing, DEFAULT_EDGE_INSET, DEFAULT_TRAILING_SPACER, body_with_trailing,
        paint_cols, prefixed_wrap, trailing_reserve, wrap_spans,
    },
};

/// Format protocol epoch-ms for message chrome.
///
/// Same calendar day → `HH:MM`; same year → `MM-DD HH:MM`; else full date.
pub(super) fn format_message_timestamp(ts: Option<i64>) -> Option<String> {
    use chrono::Datelike;

    let ms = ts.filter(|n| *n > 0)?;
    let utc = chrono::DateTime::from_timestamp_millis(ms)?;
    let local = utc.with_timezone(&chrono::Local);
    let now = chrono::Local::now();
    let label = if local.date_naive() == now.date_naive() {
        local.format("%H:%M").to_string()
    } else if local.year() == now.year() {
        local.format("%m-%d %H:%M").to_string()
    } else {
        local.format("%Y-%m-%d %H:%M").to_string()
    };
    Some(label)
}

/// Soft-wrapped plain body with leading gutter (thinking / image).
/// No full-width pad here — chrome layer may pad when a trailing clock is present.
pub(super) fn present_plain_body(
    text: &str,
    style: Style,
    width: u16,
    reserve: usize,
) -> Vec<Line<'static>> {
    body_with_trailing(BodyWithTrailing {
        text,
        trailing: None,
        reserve,
        left_style: style,
        trailing_style: Style::default(),
        fill: Style::default(),
        width,
        leading_space: true,
        pad_rows: false,
    })
}

pub(super) fn present_plain_body_unguttered(
    text: &str,
    style: Style,
    width: u16,
    reserve: usize,
) -> Vec<Line<'static>> {
    body_with_trailing(BodyWithTrailing {
        text,
        trailing: None,
        reserve,
        left_style: style,
        trailing_style: Style::default(),
        fill: Style::default(),
        width,
        leading_space: false,
        pad_rows: false,
    })
}

/// Markdown body only — no timestamp / reserve logic.
pub(super) fn present_assistant_markdown(
    text: &str,
    theme: &Theme,
    width: u16,
    reserve: usize,
) -> Vec<Line<'static>> {
    let lead = 1usize;
    let left_max = usize::from(width)
        .saturating_sub(lead)
        .saturating_sub(reserve)
        .max(1);
    let parsed = parse_markdown(text, theme, left_max);
    if parsed.is_empty() {
        return vec![Line::from(Span::from(" "))];
    }
    let mut out = Vec::new();
    for mut line in parsed {
        if line.spans.is_empty() {
            out.push(Line::from(" "));
            continue;
        }
        let wrapped = wrap_spans(std::mem::take(&mut line.spans), left_max);
        if wrapped.is_empty() {
            out.push(Line::from(" "));
            continue;
        }
        for mut row in wrapped {
            // Leading gutter keeps continuation lines aligned under the first.
            row.spans.insert(0, Span::from(" "));
            out.push(row);
        }
    }
    out
}

/// Layout-only: pin trailing label (timestamp) on the first row and keep every
/// row's left budget clear of the right chrome zone. Content is already final.
pub(super) fn apply_message_trailing_chrome(
    lines: Vec<Line<'static>>,
    trailing: Option<&str>,
    width: u16,
    trailing_style: Style,
) -> Vec<Line<'static>> {
    if lines.is_empty() {
        return lines;
    }
    let reserve = trailing_reserve(trailing, DEFAULT_TRAILING_SPACER, DEFAULT_EDGE_INSET);
    if reserve == 0 {
        return lines;
    }

    let target = usize::from(width);
    let edge = DEFAULT_EDGE_INSET;
    let mid = DEFAULT_TRAILING_SPACER;

    lines
        .into_iter()
        .enumerate()
        .map(|(i, mut line)| {
            if i == 0 {
                if let Some(ts) = trailing {
                    let right_w = paint_cols(ts);
                    let left_budget = target
                        .saturating_sub(right_w)
                        .saturating_sub(mid)
                        .saturating_sub(edge);
                    line.spans = truncate_spans_to_cols(line.spans, left_budget);
                    let left_w: usize = line
                        .spans
                        .iter()
                        .map(|s| paint_cols(s.content.as_ref()))
                        .sum();
                    let spacer = target
                        .saturating_sub(left_w)
                        .saturating_sub(right_w)
                        .saturating_sub(edge);
                    if spacer > 0 {
                        line.spans
                            .push(Span::styled(" ".repeat(spacer), Style::default()));
                    }
                    line.spans
                        .push(Span::styled(ts.to_string(), trailing_style));
                    if edge > 0 {
                        line.spans
                            .push(Span::styled(" ".repeat(edge), Style::default()));
                    }
                }
            } else {
                let left_budget = target.saturating_sub(reserve);
                line.spans = truncate_spans_to_cols(line.spans, left_budget);
                let left_w: usize = line
                    .spans
                    .iter()
                    .map(|s| paint_cols(s.content.as_ref()))
                    .sum();
                let pad = target.saturating_sub(left_w);
                if pad > 0 {
                    line.spans
                        .push(Span::styled(" ".repeat(pad), Style::default()));
                }
            }
            line
        })
        .collect()
}

fn truncate_spans_to_cols(spans: Vec<Span<'static>>, max_cols: usize) -> Vec<Span<'static>> {
    if max_cols == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut used = 0usize;
    for span in spans {
        let w = paint_cols(span.content.as_ref());
        if used.saturating_add(w) <= max_cols {
            out.push(span);
            used = used.saturating_add(w);
            continue;
        }
        let room = max_cols.saturating_sub(used);
        if room > 0 {
            let clipped = crate::ui::line_layout::truncate_paint_cols(span.content.as_ref(), room);
            if !clipped.is_empty() {
                out.push(Span::styled(clipped, span.style));
            }
        }
        break;
    }
    out
}

pub(super) fn notice_lines(
    label: &str,
    color: Color,
    text: String,
    width: u16,
) -> Vec<Line<'static>> {
    let prefix = vec![Span::styled(
        format!("{label} "),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )];
    prefixed_wrap(
        prefix,
        &text,
        Style::default().fg(color),
        Style::default(),
        width,
    )
}

pub(super) fn error_lines(
    component: &ErrorComponent,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    let bg = theme.tool_error_bg;
    let body = Style::default().fg(theme.error).bg(bg);
    let mut lines = vec![crate::ui::line_layout::filled_line("", body, width)];
    for line in text_lines(&component.text) {
        lines.extend(prefixed_wrap(
            vec![Span::styled(" Error: ", body)],
            &line,
            body,
            body,
            width,
        ));
    }
    lines.push(crate::ui::line_layout::filled_line("", body, width));
    lines
}

pub(super) fn custom_message_lines(
    component: &CustomMessageComponent,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    let text = match &component.content {
        piko_protocol::CustomMessageContent::String(text) => text.clone(),
        piko_protocol::CustomMessageContent::Blocks(blocks) => blocks
            .iter()
            .map(|block| match block {
                piko_protocol::ContentBlock::Text { text } => text.clone(),
                piko_protocol::ContentBlock::Thinking { thinking, .. } => thinking.clone(),
                piko_protocol::ContentBlock::Image { mime_type, .. } => {
                    format!("[image: {mime_type}]")
                }
                other => other.text_projection(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    };
    notice_lines(&component.custom_type, theme.accent, text, width)
}

fn text_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    text.lines().map(str::to_string).collect()
}
