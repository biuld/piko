use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use piko_tui_layout::{Component, SurfacePanel};

use crate::app::HitId;
use crate::features::approval::ApprovalPanel;
use crate::features::timeline::Timeline;
use crate::navigation::SurfaceId;
use crate::{
    app::{QueueStatus, ToolStatus},
    features::notifications::NotificationCenter,
    theme::Theme,
    ui::components::pane::{PaneSpec, render_pane},
};

/// Status panel: read-only diagnostic panel.
pub struct StatusPanel;

#[derive(Clone, Copy)]
pub struct StatusPanelView<'a> {
    pub session_id: Option<&'a str>,
    pub turn_id: Option<&'a str>,
    pub queue: &'a QueueStatus,
    pub notifications: &'a NotificationCenter,
    pub theme: &'a Theme,
}

/// Render context for the status surface.
pub struct StatusCtx<'a> {
    pub view: StatusPanelView<'a>,
    pub timeline: &'a Timeline,
    pub approvals: &'a ApprovalPanel,
}

impl Component<HitId, StatusCtx<'_>> for StatusPanel {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &StatusCtx<'_>) {
        StatusPanel::render(frame, area, ctx.view, ctx.timeline, ctx.approvals);
    }

    fn component_regions(&self, _area: Rect) -> Vec<(Rect, HitId)> {
        Vec::new()
    }
}

impl SurfacePanel<SurfaceId, HitId, StatusCtx<'_>> for StatusPanel {
    fn region(&self) -> SurfaceId {
        SurfaceId::Status
    }
}

impl StatusPanel {
    pub fn render(
        frame: &mut Frame<'_>,
        area: Rect,
        view: StatusPanelView<'_>,
        timeline: &Timeline,
        approvals: &ApprovalPanel,
    ) {
        let running = timeline
            .tool_calls
            .iter()
            .filter(|t| t.status == ToolStatus::Running)
            .count();
        let completed = timeline
            .tool_calls
            .iter()
            .filter(|t| t.status == ToolStatus::Completed)
            .count();
        let failed = timeline
            .tool_calls
            .iter()
            .filter(|t| t.status == ToolStatus::Failed)
            .count();

        let session = view.session_id.unwrap_or("none");
        let turn = view.turn_id.unwrap_or("none");
        let approvals_len = approvals.len().to_string();
        let queue_str = format!(
            "steer={} follow_up={} next_turn={}",
            view.queue.steer_count, view.queue.follow_up_count, view.queue.next_turn_count
        );
        let tools_str = format!(
            "{} total, {running} running, {completed} completed, {failed} failed",
            timeline.tool_calls.len()
        );
        let notifications_len = view
            .notifications
            .count_for(view.session_id, None)
            .to_string();

        let accent = view.theme.accent;
        let mut lines = vec![
            kv("session ", session, accent),
            kv("active turn ", turn, accent),
            kv("queue ", &queue_str, accent),
            kv("approvals ", &approvals_len, accent),
            kv("tools ", &tools_str, accent),
            kv("notifications ", &notifications_len, accent),
        ];

        if let Some(preview) = &view.queue.steer_preview {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "steer preview",
                Style::default().fg(view.theme.warning),
            )));
            lines.push(Line::from(preview.as_str()));
        }
        if let Some(preview) = &view.queue.follow_up_preview {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "follow-up preview",
                Style::default().fg(view.theme.warning),
            )));
            lines.push(Line::from(preview.as_str()));
        }

        let spec = PaneSpec::new("status").hints("Esc close").focused(true);
        if let Some(areas) = render_pane(frame, area, &spec, view.theme) {
            frame.render_widget(Paragraph::new(lines), areas.content);
        }
    }
}

fn kv<'a>(key: &'a str, value: &'a str, accent: ratatui::style::Color) -> Line<'a> {
    Line::from(vec![
        Span::styled(key, Style::default().fg(accent)),
        Span::raw(value.to_string()),
    ])
}
