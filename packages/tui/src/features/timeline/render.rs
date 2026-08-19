use piko_tui_layout::InteractionState;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use crate::{
    app::{HitId, ToolStatus},
    features::welcome::{self, WelcomeView},
    theme::Theme,
    ui::components::feedback::{
        CANCELLED_GLYPH, CHIP_SEP, DISCLOSURE_COLLAPSED, DISCLOSURE_EXPANDED, FAIL_GLYPH,
        RUNNING_GLYPH, SUCCESS_GLYPH,
    },
    ui::line_layout::{
        BodyWithTrailing, DEFAULT_EDGE_INSET, DEFAULT_TRAILING_SPACER, body_with_trailing,
        filled_line, paint_cols, trailing_reserve, truncate_cols, truncate_paint_cols,
    },
};

use super::{
    AssistantMessageComponent, ContentBlock, CustomMessageComponent, ErrorComponent, SummaryKind,
    Timeline, TimelineComponent, ToolEntry, UserMessageComponent,
    layout::TimelineRenderPlan,
    render_diff::render_tool_body,
    tool_format::{BadgeTone, TitleBadge, ToolBody, present_tool},
};

impl Timeline {
    pub fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &Theme,
        interaction: InteractionState<HitId>,
        welcome: WelcomeView<'_>,
    ) {
        let hovered_tool = match interaction.hovered {
            Some(HitId::TimelineTool(hit_id)) => Some(hit_id),
            _ => None,
        };
        let plan = self.render_plan(area, theme, hovered_tool);
        self.render_prepared(frame, area, theme, welcome, plan);
    }

    pub(crate) fn render_prepared(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &Theme,
        welcome: WelcomeView<'_>,
        mut plan: TimelineRenderPlan,
    ) {
        if plan.lines.is_empty() {
            // Bordered, centered welcome card — not part of the scrollable stream.
            welcome::render(frame, plan.content_area, theme, welcome);
            return;
        }

        let block = if self.viewport.pending_new_items() > 0 {
            // Floating bottom hint: the Dock Stack boundary already paints the
            // single separator under the stream, so the banner must not add its
            // own bottom rule (avoid a duplicate line pair).
            Block::default()
                .title_bottom(format!(" {} new items ", self.viewport.pending_new_items()))
                .title_style(Style::default().fg(theme.warning))
        } else {
            Block::default()
        };
        frame.render_widget(
            Paragraph::new(std::mem::take(&mut plan.lines))
                .scroll((plan.top_offset.min(usize::from(u16::MAX)) as u16, 0))
                .block(block),
            plan.content_area,
        );
        if self.viewport.max_scroll() > 0 {
            let mut scrollbar_state = ScrollbarState::new(self.viewport.content_height())
                .position(self.viewport.scrollbar_position())
                .viewport_content_length(self.viewport.viewport_height());
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .style(Style::default().fg(theme.border_muted))
                    .thumb_style(Style::default().fg(theme.dim)),
                area,
                &mut scrollbar_state,
            );
        }
    }

    #[cfg(test)]
    fn render_lines(&self, theme: &Theme, width: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        // Zero-height components (e.g. tool_use-only assistant with empty body)
        // stay in the timeline for transcript fidelity but must not contribute
        // inter-component gap rows.
        for component in &self.components {
            let body = component_lines(component, self.thinking_visible, false, theme, width);
            if body.is_empty() {
                continue;
            }
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.extend(body);
        }
        lines
    }
}

pub(super) fn component_lines(
    component: &TimelineComponent,
    thinking_visible: bool,
    hovered: bool,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    match component {
        TimelineComponent::User(component) => user_lines(component, theme, width),
        TimelineComponent::Assistant(component) => {
            assistant_lines(component, thinking_visible, theme, width)
        }
        TimelineComponent::Tool(tool) => tool_lines(tool, hovered, theme, width),
        TimelineComponent::SessionFact(component) => {
            notice_lines(component.label, theme.accent_alt, component.text.clone())
        }
        TimelineComponent::Summary(component) => {
            let label = match component.kind {
                SummaryKind::Compaction => "compaction",
                SummaryKind::Branch => "branch summary",
            };
            notice_lines(label, theme.accent, component.text.clone())
        }
        TimelineComponent::CustomMessage(component) => custom_message_lines(component, theme),
        TimelineComponent::Error(component) => error_lines(component, theme, width),
    }
}

