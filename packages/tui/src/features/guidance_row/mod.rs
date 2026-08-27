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
    input::{
        binding::{BindingContext, active_scope_stack},
        command::CommandId,
    },
    navigation::{SurfaceGuidance, SurfaceId},
    ui::components::{dock_line, hover_bg},
};

/// One frame's selected Guidance Row projection.
pub enum GuidanceContent<'a> {
    Notice(&'a Notification),
    Hint(Cow<'a, str>),
    Empty,
}

/// Binding-derived pane chrome for surfaces that cover the workspace plane.
///
/// These panes cannot use the resident Guidance row because their modal layer
/// owns the whole body. Keeping their tip/footer text here makes them use the
/// same context-sensitive registry queries as the resident row.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PaneHints {
    pub tip: Option<String>,
    pub footer: Option<String>,
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
            return suggestion_hint(app)
                .map(Cow::Owned)
                .map(GuidanceContent::Hint)
                .unwrap_or(GuidanceContent::Empty);
        }
        return GuidanceContent::Hint(composer_hint(app));
    };

    surface_hint(app, surface)
        .map(GuidanceContent::Hint)
        .unwrap_or(GuidanceContent::Empty)
}

fn composer_hint(app: &AppState) -> Cow<'static, str> {
    let submit = binding_hint(app, CommandId::EditorSubmit);
    let follow_up = binding_hint(app, CommandId::EditorFollowUp);
    let dequeue = binding_hint(app, CommandId::EditorDequeueFollowUp);
    let summary = app.queue_summary();
    let pending = queue_pending_hint(&summary);
    if app.viewed_agent_is_busy() {
        let mut parts = Vec::new();
        if let Some(key) = submit {
            parts.push(format!("{key} steer"));
        }
        if let Some(key) = follow_up {
            parts.push(format!("{key} queue"));
        }
        if let Some(key) = dequeue {
            parts.push(format!("{key} dequeue"));
        }
        return match pending {
            Some(pending) => {
                parts.push(pending);
                Cow::Owned(parts.join(" · "))
            }
            None => Cow::Owned(parts.join(" · ")),
        };
    }
    let viewed = app.agent_panel.active_agent_instance_id.as_deref();
    let has_follow_up = app
        .session
        .follow_ups
        .iter()
        .any(|item| viewed.is_some_and(|id| item.agent_instance_id == id));
    if has_follow_up || pending.is_some() {
        let mut parts = Vec::new();
        if let Some(key) = dequeue {
            parts.push(format!("{key} dequeue"));
        }
        parts.push("/ commands".to_string());
        parts.push("@ files".to_string());
        if let Some(key) = submit {
            parts.push(format!("{key} send"));
        }
        return match pending {
            Some(pending) => {
                parts.push(pending);
                Cow::Owned(parts.join(" · "))
            }
            None => Cow::Owned(parts.join(" · ")),
        };
    }
    let mut parts = vec!["/ commands".to_string(), "@ files".to_string()];
    if let Some(key) = submit {
        parts.push(format!("{key} send"));
    }
    Cow::Owned(parts.join(" · "))
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

fn suggestion_hint(app: &AppState) -> Option<String> {
    joined_hint([
        binding_pair_hint(app, CommandId::SelectionPrevious, CommandId::SelectionNext)
            .map(|key| format!("{key} navigate")),
        binding_hint(app, CommandId::CompletionAccept).map(|key| format!("{key} accept")),
        binding_hint(app, CommandId::CompletionAcceptAndSubmit).map(|key| format!("{key} execute")),
        binding_hint(app, CommandId::UiCancel).map(|key| format!("{key} cancel")),
    ])
}

fn surface_hint<'a>(app: &'a AppState, surface: SurfaceId) -> Option<Cow<'a, str>> {
    match surface.spec().guidance {
        SurfaceGuidance::DefaultList => default_list_hint(app).map(Cow::Owned),
        SurfaceGuidance::Feature => match surface {
            SurfaceId::AuthSelector if app.active_text_box_is_present() => joined_hint([
                binding_hint(app, CommandId::UiConfirm).map(|key| format!("{key} save")),
                binding_hint(app, CommandId::UiCancel).map(|key| format!("{key} back")),
            ])
            .map(Cow::Owned),
            SurfaceId::AuthSelector => default_list_hint(app).map(Cow::Owned),
            SurfaceId::Mcp | SurfaceId::Processes => joined_hint([
                binding_pair_hint(app, CommandId::SelectionPrevious, CommandId::SelectionNext)
                    .map(|key| format!("{key} browse")),
                binding_hint(app, CommandId::UiCancel).map(|key| format!("{key} close")),
            ])
            .map(Cow::Owned),
            _ => None,
        },
        SurfaceGuidance::Workflow => match surface {
            SurfaceId::Approval if !app.approvals.is_empty() => joined_hint([
                binding_pair_hint(app, CommandId::ApprovalPrevious, CommandId::ApprovalNext)
                    .map(|key| format!("{key} select")),
                binding_hint(app, CommandId::ApprovalConfirm).map(|key| format!("{key} confirm")),
                binding_hint(app, CommandId::ApprovalDecline).map(|key| format!("{key} decline")),
            ])
            .map(Cow::Owned),
            SurfaceId::ToolInteraction if !app.interactions.is_empty() => joined_hint([
                binding_pair_hint(
                    app,
                    CommandId::WorkflowPreviousChoice,
                    CommandId::WorkflowNextChoice,
                )
                .map(|key| format!("{key} select")),
                binding_hint(app, CommandId::WorkflowSubmit).map(|key| format!("{key} submit")),
                binding_hint(app, CommandId::WorkflowCancel).map(|key| format!("{key} cancel")),
                binding_pair_hint(
                    app,
                    CommandId::WorkflowPreviousStep,
                    CommandId::WorkflowNextStep,
                )
                .map(|key| format!("{key} step")),
            ])
            .map(Cow::Owned),
            _ => default_list_hint(app).map(Cow::Owned),
        },
        SurfaceGuidance::None => None,
    }
}

