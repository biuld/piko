//! AgentPanel — inline agent status row between Timeline and Editor.
//!
//! Architecture spec (from architecture.md Section 7):
//! - Collapsed: single line `● main`
//! - Expanded: agent line + queue count + queue previews
//! - State icons: idle ● / running ◌(spinner)
//! - Notification row is now a separate layout slot (see render.rs).

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{app::QueueStatus, theme::Theme};

/// AgentPanel widget.
pub struct AgentPanel;

pub struct AgentPanelView<'a> {
    pub is_running: bool,
    pub queue: &'a QueueStatus,
    pub spinner_frame: usize,
    pub theme: &'a Theme,
}

impl AgentPanel {
    pub fn render(frame: &mut Frame<'_>, area: Rect, view: AgentPanelView<'_>) {
        let has_queue = view.queue.steer_count > 0
            || view.queue.follow_up_count > 0
            || view.queue.next_turn_count > 0;

        let mut lines = Vec::new();

        if view.is_running || has_queue {
            lines.push(render_agent_row(
                view.is_running,
                view.queue,
                view.spinner_frame,
                view.theme.accent,
                view.theme.warning,
                view.theme.dim,
            ));
        } else {
            lines.push(render_idle_agent_row(view.theme.accent));
        }

        let widget = Paragraph::new(lines).block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::TOP)
                .border_style(Style::default().fg(view.theme.border_muted)),
        );
        frame.render_widget(widget, area);
    }
}

fn render_agent_row<'a>(
    is_running: bool,
    queue: &QueueStatus,
    frame_idx: usize,
    accent: Color,
    warning: Color,
    dim: Color,
) -> Line<'a> {
    let spinner = if is_running {
        let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        frames[frame_idx % frames.len()]
    } else {
        "●"
    };

    let marker_style = if is_running {
        Style::default().fg(warning)
    } else {
        Style::default().fg(accent)
    };

    let mut spans = vec![
        Span::raw(" "),
        Span::styled(spinner, marker_style),
        Span::raw("   "),
        Span::styled("main", Style::default().add_modifier(Modifier::BOLD)),
    ];

    let total_queue = queue.steer_count + queue.follow_up_count + queue.next_turn_count;
    if total_queue > 0 {
        spans.push(Span::raw("    "));
        spans.push(Span::styled(
            format!("{} queued", total_queue),
            Style::default().fg(dim),
        ));
    }

    Line::from(spans)
}

fn render_idle_agent_row<'a>(accent: Color) -> Line<'a> {
    Line::from(vec![
        Span::raw(" "),
        Span::styled("●", Style::default().fg(accent)),
        Span::raw("   "),
        Span::styled("main", Style::default().add_modifier(Modifier::BOLD)),
    ])
}