fn custom_message_lines(component: &CustomMessageComponent, theme: &Theme) -> Vec<Line<'static>> {
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
    notice_lines(&component.custom_type, theme.accent, text)
}

fn user_lines(component: &UserMessageComponent, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    let bg = theme.user_message_bg;
    let body = Style::default().fg(theme.user_message_text).bg(bg);
    let dim = Style::default().fg(theme.dim).bg(bg);
    let mut lines = vec![filled_line("", body, width)];
    let ts = format_message_timestamp(component.timestamp);
    let reserve = trailing_reserve(ts.as_deref(), DEFAULT_TRAILING_SPACER, DEFAULT_EDGE_INSET);
    lines.extend(body_with_trailing(BodyWithTrailing {
        text: &component.text,
        trailing: ts.as_deref(),
        reserve,
        left_style: body,
        trailing_style: dim,
        fill: body,
        width,
        leading_space: true,
        pad_rows: true,
    }));
    lines.push(filled_line("", body, width));
    lines
}

fn notice_lines(label: &str, color: Color, text: String) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        format!("{label} "),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ))];
    for (index, line) in text_lines(&text).into_iter().enumerate() {
        if index == 0 {
            lines[0]
                .spans
                .push(Span::styled(line, Style::default().fg(color)));
        } else {
            lines.push(Line::from(Span::styled(
                format!("  {line}"),
                Style::default().fg(color),
            )));
        }
    }
    lines
}

fn error_lines(component: &ErrorComponent, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    let bg = theme.tool_error_bg;
    let body = Style::default().fg(theme.error).bg(bg);
    let mut lines = vec![filled_line("", body, width)];
    for line in text_lines(&component.text) {
        lines.push(filled_line(format!(" Error: {line}"), body, width));
    }
    lines.push(filled_line("", body, width));
    lines
}

fn assistant_lines(
    component: &AssistantMessageComponent,
    thinking_visible: bool,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    // ── Content presentation (never depends on timestamp) ─────────────────
    // Timestamp is stream chrome applied after body lines exist. Protocol may
    // omit epoch on drafts; missing clock only means reserve=0, not plain body.
    let visible_blocks: Vec<&ContentBlock> = component
        .blocks
        .iter()
        .filter(|block| match block {
            ContentBlock::Text(text) => !text.trim().is_empty(),
            ContentBlock::Thinking(text) => !text.trim().is_empty(),
            ContentBlock::Image { .. } => true,
        })
        .collect();

    let mut body: Vec<Line<'static>> = Vec::new();
    for (index, block) in visible_blocks.iter().enumerate() {
        match block {
            ContentBlock::Text(text) => {
                body.extend(present_assistant_markdown(text.trim(), theme));
            }
            ContentBlock::Thinking(text) if thinking_visible => {
                let style = Style::default()
                    .fg(theme.thinking_text)
                    .add_modifier(Modifier::ITALIC);
                body.extend(present_plain_body(text.trim(), style, width));
            }
            ContentBlock::Thinking(_) => {
                let style = Style::default()
                    .fg(theme.thinking_text)
                    .add_modifier(Modifier::ITALIC);
                body.extend(present_plain_body("Thinking...", style, width));
            }
            ContentBlock::Image { mime_type } => {
                let style = Style::default().fg(theme.dim);
                let label = format!("[image {mime_type}]");
                body.extend(present_plain_body(&label, style, width));
            }
        }
        let has_visible_content_after = visible_blocks[index + 1..]
            .iter()
            .any(|block| matches!(block, ContentBlock::Text(_) | ContentBlock::Thinking(_)));
        if matches!(block, ContentBlock::Text(_) | ContentBlock::Thinking(_))
            && has_visible_content_after
        {
            body.push(Line::from(""));
        }
    }
    if let Some(stop_reason) = &component.stop_reason
        && !matches!(stop_reason.as_str(), "stop" | "toolUse" | "tool_use")
    {
        if !body.is_empty() {
            body.push(Line::from(""));
        }
        let message = match stop_reason.as_str() {
            "length" => "Error: Model stopped because it reached the maximum output token limit. The response may be incomplete.".to_string(),
            "aborted" => "Operation aborted".to_string(),
            "error" => {
                if let Some(msg) = &component.error_message {
                    format!("Error: {}", msg)
                } else {
                    "Error: Unknown error".to_string()
                }
            }
            other => format!("Error: stopped: {other}"),
        };
        let err = Style::default().fg(theme.error);
        // Error lines keep no leading gutter (historical).
        body.extend(present_plain_body_unguttered(&message, err, width));
    }

    // ── Stream chrome: optional clock on first body row ───────────────────
    let ts = format_message_timestamp(component.timestamp);
    let show_ts = ts.as_deref().filter(|_| {
        !body.is_empty()
            || component.error_message.is_some()
            || component
                .stop_reason
                .as_deref()
                .is_some_and(|r| !matches!(r, "stop" | "toolUse" | "tool_use"))
    });
    let dim = Style::default().fg(theme.dim);
    apply_message_trailing_chrome(body, show_ts, width, dim)
}

