use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use crate::{
    app::ToolStatus,
    features::{preview_text, short_id},
    layout::DEFAULT_HORIZONTAL_INSET,
    theme::Theme,
};

use super::{
    AssistantMessageComponent, ContentBlock, ErrorComponent, NoticeColor, Timeline,
    TimelineComponent, ToolEntry, UserMessageComponent,
};

impl Timeline {
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        // Layout: [left inset][content][scrollbar]. The scrollbar column is the
        // right gutter — do not inset again inside the content band.
        let content_band = Rect {
            x: area.x,
            y: area.y,
            width: area.width.saturating_sub(1).max(1),
            height: area.height,
        };
        let left = DEFAULT_HORIZONTAL_INSET.min(content_band.width.saturating_sub(1));
        let content_area = Rect {
            x: content_band.x.saturating_add(left),
            y: content_band.y,
            width: content_band.width.saturating_sub(left),
            height: content_band.height,
        };
        let mut lines = self.render_lines(theme, content_area.width);
        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "Type a prompt and press Enter.",
                Style::default().fg(theme.dim),
            )));
        }

        let has_pending = self.viewport.pending_new_items() > 0;
        let visible_height =
            usize::from(content_area.height.saturating_sub(u16::from(has_pending))).max(1);
        self.viewport.set_metrics(lines.len(), visible_height);
        let top_offset = self.viewport.top_offset();

        let block = if self.viewport.pending_new_items() > 0 {
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme.border))
                .title(format!(" {} new items ", self.viewport.pending_new_items()))
                .title_style(Style::default().fg(theme.warning))
        } else {
            Block::default().borders(Borders::empty())
        };
        frame.render_widget(
            Paragraph::new(std::mem::take(&mut lines))
                .scroll((top_offset.min(usize::from(u16::MAX)) as u16, 0))
                .block(block),
            content_area,
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

    fn render_lines(&self, theme: &Theme, width: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        for (index, component) in self.components.iter().enumerate() {
            if index > 0 {
                lines.push(Line::from(""));
            }
            lines.extend(component_lines(
                component,
                self.tools_expanded,
                self.thinking_visible,
                theme,
                width,
            ));
        }
        lines
    }
}

fn component_lines(
    component: &TimelineComponent,
    tools_expanded: bool,
    thinking_visible: bool,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    match component {
        TimelineComponent::User(component) => user_lines(component, theme, width),
        TimelineComponent::Assistant(component) => {
            assistant_lines(component, thinking_visible, theme)
        }
        TimelineComponent::Tool(tool) => tool_lines(tool, tools_expanded, theme, width),
        TimelineComponent::Notice(component) => {
            let color = match component.color {
                NoticeColor::System => theme.accent,
                NoticeColor::Session => theme.accent_alt,
            };
            notice_lines(component.label, color, component.text.clone())
        }
        TimelineComponent::Error(component) => error_lines(component, theme, width),
    }
}

fn user_lines(component: &UserMessageComponent, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    let bg = theme.get("userMessageBg");
    let body = Style::default().fg(theme.get("userMessageText")).bg(bg);
    let mut lines = vec![filled_line("", body, width)];
    for line in text_lines(&component.text) {
        lines.push(filled_line(format!(" {line}"), body, width));
    }
    lines.push(filled_line("", body, width));
    lines
}

