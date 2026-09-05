use super::CommandId;
use crate::input::binding::{BindingContext, ScopeKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepeatPolicy {
    PressOnly,
    Repeatable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalRequirement {
    EnhancedKeyboard,
}

#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub id: CommandId,
    #[allow(dead_code)]
    pub title: &'static str,
    #[allow(dead_code)]
    pub description: &'static str,
    pub scopes: &'static [ScopeKind],
    pub repeat: RepeatPolicy,
    pub terminal_requirement: Option<TerminalRequirement>,
    pub enablement: fn(&BindingContext) -> bool,
}

const APP: &[ScopeKind] = &[ScopeKind::Application];
const WORKSPACE: &[ScopeKind] = &[ScopeKind::Workspace];
const EDITOR: &[ScopeKind] = &[ScopeKind::Editor];
const TEXT_DELETE_BACKWARD: &[ScopeKind] = &[
    ScopeKind::Editor,
    ScopeKind::Selection,
    ScopeKind::ToolInteraction,
];
const CANCEL: &[ScopeKind] = &[
    ScopeKind::Selection,
    ScopeKind::Suggestions,
    ScopeKind::History,
];
const SELECTION_COMMAND: &[ScopeKind] = &[ScopeKind::Selection, ScopeKind::Suggestions];
const SUGGESTIONS: &[ScopeKind] = &[ScopeKind::Suggestions];
const TIMELINE: &[ScopeKind] = &[ScopeKind::Timeline];
const TIMELINE_COPY: &[ScopeKind] = &[ScopeKind::Editor, ScopeKind::Timeline];
const SELECTION: &[ScopeKind] = &[ScopeKind::Selection, ScopeKind::History];
const TREE: &[ScopeKind] = &[ScopeKind::Tree];
const SESSIONS: &[ScopeKind] = &[ScopeKind::Sessions];
const HISTORY: &[ScopeKind] = &[ScopeKind::History];
const NOTIFICATION: &[ScopeKind] = &[ScopeKind::Notifications];
const APPROVAL: &[ScopeKind] = &[ScopeKind::Approval];
const WORKFLOW: &[ScopeKind] = &[ScopeKind::ToolInteraction];

const fn enabled(_: &BindingContext) -> bool {
    true
}

const fn multiline(context: &BindingContext) -> bool {
    context.editor_multiline
}

const fn not_running(context: &BindingContext) -> bool {
    !context.turn_running
}

const fn running(context: &BindingContext) -> bool {
    context.turn_running
}

const fn agent_running(context: &BindingContext) -> bool {
    context.agent_running
}

const fn idle_escape(context: &BindingContext) -> bool {
    context.editor_empty && !context.turn_running && !context.suggest_visible
}

const fn suggestions(context: &BindingContext) -> bool {
    context.suggest_visible
}

const fn history_browsing(context: &BindingContext) -> bool {
    context.history_browsing
}

macro_rules! command_specs {
    ($( $id:ident, $title:literal, $description:literal, $scopes:ident, $repeat:ident, $enablement:ident; )*) => {
        vec![$(spec(
            CommandId::$id,
            $title,
            $description,
            $scopes,
            RepeatPolicy::$repeat,
            None,
            $enablement,
        )),*]
    };
}

/// The one command catalog used by configuration, defaults, routing, and
/// guidance. Commands absent from this table are not configurable.
pub fn catalog() -> Vec<CommandSpec> {
    command_specs! {
        AppQuit, "Quit", "Exit piko", APP, PressOnly, enabled;
        WorkspaceIdleEscape, "Idle escape", "Open workspace tree after the idle escape gesture", WORKSPACE, PressOnly, idle_escape;
        TurnInterrupt, "Interrupt agent", "Cancel the viewed agent's current work", EDITOR, PressOnly, agent_running;
        EditorSubmit, "Submit", "Submit the composer", EDITOR, PressOnly, enabled;
        EditorNewline, "New line", "Insert a composer newline", EDITOR, PressOnly, multiline;
        EditorClear, "Clear editor", "Clear the idle composer", EDITOR, PressOnly, not_running;
        EditorHistoryPrevious, "Previous history", "Recall the previous composer entry", EDITOR, Repeatable, enabled;
        EditorHistoryNext, "Next history", "Recall the next composer entry", EDITOR, Repeatable, history_browsing;
        EditorCursorLeft, "Cursor left", "Move the composer caret left", EDITOR, Repeatable, enabled;
        EditorCursorRight, "Cursor right", "Move the composer caret right", EDITOR, Repeatable, enabled;
        EditorCursorWordLeft, "Cursor word left", "Move the composer caret one word left", EDITOR, Repeatable, enabled;
        EditorCursorWordRight, "Cursor word right", "Move the composer caret one word right", EDITOR, Repeatable, enabled;
        EditorCursorLineStart, "Cursor line start", "Move the composer caret to line start", EDITOR, Repeatable, enabled;
        EditorCursorLineEnd, "Cursor line end", "Move the composer caret to line end", EDITOR, Repeatable, enabled;
        TextDeleteBackward, "Delete backward", "Delete text before the caret", TEXT_DELETE_BACKWARD, Repeatable, enabled;
        TextDeleteForward, "Delete forward", "Delete text after the caret", EDITOR, Repeatable, enabled;
        TextDeleteWordBackward, "Delete word backward", "Delete the previous word", EDITOR, Repeatable, enabled;
        TextDeleteWordForward, "Delete word forward", "Delete the next word", EDITOR, Repeatable, enabled;
        TextDeleteToLineStart, "Delete to line start", "Delete to the line start", EDITOR, Repeatable, enabled;
        TextDeleteToLineEnd, "Delete to line end", "Delete to the line end", EDITOR, Repeatable, enabled;
        TimelinePageUp, "Timeline page up", "Scroll the conversation up", TIMELINE, Repeatable, enabled;
        TimelinePageDown, "Timeline page down", "Scroll the conversation down", TIMELINE, Repeatable, enabled;
        TimelineUp, "Timeline up", "Scroll the conversation up one row", TIMELINE, Repeatable, enabled;
        TimelineDown, "Timeline down", "Scroll the conversation down one row", TIMELINE, Repeatable, enabled;
        TimelineJumpLatest, "Timeline latest", "Jump to the newest conversation entry", TIMELINE, PressOnly, enabled;
        TimelineCopySelection, "Copy timeline selection", "Copy selected timeline text", TIMELINE_COPY, PressOnly, enabled;
        UiCancel, "Cancel", "Cancel the current interaction", CANCEL, PressOnly, enabled;
        UiConfirm, "Confirm", "Confirm the focused choice", SELECTION, PressOnly, enabled;
        SelectionPrevious, "Previous", "Move to the previous choice", SELECTION_COMMAND, Repeatable, enabled;
        SelectionNext, "Next", "Move to the next choice", SELECTION_COMMAND, Repeatable, enabled;
        SelectionPagePrevious, "Previous page", "Move one page backward", SELECTION, Repeatable, enabled;
        SelectionPageNext, "Next page", "Move one page forward", SELECTION, Repeatable, enabled;
        HistoryRefresh, "Refresh history", "Reload the inspected session snapshot", HISTORY, PressOnly, enabled;
        HistoryChooseSession, "Inspect session", "Choose a session without opening it", HISTORY, PressOnly, enabled;
        HistoryFilter, "Filter history", "Filter the history list", HISTORY, PressOnly, enabled;
        HistoryFactsOnly, "History facts only", "Show required journal facts only", HISTORY, PressOnly, enabled;
        HistoryDiagnostics, "History diagnostics", "Toggle diagnostic visibility", HISTORY, PressOnly, enabled;
        CompletionAccept, "Accept completion", "Accept the selected suggestion", SUGGESTIONS, PressOnly, suggestions;
        CompletionAcceptAndSubmit, "Accept and submit completion", "Accept and submit the selected suggestion", SUGGESTIONS, PressOnly, suggestions;
        SessionListOpen, "Open sessions", "Open the session list", WORKSPACE, PressOnly, enabled;
        SessionTreeOpen, "Open tree", "Open the session tree", WORKSPACE, PressOnly, enabled;
        ModelSelectorOpen, "Open models", "Open the model selector", WORKSPACE, PressOnly, enabled;
        AgentSelectorOpen, "Open agents", "Open the agent selector", WORKSPACE, PressOnly, enabled;
        SettingsOpen, "Open settings", "Open settings", WORKSPACE, PressOnly, enabled;
        UsageOpen, "Open usage", "Open usage", WORKSPACE, PressOnly, enabled;
        NotificationDismissVisible, "Dismiss notice", "Dismiss the visible notice", WORKSPACE, PressOnly, enabled;
        NotificationToggleScope, "Toggle notification scope", "Toggle notification scope", NOTIFICATION, PressOnly, enabled;
        NotificationPrevious, "Previous notification", "Select the previous notification", NOTIFICATION, Repeatable, enabled;
        NotificationNext, "Next notification", "Select the next notification", NOTIFICATION, Repeatable, enabled;
        NotificationCopySelected, "Copy notification", "Copy the selected notification", NOTIFICATION, PressOnly, enabled;
        NotificationPageUp, "Notification page up", "Scroll notifications up", NOTIFICATION, Repeatable, enabled;
        NotificationPageDown, "Notification page down", "Scroll notifications down", NOTIFICATION, Repeatable, enabled;
        EditorFollowUp, "Queue follow-up", "Queue a follow-up message", EDITOR, PressOnly, enabled;
        EditorSteer, "Steer turn", "Steer the running turn", EDITOR, PressOnly, running;
        EditorDequeueFollowUp, "Dequeue follow-up", "Restore the last queued follow-up", EDITOR, PressOnly, enabled;
        ClipboardPasteImage, "Paste image", "Paste an image from the clipboard", EDITOR, PressOnly, enabled;
        TreeFoldOrUp, "Fold tree", "Fold or move to the tree parent", TREE, Repeatable, enabled;
        TreeUnfoldOrDown, "Unfold tree", "Unfold or move to a tree child", TREE, Repeatable, enabled;
        TreeEditLabel, "Edit tree label", "Edit the selected tree label", TREE, PressOnly, enabled;
        TreeToggleLabelTimestamp, "Toggle timestamps", "Toggle tree label timestamps", TREE, PressOnly, enabled;
        TreeFilterCycleForward, "Next tree filter", "Cycle the tree filter forward", TREE, PressOnly, enabled;
        TreeFilterCycleBackward, "Previous tree filter", "Cycle the tree filter backward", TREE, PressOnly, enabled;
        SessionToggleScope, "Toggle session scope", "Toggle the session list scope", SESSIONS, PressOnly, enabled;
        SessionToggleNamed, "Toggle named sessions", "Toggle named-session filtering", SESSIONS, PressOnly, enabled;
        SessionTogglePath, "Toggle session path", "Toggle session paths", SESSIONS, PressOnly, enabled;
        ApprovalDecline, "Decline approval", "Decline the pending approval", APPROVAL, PressOnly, enabled;
        ApprovalConfirm, "Confirm approval", "Confirm the selected approval", APPROVAL, PressOnly, enabled;
        ApprovalPrevious, "Previous approval", "Select the previous approval", APPROVAL, Repeatable, enabled;
        ApprovalNext, "Next approval", "Select the next approval", APPROVAL, Repeatable, enabled;
        WorkflowSubmit, "Submit workflow", "Submit the active workflow", WORKFLOW, PressOnly, enabled;
        WorkflowCancel, "Cancel workflow", "Cancel the active workflow", WORKFLOW, PressOnly, enabled;
        WorkflowNextStep, "Next workflow step", "Move to the next workflow step", WORKFLOW, PressOnly, enabled;
        WorkflowPreviousStep, "Previous workflow step", "Move to the previous workflow step", WORKFLOW, PressOnly, enabled;
        WorkflowPreviousChoice, "Previous workflow choice", "Select the previous workflow choice", WORKFLOW, Repeatable, enabled;
        WorkflowNextChoice, "Next workflow choice", "Select the next workflow choice", WORKFLOW, Repeatable, enabled;
    }
}

fn spec(
    id: CommandId,
    title: &'static str,
    description: &'static str,
    scopes: &'static [ScopeKind],
    repeat: RepeatPolicy,
    terminal_requirement: Option<TerminalRequirement>,
    enablement: fn(&BindingContext) -> bool,
) -> CommandSpec {
    CommandSpec {
        id,
        title,
        description,
        scopes,
        repeat,
        terminal_requirement,
        enablement,
    }
}

pub fn command_spec(id: CommandId) -> Option<CommandSpec> {
    catalog().into_iter().find(|spec| spec.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_one_unique_spec_for_every_command_id() {
        let specs = catalog();
        assert_eq!(specs.len(), CommandId::ALL.len());

        let mut ids = std::collections::HashSet::new();
        for spec in specs {
            assert!(ids.insert(spec.id), "duplicate command {}", spec.id);
            assert_eq!(command_spec(spec.id).map(|item| item.id), Some(spec.id));
        }
        for command in CommandId::ALL {
            assert!(ids.contains(command), "missing command {command}");
        }
    }
}
