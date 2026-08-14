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
    navigation::{SurfaceId, SurfaceIntent},
    ui::components::{dock_line, feedback::default_list_hints, hover_bg},
};

const COMPOSER_HINTS: &str = "/ commands · @ files · Enter send";

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
        return GuidanceContent::Hint(Cow::Borrowed(COMPOSER_HINTS));
    };

    if !matches!(
        surface.intent(),
        SurfaceIntent::Select | SurfaceIntent::Dock
    ) {
        return GuidanceContent::Empty;
    }

    surface_hint(app, surface)
        .map(GuidanceContent::Hint)
        .unwrap_or(GuidanceContent::Empty)
}

fn surface_hint<'a>(app: &'a AppState, surface: SurfaceId) -> Option<Cow<'a, str>> {
    match surface {
        SurfaceId::Agents | SurfaceId::Models | SurfaceId::Thinking => {
            default_list_hints().single_line().map(Cow::Borrowed)
        }
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
        SurfaceId::Approval => app
            .approvals
            .workflow()
            .map(|workflow| Cow::Owned(workflow.help_text())),
        SurfaceId::ToolInteraction => app
            .interactions
            .front()
            .map(|interaction| Cow::Owned(interaction.workflow.help_text())),
        _ => None,
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
