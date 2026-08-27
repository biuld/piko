use std::time::Instant;

use crate::{
    app::command::{
        Action, AppAction, ApprovalAction, EditorAction, ModelAction, NotificationAction,
        SessionAction, SurfaceAction, TimelineAction, ToolInteractionAction, TreeAction,
    },
    app::{AppMode, AppState, SurfaceId},
    input::binding::{BindingContext, BindingRegistry, Resolution, TextSink, active_scope_stack},
    input::command::CommandId,
    terminal::{KeyPhase, NormalizedInput},
};

impl super::InputRouter {
    pub(super) fn route_normalized_key(
        app: &AppState,
        registry: &BindingRegistry,
        input: NormalizedInput,
    ) -> Option<Action> {
        let NormalizedInput::Key {
            stroke,
            phase,
            text,
        } = input
        else {
            return None;
        };
        if phase == KeyPhase::Release {
            return None;
        }

        let context = BindingContext::from_app(app, registry.profile());
        let scopes = active_scope_stack(app);
        match registry.resolve(stroke, phase, &context, &scopes) {
            Resolution::Command { command, .. } => dispatch(app, command),
            Resolution::Conflict { .. } | Resolution::Consumed => None,
            Resolution::Unhandled => text_fallback(app, &scopes, text),
        }
    }
}

fn dispatch(app: &AppState, command: CommandId) -> Option<Action> {
    use CommandId::*;

    let surface = app.mode().as_surface();
    let text_sink = active_scope_stack(app).text_sink();
    Some(match command {
        AppQuit => AppAction::Quit.into(),
        WorkspaceIdleEscape => AppAction::IdleEscape(Instant::now()).into(),
        TurnInterrupt => EditorAction::Cancel.into(),
        EditorSubmit => EditorAction::Submit.into(),
        EditorNewline => EditorAction::InsertNewline.into(),
        EditorClear => EditorAction::Cancel.into(),
        EditorHistoryPrevious => EditorAction::HistoryPrev.into(),
        EditorHistoryNext => EditorAction::HistoryNext.into(),
        EditorCursorLeft => EditorAction::CursorLeft.into(),
        EditorCursorRight => EditorAction::CursorRight.into(),
        EditorCursorWordLeft => EditorAction::CursorWordLeft.into(),
        EditorCursorWordRight => EditorAction::CursorWordRight.into(),
        EditorCursorLineStart => EditorAction::CursorLineStart.into(),
        EditorCursorLineEnd => EditorAction::CursorLineEnd.into(),
        TextDeleteBackward => match text_sink {
            Some(TextSink::Surface) => SurfaceAction::FilterBackspace.into(),
            _ => EditorAction::DeleteBackward.into(),
        },
        TextDeleteForward => EditorAction::DeleteForward.into(),
        TextDeleteWordBackward => EditorAction::DeleteWordBackward.into(),
        TextDeleteWordForward => EditorAction::DeleteWordForward.into(),
        TextDeleteToLineStart => EditorAction::DeleteToLineStart.into(),
        TextDeleteToLineEnd => EditorAction::DeleteToLineEnd.into(),
        TimelinePageUp | NotificationPageUp => match surface {
            Some(SurfaceId::Notifications) => NotificationAction::ScrollUp(10).into(),
            _ => TimelineAction::ScrollUp(8).into(),
        },
        TimelinePageDown | NotificationPageDown => match surface {
            Some(SurfaceId::Notifications) => NotificationAction::ScrollDown(10).into(),
            _ => TimelineAction::ScrollDown(8).into(),
        },
        TimelineUp => TimelineAction::ScrollUp(1).into(),
        TimelineDown => TimelineAction::ScrollDown(1).into(),
        TimelineJumpLatest => TimelineAction::JumpLatest.into(),
        UiCancel => cancel_action(app),
        UiConfirm => match surface {
            Some(SurfaceId::Notifications) => NotificationAction::CopySelected.into(),
            Some(_) => SurfaceAction::Confirm.into(),
            None => return None,
        },
        SelectionPrevious => selection_action(app, true),
        SelectionNext => selection_action(app, false),
        SelectionPagePrevious | SelectionPageNext => match surface {
            Some(SurfaceId::Notifications) => {
                if matches!(command, SelectionPagePrevious) {
                    NotificationAction::ScrollUp(10).into()
                } else {
                    NotificationAction::ScrollDown(10).into()
                }
            }
            Some(_) => {
                if matches!(command, SelectionPagePrevious) {
                    SurfaceAction::SelectPrev.into()
                } else {
                    SurfaceAction::SelectNext.into()
                }
            }
            None => return None,
        },
        CompletionAccept => EditorAction::AcceptSuggestion.into(),
        CompletionAcceptAndSubmit => EditorAction::AcceptAndSubmitSuggestion.into(),
        SessionListOpen => SessionAction::RequestList.into(),
        SessionTreeOpen => SurfaceAction::OpenTree.into(),
        ModelSelectorOpen => ModelAction::RequestList.into(),
        AgentSelectorOpen => SurfaceAction::OpenAgents.into(),
        SettingsOpen => SurfaceAction::OpenSettings.into(),
        UsageOpen => SurfaceAction::OpenUsage.into(),
        NotificationDismissVisible => NotificationAction::DismissVisible.into(),
        NotificationToggleScope => NotificationAction::ToggleScope.into(),
        NotificationPrevious => NotificationAction::SelectPrev.into(),
        NotificationNext => NotificationAction::SelectNext.into(),
        NotificationCopySelected => NotificationAction::CopySelected.into(),
        EditorFollowUp => EditorAction::FollowUp.into(),
        EditorSteer => EditorAction::Steer.into(),
        EditorDequeueFollowUp => EditorAction::DequeueFollowUp.into(),
        ClipboardPasteImage => EditorAction::PasteImage.into(),
        TreeFoldOrUp => TreeAction::FoldOrUp.into(),
        TreeUnfoldOrDown => TreeAction::UnfoldOrDown.into(),
        TreeEditLabel => TreeAction::EditLabel.into(),
        TreeToggleLabelTimestamp => TreeAction::ToggleLabelTimestamp.into(),
        TreeFilterCycleForward => TreeAction::FilterCycleForward.into(),
        TreeFilterCycleBackward => TreeAction::FilterCycleBackward.into(),
        SessionToggleScope => SessionAction::ToggleScope.into(),
        SessionToggleNamed => SessionAction::ToggleNamed.into(),
        SessionTogglePath => SessionAction::TogglePath.into(),
        ApprovalDecline => ApprovalAction::Respond(piko_protocol::ApprovalDecision::Decline).into(),
        ApprovalConfirm => ApprovalAction::ConfirmSelected.into(),
        ApprovalPrevious => ApprovalAction::SelectPrev.into(),
        ApprovalNext => ApprovalAction::SelectNext.into(),
        WorkflowSubmit => ToolInteractionAction::Submit.into(),
        WorkflowCancel => ToolInteractionAction::Cancel.into(),
        WorkflowNextStep => ToolInteractionAction::NextStep.into(),
        WorkflowPreviousStep => ToolInteractionAction::PrevStep.into(),
        WorkflowPreviousChoice => ToolInteractionAction::SelectPrev.into(),
        WorkflowNextChoice => ToolInteractionAction::SelectNext.into(),
    })
}