pub(crate) fn pane_hints(app: &AppState, surface: SurfaceId) -> PaneHints {
    match surface {
        SurfaceId::Sessions => PaneHints {
            tip: joined_hint([
                binding_hint(app, CommandId::SessionToggleScope).map(|key| format!("{key} scope")),
                binding_hint(app, CommandId::SessionToggleNamed).map(|key| format!("{key} named")),
                binding_hint(app, CommandId::SessionTogglePath).map(|key| format!("{key} path")),
            ]),
            footer: navigation_hint(app, "resume", "close"),
        },
        SurfaceId::Tree => PaneHints {
            tip: joined_hint([
                binding_pair_hint(
                    app,
                    CommandId::TreeFilterCycleBackward,
                    CommandId::TreeFilterCycleForward,
                )
                .map(|key| format!("{key} filter")),
                binding_hint(app, CommandId::TreeEditLabel).map(|key| format!("{key} label")),
                binding_pair_hint(app, CommandId::TreeFoldOrUp, CommandId::TreeUnfoldOrDown)
                    .map(|key| format!("{key} fold")),
            ]),
            footer: navigation_hint(app, "confirm", "close"),
        },
        SurfaceId::SummaryPrompt => PaneHints {
            tip: navigation_hint(app, "select", "cancel"),
            footer: None,
        },
        SurfaceId::Usage | SurfaceId::Diagnostics => PaneHints {
            tip: None,
            footer: navigation_hint(app, "scroll", "close"),
        },
        SurfaceId::Notifications => PaneHints {
            tip: None,
            footer: joined_hint([
                binding_hint(app, CommandId::NotificationToggleScope)
                    .map(|key| format!("{key} scope")),
                binding_pair_hint(
                    app,
                    CommandId::NotificationPrevious,
                    CommandId::NotificationNext,
                )
                .map(|key| format!("{key} select")),
                binding_hint(app, CommandId::NotificationCopySelected)
                    .map(|key| format!("{key} copy")),
                binding_pair_hint(
                    app,
                    CommandId::NotificationPageUp,
                    CommandId::NotificationPageDown,
                )
                .map(|key| format!("{key} scroll")),
                binding_hint(app, CommandId::UiCancel).map(|key| format!("{key} close")),
            ]),
        },
        SurfaceId::Settings => PaneHints {
            tip: None,
            footer: navigation_hint(app, "open", "close"),
        },
        _ => PaneHints::default(),
    }
}

fn navigation_hint(app: &AppState, confirm_label: &str, cancel_label: &str) -> Option<String> {
    joined_hint([
        binding_pair_hint(app, CommandId::SelectionPrevious, CommandId::SelectionNext)
            .map(|key| format!("{key} navigate")),
        binding_hint(app, CommandId::UiConfirm).map(|key| format!("{key} {confirm_label}")),
        binding_hint(app, CommandId::UiCancel).map(|key| format!("{key} {cancel_label}")),
    ])
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
            let mut spans = vec![
                Span::styled(format!(" {glyph}  "), Style::default().fg(color)),
                Span::styled(notification.message.clone(), Style::default().fg(color)),
            ];
            if let Some(key) = binding_hint(app, CommandId::NotificationDismissVisible) {
                spans.push(Span::styled(
                    format!(" · {key} dismiss"),
                    Style::default().fg(app.theme.dim),
                ));
            }
            let line = Line::from(spans);
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

pub(crate) fn binding_hint(app: &AppState, command: CommandId) -> Option<String> {
    let context = BindingContext::from_app(app, app.binding_registry.profile());
    let scopes = active_scope_stack(app);
    app.binding_registry.hint_for(command, &context, &scopes)
}

pub(crate) fn binding_pair_hint(
    app: &AppState,
    previous: CommandId,
    next: CommandId,
) -> Option<String> {
    let previous = binding_hint(app, previous)?;
    let next = binding_hint(app, next)?;
    Some(format!("{previous}/{next}"))
}

pub(crate) fn joined_hint<const N: usize>(parts: [Option<String>; N]) -> Option<String> {
    let parts = parts.into_iter().flatten().collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn default_list_hint(app: &AppState) -> Option<String> {
    joined_hint([
        binding_pair_hint(app, CommandId::SelectionPrevious, CommandId::SelectionNext)
            .map(|key| format!("{key} navigate")),
        binding_hint(app, CommandId::UiConfirm).map(|key| format!("{key} confirm")),
        binding_hint(app, CommandId::UiCancel).map(|key| format!("{key} cancel")),
    ])
}