/// Markdown body only — no timestamp / reserve logic.
fn present_assistant_markdown(text: &str, theme: &Theme) -> Vec<Line<'static>> {
    let parsed = super::markdown::parse_markdown(text, theme);
    if parsed.is_empty() {
        return vec![Line::from(Span::from(" "))];
    }
    parsed
        .into_iter()
        .map(|mut line| {
            if line.spans.is_empty() {
                line.spans.push(Span::from(" "));
            } else {
                line.spans.insert(0, Span::from(" "));
            }
            line
        })
        .collect()
}

/// Soft-wrapped plain body with leading gutter (thinking / image).
/// No full-width pad here — chrome layer may pad when a trailing clock is present.
fn present_plain_body(text: &str, style: Style, width: u16) -> Vec<Line<'static>> {
    body_with_trailing(BodyWithTrailing {
        text,
        trailing: None,
        reserve: 0,
        left_style: style,
        trailing_style: Style::default(),
        fill: Style::default(),
        width,
        leading_space: true,
        pad_rows: false,
    })
}

fn present_plain_body_unguttered(text: &str, style: Style, width: u16) -> Vec<Line<'static>> {
    body_with_trailing(BodyWithTrailing {
        text,
        trailing: None,
        reserve: 0,
        left_style: style,
        trailing_style: Style::default(),
        fill: Style::default(),
        width,
        leading_space: false,
        pad_rows: false,
    })
}

/// Layout-only: pin trailing label (timestamp) on the first row and keep every
/// row's left budget clear of the right chrome zone. Content is already final.
fn apply_message_trailing_chrome(
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
            let clipped = truncate_paint_cols(span.content.as_ref(), room);
            if !clipped.is_empty() {
                out.push(Span::styled(clipped, span.style));
            }
        }
        break;
    }
    out
}

/// Line index of the title row within a tool card produced by [`tool_lines`]
/// (`pad` then `title` …). Used by hit-testing so only the title toggles.
pub(super) const TOOL_TITLE_ROW_OFFSET: usize = 1;

/// Tool card layout:
/// ```text
///  (pad)
///  ▸ exec_command  $ cargo fmt...        exit 127 | ~1.2k
///  (pad)
/// ```
/// Expanded inserts body between title and bottom pad.
mod tool;
use tool::tool_lines;

fn text_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    text.lines().map(str::to_string).collect()
}

/// Format protocol epoch-ms for message chrome.
///
/// Same calendar day → `HH:MM`; same year → `MM-DD HH:MM`; else full date.
fn format_message_timestamp(ts: Option<i64>) -> Option<String> {
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

#[cfg(test)]
#[path = "render_more_tests.rs"]
mod more_tests;
#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
