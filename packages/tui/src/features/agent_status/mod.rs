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

use crate::app::{AppState, QueueStatus};

/// AgentPanel widget.
pub struct AgentPanel;

impl AgentPanel {
    pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
        let is_running = app.active_turn_id().is_some();
        let has_queue = app.queue_status.steer_count > 0
            || app.queue_status.follow_up_count > 0
            || app.queue_status.next_turn_count > 0;

        let mut lines = Vec::new();

        if is_running || has_queue {
            lines.push(render_agent_row(
                is_running,
                &app.queue_status,
                app.spinner_frame,
                app.theme.accent,
                app.theme.warning,
                app.theme.dim,
            ));
        } else {
            lines.push(render_idle_agent_row(app.theme.accent));
        }

        let widget = Paragraph::new(lines).block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::TOP)
                .border_style(Style::default().fg(app.theme.border_muted)),
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
