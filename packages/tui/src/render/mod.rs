//! Top-level render: shell chrome + plane + modal layers (`piko-tui-layout`).

use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    app::{AppMode, AppState, HitId},
    features::{
        agent_status::AgentPanelView,
        bottom_bar::{BottomBar, BottomBarView},
        model_selector::ModelCtx,
        notifications::NotificationLevel,
        session_list::SessionListCtx,
        status::{StatusCtx, StatusPanel, StatusPanelView},
        thinking::ThinkingCtx,
        tree::TreeCtx,
    },
    layout::{Region, SurfaceId, compose_frame},
};
use piko_tui_layout::Component;

pub fn render(frame: &mut Frame<'_>, app: &AppState) {
    let composed = compose_frame(app, frame.area());
    let plan = &composed.plan;

    render_bottom_bar(frame, composed.shell.chrome, app);

    let cover_body = composed
        .modal_surface
        .map(SurfaceId::covers_body)
        .unwrap_or(false);

    if !cover_body {
        paint_regions(frame, app, &plan.rects);
    }

    for layer in &plan.layers {
        paint_regions(frame, app, &layer.rects);
    }
}

fn paint_regions(
    frame: &mut Frame<'_>,
    app: &AppState,
    rects: &std::collections::HashMap<Region, Rect>,
) {
    if let Some(area) = rects.get(&Region::Stream).copied() {
        app.timeline.render(frame, area, &app.theme);
    }
    if let Some(area) = rects.get(&Region::Notice).copied() {
        render_notification_row(frame, area, app);
    }
    if let Some(area) = rects.get(&Region::Suggest).copied() {
        app.editor.auto_complete.render(frame, area, &app.theme);
    }
    if let Some(area) = rects.get(&Region::Composer).copied() {
        render_editor(frame, app, area);
    }

    for (region, area) in rects {
        if let Region::Surface(surface) = *region {
            render_surface(frame, app, *area, surface);
        }
    }
}

fn render_bottom_bar(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let agent_label = agent_chrome_label(app);
    let agent_busy = app.agent_panel.agents().iter().any(|a| {
        app.agent_foreground(&a.agent_instance_id, &a.activity)
            .is_busy()
    });

    BottomBar::render(
        frame,
        area,
        BottomBarView {
            items: &app.tui_config.bottom_bar.items,
            agent: agent_label.as_deref(),
            agent_busy,
            spinner_frame: app.spinner_frame,
            model_id: app.model.active_model_id.as_deref(),
            thinking_level: app.model.active_thinking_level.as_deref(),
            cwd: &app.cwd,
            context_used: app.session.last_context_tokens,
            context_total: app.model.active_context_window(),
            cost_usd: app
                .session
                .cumulative_usage
                .as_ref()
                .map(|usage| usage.cost.total),
            theme: &app.theme,
        },
    );
}

/// Compact agent projection for shell chrome (not the full tree UI).
///
/// Only the viewed agent name — multi-agent roster lives on `/agents` / F4.
/// Busy work is signalled by the adjacent spinner, not a session count.
fn agent_chrome_label(app: &AppState) -> Option<String> {
    if app.agent_panel.is_loading() {
        return Some("…".to_string());
    }
    let agents = &app.agent_panel.agents();
    if agents.is_empty() {
        return None;
    }
    let active = app
        .agent_panel
        .active_agent_instance_id
        .as_ref()
        .and_then(|id| agents.iter().find(|a| &a.agent_instance_id == id))
        .or_else(|| agents.first());
    Some(
        active
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "agent".into()),
    )
}

