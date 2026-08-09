//! Top-level render: shell chrome + plane + modal layers (`piko-tui-layout`).

use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders},
};

use crate::{
    app::{AppMode, AppState, HitId},
    features::{
        agent_status::AgentPanelView,
        bottom_bar::{BottomBar, BottomBarView},
        model_selector::ModelCtx,
        notifications::{NotificationLevel, NotificationPanelCtx},
        session_list::SessionListCtx,
        status::{StatusCtx, StatusPanel, StatusPanelView},
        thinking::ThinkingCtx,
        tree::TreeCtx,
    },
    layout::{Region, SurfaceId, compose_frame},
};
use piko_tui_layout::{Component, InteractionState};

#[cfg(test)]
mod tests;

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

    // Real terminal caret while inline-editing a tool-interaction workflow.
    // Ratatui hides the cursor on any frame that does not call
    // `set_cursor_position`, so non-editing frames stay caret-free.
    if app.mode == AppMode::Surface(SurfaceId::ToolInteraction)
        && let Some(area) = plan
            .layers
            .first()
            .and_then(|l| l.rects.get(&Region::Surface(SurfaceId::ToolInteraction)))
            .copied()
        && let Some(interaction) = app.interactions.front()
        && let Some(position) = interaction.workflow.input_cursor(area)
    {
        frame.set_cursor_position(position);
    }
}

fn paint_regions(
    frame: &mut Frame<'_>,
    app: &AppState,
    rects: &std::collections::HashMap<Region, Rect>,
) {
    if let Some(area) = rects.get(&Region::Stream).copied() {
        app.timeline.render_with_state(
            frame,
            area,
            &app.theme,
            interaction_state(app, Region::Stream),
        );
    }
    if let Some(area) = rects.get(&Region::Notice).copied() {
        render_notification_row(frame, area, app, interaction_state(app, Region::Notice));
    }
    if let Some(area) = rects.get(&Region::Suggest).copied() {
        app.editor.auto_complete.render(
            frame,
            area,
            &app.theme,
            interaction_state(app, Region::Suggest),
        );
    }
    if let Some(area) = rects.get(&Region::Composer).copied() {
        render_editor(frame, app, area, interaction_state(app, Region::Composer));
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
fn render_panel<P, C>(
    panel: &P,
    frame: &mut Frame<'_>,
    area: Rect,
    ctx: &C,
    interaction: InteractionState<HitId>,
) where
    P: Component<HitId, C>,
    C: ?Sized,
{
    Component::<HitId, C>::render_with_state(panel, frame, area, ctx, interaction);
}

fn render_surface(frame: &mut Frame<'_>, app: &AppState, area: Rect, surface: SurfaceId) {
    let interaction = interaction_state(app, Region::Surface(surface));
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
            render_panel(&app.agent_panel, frame, area, &view, interaction);
        }
        SurfaceId::Sessions => {
            let ctx = SessionListCtx {
                active_session_id: app.session_id(),
                theme: &app.theme,
            };
            render_panel(&app.sessions, frame, area, &ctx, interaction);
        }
        SurfaceId::Tree => {
            let ctx = TreeCtx {
                filter: &app.tree.filter,
                summary_prompt: None,
                theme: &app.theme,
            };
            render_panel(&app.tree, frame, area, &ctx, interaction);
        }
        SurfaceId::SummaryPrompt => {
            let ctx = TreeCtx {
                filter: &app.tree.filter,
                summary_prompt: app.summary_prompt.as_ref(),
                theme: &app.theme,
            };
            render_panel(&app.tree, frame, area, &ctx, interaction);
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
            render_panel(&StatusPanel, frame, area, &ctx, interaction);
        }
        SurfaceId::Notifications => {
            let ctx = NotificationPanelCtx {
                session_id: app.session_id(),
                now: app.last_tick,
                theme: &app.theme,
            };
            render_panel(&app.notifications, frame, area, &ctx, interaction);
        }
        SurfaceId::Diagnostics => {
            render_panel(&app.diagnostics, frame, area, &app.theme, interaction)
        }
        SurfaceId::Settings => render_panel(&app.settings, frame, area, &app.theme, interaction),
        SurfaceId::Models => {
            let ctx = ModelCtx {
                active_model_id: app.model.active_model_id.as_deref(),
                active_provider: app.model.active_provider.as_deref(),
                theme: &app.theme,
            };
            render_panel(&app.models, frame, area, &ctx, interaction);
        }
        SurfaceId::Thinking => {
            let ctx = ThinkingCtx {
                active_level: app.model.active_thinking_level.as_deref(),
                theme: &app.theme,
            };
            render_panel(&app.thinking, frame, area, &ctx, interaction);
        }
        SurfaceId::Approval => render_panel(&app.approvals, frame, area, &app.theme, interaction),
        SurfaceId::ToolInteraction => {
            render_panel(&app.interactions, frame, area, &app.theme, interaction)
        }
        SurfaceId::AuthSelector => {
            render_panel(&app.auth_selector, frame, area, &app.theme, interaction)
        }
        SurfaceId::Mcp => render_panel(&app.mcp, frame, area, &app.theme, interaction),
        SurfaceId::Processes => render_panel(&app.processes, frame, area, &app.theme, interaction),
    }
}

fn interaction_state(app: &AppState, region: Region) -> InteractionState<HitId> {
    let plane_is_blocked = !matches!(region, Region::Surface(_)) && app.modal_surface().is_some();
    let hovered = (!plane_is_blocked)
        .then_some(app.hovered)
        .flatten()
        .and_then(|(hovered_region, element)| (hovered_region == region).then_some(element))
        .flatten();
    InteractionState { hovered }
}

fn render_editor(
    frame: &mut Frame<'_>,
    app: &AppState,
    area: Rect,
    _interaction: InteractionState<HitId>,
) {
    let focused = app.mode == AppMode::Chat;
    let border_color = if focused {
        app.theme.prompt_border_active
    } else {
        app.theme.prompt_border
    };
    let background = app.theme.bg_elevated;
    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .style(Style::default().bg(background))
        .border_style(Style::default().fg(border_color).bg(background));
    app.editor.render(frame, area, block);

    if focused {
        let visible_rows = area.height.saturating_sub(2).max(1);
        let (row, col) = app.editor.cursor_line_col(area.width, visible_rows);
        let cursor_x = area.x + col.min(area.width.saturating_sub(1));
        let cursor_y = area.y + 1 + row.min(visible_rows.saturating_sub(1));
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

fn render_notification_row(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &AppState,
    interaction: InteractionState<HitId>,
) {
    let Some(notification) = app.notifications.row_visible_for(
        app.last_tick,
        app.session.id.as_deref(),
        app.agent_panel.active_agent_instance_id.as_deref(),
    ) else {
        return;
    };
    let (label, color) = match notification.level {
        NotificationLevel::Info => ("info", app.theme.info),
        NotificationLevel::Warning => ("warning", app.theme.warning),
        NotificationLevel::Error => ("error", app.theme.error),
    };
    let line = Line::from(vec![
        Span::styled(format!(" ● {label} · "), Style::default().fg(color)),
        Span::styled(&notification.message, Style::default().fg(color)),
        Span::styled(" · F8 dismiss", Style::default().fg(app.theme.dim)),
    ]);
    let background = (interaction.hovered == Some(HitId::Notice))
        .then(|| crate::ui::components::hover_bg(&app.theme))
        .flatten();
    crate::ui::components::dock_line::render(frame, area, line, background);
}