fn notice_lines(label: &'static str, color: Color, text: String) -> Vec<Line<'static>> {
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
    let bg = theme.get("toolErrorBg");
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
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let visible_blocks: Vec<&ContentBlock> = component
        .blocks
        .iter()
        .filter(|block| match block {
            ContentBlock::Text(text) => !text.trim().is_empty(),
            ContentBlock::Thinking(text) => !text.trim().is_empty(),
            ContentBlock::Image { .. } => true,
        })
        .collect();
    for (index, block) in visible_blocks.iter().enumerate() {
        match block {
            ContentBlock::Text(text) => {
                let parsed = super::markdown::parse_markdown(text.trim(), theme);
                for mut line in parsed {
                    if line.spans.is_empty() {
                        line.spans.push(Span::from(" "));
                    } else {
                        line.spans.insert(0, Span::from(" "));
                    }
                    lines.push(line);
                }
            }
            ContentBlock::Thinking(text) if thinking_visible => {
                for line in text_lines(text.trim()) {
                    lines.push(Line::from(Span::styled(
                        format!(" {line}"),
                        Style::default()
                            .fg(theme.get("thinkingText"))
                            .add_modifier(Modifier::ITALIC),
                    )));
                }
            }
            ContentBlock::Thinking(_) => {
                lines.push(Line::from(Span::styled(
                    " Thinking...",
                    Style::default()
                        .fg(theme.get("thinkingText"))
                        .add_modifier(Modifier::ITALIC),
                )));
            }
            ContentBlock::Image { mime_type } => lines.push(Line::from(Span::styled(
                format!("  [image {mime_type}]"),
                Style::default().fg(theme.dim),
            ))),
        }
        let has_visible_content_after = visible_blocks[index + 1..]
            .iter()
            .any(|block| matches!(block, ContentBlock::Text(_) | ContentBlock::Thinking(_)));
        if matches!(block, ContentBlock::Text(_) | ContentBlock::Thinking(_))
            && has_visible_content_after
        {
            lines.push(Line::from(""));
        }
    }
    if let Some(stop_reason) = &component.stop_reason
        && !matches!(stop_reason.as_str(), "stop" | "toolUse" | "tool_use")
    {
        if !lines.is_empty() {
            lines.push(Line::from(""));
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
        lines.push(Line::from(Span::styled(
            message,
            Style::default().fg(theme.error),
        )));
    }
    lines
}

fn tool_lines(
    tool: &ToolEntry,
    tools_expanded: bool,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    let bg = match tool.status {
        ToolStatus::Running => theme.get("toolPendingBg"),
        ToolStatus::Completed => theme.get("toolSuccessBg"),
        ToolStatus::Failed => theme.get("toolErrorBg"),
    };
    let title_style = Style::default()
        .fg(theme.get("toolTitle"))
        .add_modifier(Modifier::BOLD)
        .bg(bg);
    let output_style = Style::default().fg(theme.get("toolOutput")).bg(bg);
    let muted_style = Style::default().fg(theme.dim).bg(bg);
    let mut lines = vec![
        filled_line("", output_style, width),
        filled_line(format!(" {}", tool.name), title_style, width),
    ];
    if tools_expanded {
        if let Some(parent) = &tool.parent_message_id {
            lines.push(filled_line(
                format!(" parent message {}", short_id(parent)),
                muted_style,
                width,
            ));
        }
        if !tool.args.is_empty() {
            lines.push(filled_line("", output_style, width));
            for line in text_lines(&tool.args) {
                lines.push(filled_line(format!(" {line}"), output_style, width));
            }
        }
        if let Some(result) = &tool.result {
            lines.push(filled_line("", output_style, width));
            for line in text_lines(result) {
                lines.push(filled_line(format!(" {line}"), output_style, width));
            }
        }
    } else if let Some(result) = &tool.result {
        lines.push(filled_line(
            format!(" {}", preview_text(result)),
            output_style,
            width,
        ));
    } else if !tool.args.is_empty() {
        lines.push(filled_line(
            format!(" {}", preview_text(&tool.args)),
            output_style,
            width,
        ));
    } else {
        lines.push(filled_line(" Running...", muted_style, width));
    }
    lines.push(filled_line("", output_style, width));
    lines
}

fn filled_line(text: impl Into<String>, style: Style, width: u16) -> Line<'static> {
    let mut text = text.into();
    let target = usize::from(width);
    let current = text.chars().count();
    if current < target {
        text.push_str(&" ".repeat(target - current));
    }
    Line::from(Span::styled(text, style))
}

fn text_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    text.lines().map(str::to_string).collect()
}