/// Thin dispatch: call a surface panel's unified render via the
/// [`Component`] trait, letting the compiler infer the context type.
fn render_panel<P, C>(panel: &P, frame: &mut Frame<'_>, area: Rect, ctx: &C)
where
    P: Component<HitId, C>,
    C: ?Sized,
{
    Component::<HitId, C>::render(panel, frame, area, ctx);
}

fn render_surface(frame: &mut Frame<'_>, app: &AppState, area: Rect, surface: SurfaceId) {
    match surface {
        SurfaceId::Agents => {
            let foreground: Vec<_> = app
                .agent_panel
                .agents()
                .iter()
                .map(|agent| app.agent_foreground(&agent.agent_instance_id, &agent.activity))
                .collect();
            let view = AgentPanelView {
                state: &app.agent_panel,
                foreground: &foreground,
                queue: &app.queue_status,
                spinner_frame: app.spinner_frame,
                theme: &app.theme,
            };
            render_panel(&app.agent_panel, frame, area, &view);
        }
        SurfaceId::Sessions => {
            let ctx = SessionListCtx {
                active_session_id: app.session_id(),
                theme: &app.theme,
            };
            render_panel(&app.sessions, frame, area, &ctx);
        }
        SurfaceId::Tree => {
            let ctx = TreeCtx {
                filter: &app.tree.filter,
                summary_prompt: None,
                theme: &app.theme,
            };
            render_panel(&app.tree, frame, area, &ctx);
        }
        SurfaceId::SummaryPrompt => {
            let ctx = TreeCtx {
                filter: &app.tree.filter,
                summary_prompt: app.summary_prompt.as_ref(),
                theme: &app.theme,
            };
            render_panel(&app.tree, frame, area, &ctx);
        }
        SurfaceId::Status => {
            let view = StatusPanelView {
                session_id: app.session_id(),
                turn_id: app.active_turn_id(),
                queue: &app.queue_status,
                notifications: &app.notifications,
                theme: &app.theme,
            };
            let ctx = StatusCtx {
                view,
                timeline: &app.timeline,
                approvals: &app.approvals,
            };
            render_panel(&StatusPanel, frame, area, &ctx);
        }
        SurfaceId::Diagnostics => render_panel(&app.diagnostics, frame, area, &app.theme),
        SurfaceId::Settings => render_panel(&app.settings, frame, area, &app.theme),
        SurfaceId::Models => {
            let ctx = ModelCtx {
                active_model_id: app.model.active_model_id.as_deref(),
                active_provider: app.model.active_provider.as_deref(),
                theme: &app.theme,
            };
            render_panel(&app.models, frame, area, &ctx);
        }
        SurfaceId::Thinking => {
            let ctx = ThinkingCtx {
                active_level: app.model.active_thinking_level.as_deref(),
                theme: &app.theme,
            };
            render_panel(&app.thinking, frame, area, &ctx);
        }
        SurfaceId::Approval => render_panel(&app.approvals, frame, area, &app.theme),
        SurfaceId::ToolInteraction => render_panel(&app.interactions, frame, area, &app.theme),
        SurfaceId::AuthSelector => render_panel(&app.auth_selector, frame, area, &app.theme),
        SurfaceId::Mcp => render_panel(&app.mcp, frame, area, &app.theme),
    }
}

fn render_editor(frame: &mut Frame<'_>, app: &AppState, area: Rect) {
    let border_color = app.theme.border_muted;
    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(Style::default().fg(border_color));
    app.editor.render(frame, area, block);

    if app.mode == AppMode::Chat {
        let visible_rows = area.height.saturating_sub(2).max(1);
        let (row, col) = app.editor.cursor_line_col(area.width, visible_rows);
        let cursor_x = area.x + col.min(area.width.saturating_sub(1));
        let cursor_y = area.y + 1 + row.min(visible_rows.saturating_sub(1));
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

fn render_notification_row(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let Some(notification) = app.notifications.visible() else {
        return;
    };
    let color = match notification.level {
        NotificationLevel::Info => app.theme.info,
        NotificationLevel::Warning => app.theme.warning,
        NotificationLevel::Error => app.theme.error,
    };
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled(&notification.message, Style::default().fg(color)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}
