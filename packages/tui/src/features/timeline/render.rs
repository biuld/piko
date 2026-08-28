use std::time::Instant;

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
        filled_line, paint_cols, prefixed_wrap, trailing_reserve, truncate_cols,
    },
};

use super::{
    AssistantMessageComponent, ContentBlock, SummaryKind, ThoughtComponent, ThoughtPhase, Timeline,
    TimelineComponent, ToolEntry, UpstreamInfo, UserMessageComponent,
    layout::TimelineRenderPlan,
    render_diff::render_tool_body,
    tool_format::{BadgeTone, BodyLine, TitleBadge, ToolBody, ToolPresentation, present_tool},
};

mod body;
use body::{
    apply_message_trailing_chrome, custom_message_lines, error_lines, format_message_timestamp,
    notice_lines, present_assistant_markdown, present_plain_body, present_plain_body_unguttered,
};

impl Timeline {
    #[allow(dead_code)]
    pub fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &Theme,
        interaction: InteractionState<HitId>,
        welcome: WelcomeView<'_>,
    ) {
        let plan = self.render_plan_at(area, theme, interaction.hovered, 0, Instant::now());
        self.render_prepared(frame, area, theme, welcome, plan);
    }

    pub(crate) fn render_prepared(
        &self,
        frame: &mut Frame<'_>,
        _area: Rect,
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
        self.selection.borrow().paint(
            frame,
            plan.content_area,
            plan.top_offset,
            plan.viewport.visible.len(),
            Style::default().bg(theme.bg_visual),
        );
        if let Some(metrics) = plan.viewport.scrollbar {
            let mut scrollbar_state = ScrollbarState::new(metrics.content_rows)
                .position(metrics.content_position())
                .viewport_content_length(metrics.visible_rows.max(1));
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .style(Style::default().fg(theme.border_muted))
                    .thumb_style(Style::default().fg(theme.dim)),
                plan.viewport.gutter,
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
    component_lines_at(
        component,
        thinking_visible,
        hovered,
        theme,
        width,
        0,
        Instant::now(),
    )
}

pub(super) fn component_lines_at(
    component: &TimelineComponent,
    thinking_visible: bool,
    hovered: bool,
    theme: &Theme,
    width: u16,
    spinner_frame: usize,
    now: Instant,
) -> Vec<Line<'static>> {
    match component {
        TimelineComponent::User(component) => user_lines(component, theme, width),
        TimelineComponent::Assistant(component) => {
            assistant_lines(component, thinking_visible, theme, width)
        }
        TimelineComponent::Thought(component) => thought_lines(
            component,
            thinking_visible,
            hovered,
            theme,
            width,
            spinner_frame,
            now,
        ),
        TimelineComponent::Tool(tool) => tool_lines(tool, hovered, theme, width),
        TimelineComponent::SessionFact(component) => notice_lines(
            component.label,
            theme.accent_alt,
            component.text.clone(),
            width,
        ),
        TimelineComponent::Summary(component) => {
            let label = match component.kind {
                SummaryKind::Compaction => "compaction",
                SummaryKind::Branch => "branch summary",
            };
            notice_lines(label, theme.accent, component.text.clone(), width)
        }
        TimelineComponent::CustomMessage(component) => {
            custom_message_lines(component, theme, width)
        }
        TimelineComponent::Error(component) => error_lines(component, theme, width),
    }
}

fn thought_lines(
    component: &ThoughtComponent,
    thinking_visible: bool,
    hovered: bool,
    theme: &Theme,
    width: u16,
    spinner_frame: usize,
    now: Instant,
) -> Vec<Line<'static>> {
    if !thinking_visible || width == 0 {
        return Vec::new();
    }
    let style = Style::default()
        .fg(if hovered {
            theme.accent
        } else {
            theme.thinking_text
        })
        .add_modifier(Modifier::ITALIC)
        .add_modifier(if hovered {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
    let label = match component.phase {
        ThoughtPhase::Streaming { observed_at } => format!(
            "{} thinking... ({})",
            super::THOUGHT_SPINNER[spinner_frame % super::THOUGHT_SPINNER.len()],
            super::format_duration_ms(super::elapsed_ms(observed_at, now)),
        ),
        ThoughtPhase::Completed { duration_ms } => duration_ms
            .map(|duration| {
                format!(
                    "{SUCCESS_GLYPH} thought in {}",
                    super::format_duration_ms(duration)
                )
            })
            .unwrap_or_else(|| format!("{SUCCESS_GLYPH} thought")),
    };
    let content_width = width.saturating_sub(1);
    vec![filled_line(
        format!(" {}", truncate_cols(&label, usize::from(content_width))),
        style,
        width,
    )]
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

fn assistant_lines(
    component: &AssistantMessageComponent,
    thinking_visible: bool,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    // ── Timestamp / right-zone reserve decided up front ────────────────────
    // The clock is pinned to the first body row, so every body block must wrap
    // into a left column that leaves the reserve clear. Computing it here
    // (instead of post-hoc truncation) means long markdown/plain text soft-wraps
    // instead of being clipped by the chrome pass.
    let visible_blocks: Vec<&ContentBlock> = component
        .blocks
        .iter()
        .filter(|block| match block {
            ContentBlock::Text(text) => !text.trim().is_empty(),
            ContentBlock::Thinking(text) => !text.trim().is_empty(),
            ContentBlock::Image { .. } => true,
        })
        .collect();
    let ts = format_message_timestamp(component.timestamp);
    let has_resp_issue = component.error_message.is_some()
        || component
            .stop_reason
            .as_deref()
            .is_some_and(|r| !matches!(r, "stop" | "toolUse" | "tool_use"));
    let show_ts = ts
        .as_deref()
        .filter(|_| !visible_blocks.is_empty() || has_resp_issue);
    let reserve = show_ts
        .map(|_| trailing_reserve(ts.as_deref(), DEFAULT_TRAILING_SPACER, DEFAULT_EDGE_INSET))
        .unwrap_or(0);

    let mut body: Vec<Line<'static>> = Vec::new();
    for (index, block) in visible_blocks.iter().enumerate() {
        match block {
            ContentBlock::Text(text) => {
                body.extend(present_assistant_markdown(
                    text.trim(),
                    theme,
                    width,
                    reserve,
                ));
            }
            ContentBlock::Thinking(text) if thinking_visible => {
                let style = Style::default()
                    .fg(theme.thinking_text)
                    .add_modifier(Modifier::ITALIC);
                body.extend(present_plain_body(text.trim(), style, width, reserve));
            }
            ContentBlock::Thinking(_) => {
                let style = Style::default()
                    .fg(theme.thinking_text)
                    .add_modifier(Modifier::ITALIC);
                body.extend(present_plain_body("Thinking...", style, width, reserve));
            }
            ContentBlock::Image { mime_type } => {
                let style = Style::default().fg(theme.dim);
                let label = format!("[image {mime_type}]");
                body.extend(present_plain_body(&label, style, width, reserve));
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
        body.extend(present_plain_body_unguttered(&message, err, width, reserve));
    }

    // ── Stream chrome: optional clock on first body row ───────────────────
    let dim = Style::default().fg(theme.dim);
    apply_message_trailing_chrome(body, show_ts, width, dim)
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
mod upstream;
use upstream::upstream_presentation;

#[cfg(test)]
#[path = "render_more_tests.rs"]
mod more_tests;
#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "thought_tests.rs"]
mod thought_tests;
#[cfg(test)]
#[path = "render_upstream_tests.rs"]
mod upstream_tests;