fn cancel_action(app: &AppState) -> Action {
    if app.has_suggestions() {
        EditorAction::CancelSuggestions.into()
    } else if app.pending_decide() == Some(SurfaceId::Approval) {
        ApprovalAction::Respond(piko_protocol::ApprovalDecision::Decline).into()
    } else if app.pending_decide() == Some(SurfaceId::ToolInteraction) {
        ToolInteractionAction::Cancel.into()
    } else if app.mode() != AppMode::Chat {
        SurfaceAction::Close.into()
    } else if app.active_turn_id().is_some() {
        EditorAction::Cancel.into()
    } else {
        // `ui.cancel` is not bound in the editor while it contains text.  A
        // custom rule can still target that scope, and its safest reducer
        // mapping is the ordinary editor cancellation action.
        EditorAction::Cancel.into()
    }
}

fn selection_action(app: &AppState, previous: bool) -> Action {
    if app.has_suggestions() {
        if previous {
            EditorAction::SuggestionSelectPrev.into()
        } else {
            EditorAction::SuggestionSelectNext.into()
        }
    } else if app.mode() == AppMode::Chat {
        if previous {
            TimelineAction::ScrollUp(1).into()
        } else {
            TimelineAction::ScrollDown(1).into()
        }
    } else if previous {
        SurfaceAction::SelectPrev.into()
    } else {
        SurfaceAction::SelectNext.into()
    }
}

fn text_fallback(
    app: &AppState,
    scopes: &crate::input::binding::ScopeStack,
    text: Option<String>,
) -> Option<Action> {
    let text = text?;
    let sink = scopes.text_sink()?;
    if matches!(sink, TextSink::Editor) {
        let mut chars = text.chars();
        let first = chars.next()?;
        if chars.next().is_none() {
            return Some(EditorAction::InsertChar(first).into());
        }
        return Some(EditorAction::InsertPaste(text).into());
    }

    // Tool workflow choices reserve unmodified ASCII digits for direct
    // choice selection.  The semantic decision is made after normalization,
    // so no terminal KeyCode or modifier checks leak into the product router.
    if app.mode() == AppMode::Surface(SurfaceId::ToolInteraction)
        && text.len() == 1
        && text.as_bytes()[0].is_ascii_digit()
        && let Some(index) = text
            .chars()
            .next()
            .and_then(|ch| ch.to_digit(10))
            .and_then(|digit| digit.checked_sub(1))
    {
        return Some(ToolInteractionAction::Choice(index as usize).into());
    }
    text.chars()
        .next()
        .map(|character| SurfaceAction::FilterAppend(character).into())
}
