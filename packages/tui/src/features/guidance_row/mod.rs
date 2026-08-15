//! Resident one-row notice / contextual-hint projection above Composer.

use std::borrow::Cow;

use piko_tui_layout::InteractionState;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
};

use crate::{
    app::{AppState, HitId},
    features::notifications::{Notification, NotificationLevel, level_glyph},
    navigation::{SurfaceGuidance, SurfaceId},
    ui::components::{dock_line, feedback::default_list_hints, hover_bg},
};

const COMPOSER_HINTS: &str = "/ commands · @ files · Enter send";
const RUNNING_HINTS: &str = "Enter steer · Alt+Enter queue · Alt+↑ dequeue";
const DEQUEUE_HINTS: &str = "Alt+↑ dequeue · / commands · Enter send";

/// One frame's selected Guidance Row projection.
pub enum GuidanceContent<'a> {
    Notice(&'a Notification),
    Hint(Cow<'a, str>),
    Empty,
}

impl GuidanceContent<'_> {
    pub fn is_notice(&self) -> bool {
        matches!(self, Self::Notice(_))
    }
}

pub fn resolve(app: &AppState) -> GuidanceContent<'_> {
    if let Some(notice) = app.notifications.row_visible_for(
        app.last_tick,
        app.session.id.as_deref(),
        app.agent_panel.active_agent_instance_id.as_deref(),
    ) {
        return GuidanceContent::Notice(notice);
    }

    let Some(surface) = app.modal_surface() else {
        if app.editor.auto_complete.is_active() {
            return app
                .editor
                .auto_complete
                .interaction_hints()
                .single_line()
                .map(|hint| GuidanceContent::Hint(Cow::Borrowed(hint)))
                .unwrap_or(GuidanceContent::Empty);
        }
        return GuidanceContent::Hint(composer_hint(app));
    };

    surface_hint(app, surface)
        .map(GuidanceContent::Hint)
        .unwrap_or(GuidanceContent::Empty)
}

fn composer_hint(app: &AppState) -> Cow<'static, str> {
    let summary = app.queue_summary();
    let pending = queue_pending_hint(&summary);
    if app.viewed_agent_is_busy() {
        return match pending {
            Some(pending) => Cow::Owned(format!("{RUNNING_HINTS} · {pending}")),
            None => Cow::Borrowed(RUNNING_HINTS),
        };
    }
    let viewed = app.agent_panel.active_agent_instance_id.as_deref();
    let has_follow_up = app
        .session
        .follow_ups
        .iter()
        .any(|item| viewed.is_some_and(|id| item.agent_instance_id == id));
    if has_follow_up || pending.is_some() {
        return match pending {
            Some(pending) => Cow::Owned(format!("{DEQUEUE_HINTS} · {pending}")),
            None => Cow::Borrowed(DEQUEUE_HINTS),
        };
    }
    Cow::Borrowed(COMPOSER_HINTS)
}

fn queue_pending_hint(summary: &crate::app::QueueStatus) -> Option<String> {
    let mut parts = Vec::new();
    if summary.steer_count > 0 {
        parts.push(format!("{} steer", summary.steer_count));
    }
    if summary.follow_up_count > 0 {
        parts.push(format!("{} queued", summary.follow_up_count));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

fn surface_hint<'a>(app: &'a AppState, surface: SurfaceId) -> Option<Cow<'a, str>> {
    match surface.spec().guidance {
        SurfaceGuidance::DefaultList => default_list_hints().single_line().map(Cow::Borrowed),
        SurfaceGuidance::Feature => match surface {
            SurfaceId::AuthSelector => app
                .auth_selector
                .interaction_hints()
                .single_line()
                .map(Cow::Borrowed),
            SurfaceId::Mcp => app.mcp.interaction_hints().single_line().map(Cow::Borrowed),
            SurfaceId::Processes => app
                .processes
                .interaction_hints()
                .single_line()
                .map(Cow::Borrowed),
            _ => None,
        },
        SurfaceGuidance::Workflow => match surface {
            SurfaceId::Approval => app
                .approvals
                .workflow()
                .map(|workflow| Cow::Owned(workflow.help_text())),
            SurfaceId::ToolInteraction => app
                .interactions
                .front()
                .map(|interaction| Cow::Owned(interaction.workflow.help_text())),
            _ => None,
        },
        SurfaceGuidance::None => None,
    }
}

pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &AppState,
    interaction: InteractionState<HitId>,
) {
    match resolve(app) {
        GuidanceContent::Notice(notification) => {
            let color = match notification.level {
                NotificationLevel::Info => app.theme.info,
                NotificationLevel::Warning => app.theme.warning,
                NotificationLevel::Error => app.theme.error,
            };
            let glyph = level_glyph(notification.level);
            let line = Line::from(vec![
                Span::styled(format!(" {glyph}  "), Style::default().fg(color)),
                Span::styled(notification.message.clone(), Style::default().fg(color)),
                Span::styled(" · F8 dismiss", Style::default().fg(app.theme.dim)),
            ]);
            let background = (interaction.hovered == Some(HitId::Notice))
                .then(|| hover_bg(&app.theme))
                .flatten();
            dock_line::render(frame, area, line, background);
        }
        GuidanceContent::Hint(hint) => {
            dock_line::render(frame, area, dock_line::hint_line(&hint, &app.theme), None);
        }
        GuidanceContent::Empty => {}
    }
}
