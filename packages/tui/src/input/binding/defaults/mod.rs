use super::{BindingRule, RuleSource};
use crate::input::{
    binding::{Condition, ContextAtom, KeyStroke, ScopeKind},
    command::CommandId,
};

macro_rules! rule {
    ($id:literal, $key:literal, $command:ident, $scope:ident) => {
        build_rule($id, $key, CommandId::$command, ScopeKind::$scope, &[])
    };
    ($id:literal, $key:literal, $command:ident, $scope:ident, [$($when:literal),+ $(,)?]) => {
        build_rule($id, $key, CommandId::$command, ScopeKind::$scope, &[$($when),+])
    };
}

mod history;

/// Stable built-in rule IDs. A user override replaces one of these IDs rather
/// than replacing the whole keymap.
pub fn default_rules() -> Vec<BindingRule> {
    let mut rules = history::rules();
    rules.extend([
        rule!("default-app-quit", "ctrl+d", AppQuit, Application),
        rule!("default-workspace-tree", "f2", SessionTreeOpen, Workspace),
        rule!(
            "default-workspace-models",
            "f3",
            ModelSelectorOpen,
            Workspace
        ),
        rule!(
            "default-workspace-agents",
            "f4",
            AgentSelectorOpen,
            Workspace
        ),
        rule!(
            "default-workspace-notice",
            "f8",
            NotificationDismissVisible,
            Workspace
        ),
        rule!(
            "default-timeline-page-up",
            "pageup",
            TimelinePageUp,
            Timeline
        ),
        rule!(
            "default-timeline-page-down",
            "pagedown",
            TimelinePageDown,
            Timeline
        ),
        rule!("default-timeline-up", "up", TimelineUp, Timeline),
        rule!("default-timeline-down", "down", TimelineDown, Timeline),
        rule!("default-editor-submit", "enter", EditorSubmit, Editor),
        rule!(
            "default-editor-newline-enhanced",
            "shift+enter",
            EditorNewline,
            Editor,
            ["editor.multiline", "terminal.enhancedKeyboard"]
        ),
        rule!(
            "default-editor-newline-fallback",
            "ctrl+j",
            EditorNewline,
            Editor,
            ["editor.multiline", "!terminal.enhancedKeyboard"]
        ),
        rule!(
            "default-editor-history-previous",
            "ctrl+p",
            EditorHistoryPrevious,
            Editor,
            ["!suggest.visible"]
        ),
        rule!(
            "default-editor-history-next",
            "ctrl+n",
            EditorHistoryNext,
            Editor,
            ["editor.historyBrowsing"]
        ),
        rule!("default-editor-left", "left", EditorCursorLeft, Editor),
        rule!(
            "default-editor-ctrl-left",
            "ctrl+b",
            EditorCursorLeft,
            Editor
        ),
        rule!(
            "default-editor-alt-left",
            "alt+b",
            EditorCursorWordLeft,
            Editor
        ),
        rule!(
            "default-editor-alt-left-key",
            "alt+left",
            EditorCursorWordLeft,
            Editor
        ),
        rule!("default-editor-right", "right", EditorCursorRight, Editor),
        rule!(
            "default-editor-ctrl-right",
            "ctrl+f",
            EditorCursorRight,
            Editor
        ),
        rule!(
            "default-editor-alt-right",
            "alt+f",
            EditorCursorWordRight,
            Editor
        ),
        rule!(
            "default-editor-alt-right-key",
            "alt+right",
            EditorCursorWordRight,
            Editor
        ),
        rule!(
            "default-editor-word-left",
            "ctrl+left",
            EditorCursorWordLeft,
            Editor
        ),
        rule!(
            "default-editor-word-right",
            "ctrl+right",
            EditorCursorWordRight,
            Editor
        ),
        rule!(
            "default-editor-line-start",
            "ctrl+a",
            EditorCursorLineStart,
            Editor
        ),
        rule!("default-editor-home", "home", EditorCursorLineStart, Editor),
        rule!(
            "default-editor-line-end",
            "ctrl+e",
            EditorCursorLineEnd,
            Editor
        ),
        rule!("default-editor-end", "end", EditorCursorLineEnd, Editor),
        rule!(
            "default-editor-delete-backward",
            "backspace",
            TextDeleteBackward,
            Editor
        ),
        rule!(
            "default-editor-delete-forward",
            "delete",
            TextDeleteForward,
            Editor
        ),
        rule!(
            "default-editor-delete-word-backward",
            "ctrl+w",
            TextDeleteWordBackward,
            Editor
        ),
        rule!(
            "default-editor-alt-delete-word-backward",
            "alt+backspace",
            TextDeleteWordBackward,
            Editor
        ),
        rule!(
            "default-editor-delete-word-forward",
            "alt+d",
            TextDeleteWordForward,
            Editor
        ),
        rule!(
            "default-editor-alt-delete-word-forward",
            "alt+delete",
            TextDeleteWordForward,
            Editor
        ),
        rule!(
            "default-editor-delete-line-start",
            "ctrl+u",
            TextDeleteToLineStart,
            Editor
        ),
        rule!(
            "default-editor-delete-line-end",
            "ctrl+k",
            TextDeleteToLineEnd,
            Editor
        ),
        rule!(
            "default-editor-follow-up",
            "alt+enter",
            EditorFollowUp,
            Editor
        ),
        rule!(
            "default-editor-steer",
            "ctrl+enter",
            EditorSteer,
            Editor,
            ["turn.running"]
        ),
        rule!(
            "default-editor-dequeue",
            "alt+up",
            EditorDequeueFollowUp,
            Editor
        ),
        rule!(
            "default-editor-paste-image",
            "ctrl+v",
            ClipboardPasteImage,
            Editor
        ),
        rule!(
            "default-editor-interrupt",
            "esc",
            TurnInterrupt,
            Editor,
            ["agent.running", "!suggest.visible"]
        ),
        rule!(
            "default-editor-interrupt-ctrl-c",
            "ctrl+c",
            TurnInterrupt,
            Editor,
            ["agent.running", "!timeline.selectionActive"]
        ),
        rule!(
            "default-timeline-copy-selection",
            "ctrl+c",
            TimelineCopySelection,
            Editor,
            ["timeline.selectionActive"]
        ),
        rule!(
            "default-editor-clear",
            "ctrl+c",
            EditorClear,
            Editor,
            ["!agent.running", "!timeline.selectionActive"]
        ),
        rule!(
            "default-workspace-idle-escape",
            "esc",
            WorkspaceIdleEscape,
            Workspace,
            ["editor.empty", "!agent.running", "!suggest.visible"]
        ),
        rule!(
            "default-suggest-previous",
            "up",
            SelectionPrevious,
            Suggestions
        ),
        rule!("default-suggest-next", "down", SelectionNext, Suggestions),
        rule!(
            "default-suggest-previous-tab",
            "shift+tab",
            SelectionPrevious,
            Suggestions
        ),
        rule!(
            "default-suggest-accept",
            "tab",
            CompletionAccept,
            Suggestions
        ),
        rule!(
            "default-suggest-accept-submit",
            "enter",
            CompletionAcceptAndSubmit,
            Suggestions
        ),
        rule!("default-suggest-cancel", "esc", UiCancel, Suggestions),
        rule!(
            "default-selection-previous",
            "up",
            SelectionPrevious,
            Selection
        ),
        rule!("default-selection-next", "down", SelectionNext, Selection),
        rule!(
            "default-selection-page-previous",
            "pageup",
            SelectionPagePrevious,
            Selection
        ),
        rule!(
            "default-selection-page-next",
            "pagedown",
            SelectionPageNext,
            Selection
        ),
        rule!("default-selection-confirm", "enter", UiConfirm, Selection),
        rule!("default-selection-cancel", "esc", UiCancel, Selection),
        rule!(
            "default-text-delete-backward",
            "backspace",
            TextDeleteBackward,
            Selection
        ),
        rule!("default-tree-tab", "tab", TreeFilterCycleForward, Tree),
        rule!(
            "default-tree-shift-tab",
            "shift+tab",
            TreeFilterCycleBackward,
            Tree
        ),
        rule!(
            "default-tree-filter-forward",
            "ctrl+o",
            TreeFilterCycleForward,
            Tree
        ),
        rule!(
            "default-tree-filter-backward",
            "ctrl+shift+o",
            TreeFilterCycleBackward,
            Tree
        ),
        rule!("default-tree-fold", "alt+left", TreeFoldOrUp, Tree),
        rule!("default-tree-unfold", "alt+right", TreeUnfoldOrDown, Tree),
        rule!("default-tree-edit-label", "shift+l", TreeEditLabel, Tree),
        rule!(
            "default-tree-label-timestamps",
            "shift+t",
            TreeToggleLabelTimestamp,
            Tree
        ),
        rule!(
            "default-sessions-scope",
            "tab",
            SessionToggleScope,
            Sessions
        ),
        rule!(
            "default-sessions-named",
            "ctrl+n",
            SessionToggleNamed,
            Sessions
        ),
        rule!(
            "default-notification-scope",
            "tab",
            NotificationToggleScope,
            Notifications
        ),
        rule!(
            "default-notification-previous",
            "up",
            NotificationPrevious,
            Notifications
        ),
        rule!(
            "default-notification-next",
            "down",
            NotificationNext,
            Notifications
        ),
        rule!(
            "default-notification-page-up",
            "pageup",
            NotificationPageUp,
            Notifications
        ),
        rule!(
            "default-notification-page-down",
            "pagedown",
            NotificationPageDown,
            Notifications
        ),
        rule!(
            "default-notification-copy",
            "c",
            NotificationCopySelected,
            Notifications
        ),
        rule!("default-approval-decline", "esc", ApprovalDecline, Approval),
        rule!(
            "default-approval-confirm",
            "enter",
            ApprovalConfirm,
            Approval
        ),
        rule!(
            "default-approval-previous",
            "up",
            ApprovalPrevious,
            Approval
        ),
        rule!("default-approval-next", "down", ApprovalNext, Approval),
        rule!(
            "default-workflow-submit",
            "enter",
            WorkflowSubmit,
            ToolInteraction
        ),
        rule!(
            "default-workflow-cancel",
            "esc",
            WorkflowCancel,
            ToolInteraction
        ),
        rule!(
            "default-workflow-next-step",
            "tab",
            WorkflowNextStep,
            ToolInteraction
        ),
        rule!(
            "default-workflow-previous-step",
            "shift+tab",
            WorkflowPreviousStep,
            ToolInteraction
        ),
        rule!(
            "default-workflow-previous-choice",
            "up",
            WorkflowPreviousChoice,
            ToolInteraction
        ),
        rule!(
            "default-workflow-delete-backward",
            "backspace",
            TextDeleteBackward,
            ToolInteraction,
            ["text.inputActive"]
        ),
        rule!(
            "default-workflow-next-choice",
            "down",
            WorkflowNextChoice,
            ToolInteraction
        ),
    ]);
    rules
}

fn build_rule(
    id: &str,
    key: &str,
    command: CommandId,
    scope: ScopeKind,
    when: &[&str],
) -> BindingRule {
    BindingRule {
        id: id.to_string(),
        key: KeyStroke::parse(key).expect("built-in key must parse"),
        command,
        scope,
        conditions: when
            .iter()
            .map(|raw| {
                let (atom, negated) = ContextAtom::parse(raw).expect("built-in condition");
                Condition { atom, negated }
            })
            .collect(),
        source: RuleSource::BuiltIn,
        enabled: true,
    }
}
