use std::time::Instant;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::{
    theme::Theme,
    ui::{
        components::feedback::{CHIP_SEP, FAIL_GLYPH, SUCCESS_GLYPH, spinner_glyph},
        line_layout::{filled_line, pad_spans, paint_cols, prefixed_wrap},
    },
};

use super::super::{SummaryComponent, SummaryKind, SummaryPhase, elapsed_ms, format_duration_ms};
use super::body::notice_lines;

pub(super) fn summary_lines(
    component: &SummaryComponent,
    theme: &Theme,
    width: u16,
    spinner_frame: usize,
    now: Instant,
) -> Vec<Line<'static>> {
    match component.kind {
        SummaryKind::Compaction => compaction_card(component, theme, width, spinner_frame, now),
        SummaryKind::Branch => notice_lines(
            "branch summary",
            theme.accent,
            component.text.clone(),
            width,
        ),
    }
}

fn compaction_card(
    component: &SummaryComponent,
    theme: &Theme,
    width: u16,
    spinner_frame: usize,
    now: Instant,
) -> Vec<Line<'static>> {
    let (bg, title, right) = match component.phase {
        SummaryPhase::Running { observed_at } => {
            let duration = format_duration_ms(elapsed_ms(observed_at, now));
            let title = if component.new_context_window {
                "Starting new context window"
            } else {
                "Compacting conversation"
            };
            (
                theme.tool_pending_bg,
                format!(" {} {title} ({duration})", spinner_glyph(spinner_frame)),
                None,
            )
        }
        SummaryPhase::Completed => {
            let title = if component.new_context_window {
                "New context window"
            } else {
                "Conversation compacted"
            };
            (
                theme.tool_success_bg,
                format!(" {SUCCESS_GLYPH} {title}"),
                token_chip(component.tokens_before, component.tokens_after),
            )
        }
        SummaryPhase::Failed => (
            theme.tool_error_bg,
            format!(" {FAIL_GLYPH} Compaction failed"),
            None,
        ),
    };
    let title_style = Style::default()
        .fg(theme.tool_title)
        .add_modifier(Modifier::BOLD)
        .bg(bg);
    let body_style = Style::default().fg(theme.tool_output).bg(bg);
    let muted = Style::default().fg(theme.dim).bg(bg);

    let mut lines = vec![
        filled_line("", body_style, width),
        compaction_title_line(&title, right.as_deref(), title_style, muted, width),
    ];
    if !component.text.trim().is_empty() {
        lines.push(filled_line("", body_style, width));
        lines.extend(prefixed_wrap(
            vec![Span::styled(" ", body_style)],
            component.text.trim(),
            body_style,
            body_style,
            width,
        ));
    }
    lines.push(filled_line("", body_style, width));
    lines
}

fn token_chip(before: Option<u64>, after: Option<u64>) -> Option<String> {
    match (before, after) {
        (Some(before), Some(after)) => Some(format!(
            "~{} → ~{}",
            piko_client_core::format_tokens(before),
            piko_client_core::format_tokens(after)
        )),
        (Some(before), None) => Some(format!("~{}", piko_client_core::format_tokens(before))),
        _ => None,
    }
}

fn compaction_title_line(
    left: &str,
    right: Option<&str>,
    title_style: Style,
    muted: Style,
    width: u16,
) -> Line<'static> {
    let target = usize::from(width);
    if target == 0 {
        return Line::from("");
    }
    let right = right.unwrap_or("");
    let right_w = if right.is_empty() {
        0
    } else {
        paint_cols(CHIP_SEP) + paint_cols(right) + 1
    };
    let left_max = target.saturating_sub(right_w);
    let left_text = crate::ui::line_layout::truncate_cols(left, left_max);
    let mut spans = vec![Span::styled(left_text, title_style)];
    if !right.is_empty() {
        let used: usize = spans
            .iter()
            .map(|span| paint_cols(span.content.as_ref()))
            .sum();
        let spacer = target.saturating_sub(used).saturating_sub(right_w);
        if spacer > 0 {
            spans.push(Span::styled(" ".repeat(spacer), title_style));
        }
        spans.push(Span::styled(CHIP_SEP.to_string(), muted));
        spans.push(Span::styled(right.to_string(), muted));
        spans.push(Span::styled(" ", muted));
    }
    pad_spans(spans, title_style, width)
}
