use piko_protocol::ApprovalDecision;
use piko_tui_layout::{ComponentHit, PointerGesture};

use crate::{
    app::HitId,
    navigation::{Region, SurfaceId},
};

#[path = "command/catalog.rs"]
mod catalog;
pub use catalog::*;

/// Root user intent. This is intentionally only a router over smaller intent
/// domains; feature-specific behavior should live in the nested action types.
#[derive(Debug)]
pub enum Action {
    App(AppAction),
    Editor(EditorAction),
    Timeline(TimelineAction),
    ThoughtInspector(ThoughtInspectorAction),
    Surface(SurfaceAction),
    Session(SessionAction),
    Model(ModelAction),
    Tree(TreeAction),
    Approval(ApprovalAction),
    ToolInteraction(ToolInteractionAction),
    Notifications(NotificationAction),
    Slash(SlashAction),
    AgentPanel(AgentPanelAction),
    Pointer(PointerAction),
}

#[derive(Debug)]
pub enum AppAction {
    Quit,
    /// Idle-editor Esc with the adapter timestamp used for double-Esc.
    IdleEscape(std::time::Instant),
}

#[derive(Clone, Copy, Debug)]
pub enum PointerTarget {
    Component {
        region: Region,
        hit: ComponentHit<HitId>,
    },
    OutsideModal(SurfaceId),
    None,
}

#[derive(Clone, Copy, Debug)]
pub enum PointerAction {
    LeftDown(PointerTarget),
    LeftUp(PointerTarget),
    Move(Option<(Region, Option<HitId>)>),
    Gesture {
        target: PointerTarget,
        gesture: PointerGesture,
    },
}

#[derive(Debug)]
pub enum EditorAction {
    Submit,
    Interrupt,
    Cancel,
    CancelSuggestions,
    InsertChar(char),
    InsertPaste(String),
    PasteImage,
    InsertImage {
        filename: String,
        data: String,
        mime_type: String,
    },
    ReplaceDraftWithImage {
        expected_text: String,
        filename: String,
        data: String,
        mime_type: String,
    },
    InsertNewline,
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    DeleteWordForward,
    DeleteToLineStart,
    DeleteToLineEnd,
    CursorLeft,
    CursorRight,
    CursorWordLeft,
    CursorWordRight,
    CursorLineStart,
    CursorLineEnd,
    HistoryPrev,
    HistoryNext,
    AcceptSuggestion,
    AcceptAndSubmitSuggestion,
    SuggestionSelectNext,
    SuggestionSelectPrev,
    FollowUp,
    Steer,
    DequeueFollowUp,
}

#[derive(Debug)]
pub enum TimelineAction {
    ScrollUp(usize),
    ScrollDown(usize),
    JumpLatest,
    /// Toggle one tool block by its stable interned hit id.
    ToggleTool(u64),
    /// Open the thought resolved from its stable Timeline hit id.
    OpenThought(u64),
    SelectionStart(crate::features::timeline::SelectionPoint),
    SelectionUpdate(crate::features::timeline::SelectionPoint),
    SelectionFinish {
        point: crate::features::timeline::SelectionPoint,
        activation: Option<TimelineActivation>,
    },
    CopySelection,
}

#[derive(Clone, Copy, Debug)]
pub enum TimelineActivation {
    Tool(u64),
    Thought(u64),
}

#[derive(Debug)]
pub enum ThoughtInspectorAction {
    ScrollUp(usize),
    ScrollDown(usize),
}

#[derive(Debug)]
pub enum SurfaceAction {
    OpenSettings,
    OpenTodos,
    TodoScrollUp(usize),
    TodoScrollDown(usize),
    OpenUsage,
    OpenNotifications,
    OpenHistory(Option<String>),
    HistoryLensPrevious,
    HistoryLensNext,
    HistorySelectLens(usize),
    HistoryRefresh,
    HistoryInspect,
    HistoryChooseSession,
    HistoryFilter,
    HistoryFactsOnly,
    HistoryDiagnostics,
    OpenTree,
    /// Session agent list → switch viewed agent (ComposerBand).
    OpenAgents,
    Close,
    SelectNext,
    SelectPrev,
    Confirm,
    FilterAppend(char),
    FilterBackspace,
}

#[derive(Debug)]
pub enum SessionAction {
    RequestList,
    ToggleScope,
    ToggleNamed,
    TogglePath,
}

#[derive(Debug)]
pub enum ModelAction {
    RequestList,
}

#[derive(Debug)]
pub enum TreeAction {
    FoldOrUp,
    UnfoldOrDown,
    EditLabel,
    ToggleLabelTimestamp,
    FilterCycleForward,
    FilterCycleBackward,
}

#[derive(Debug)]
pub enum ApprovalAction {
    /// Immediate decision (Esc decline, pointer click on a specific choice).
    Respond(ApprovalDecision),
    SelectNext,
    SelectPrev,
    /// Enter: confirm the currently selected grant.
    ConfirmSelected,
}

#[derive(Debug)]
pub enum ToolInteractionAction {
    Submit,
    Cancel,
    NextStep,
    PrevStep,
    GotoStep(usize),
    SelectNext,
    SelectPrev,
    Choice(usize),
}

#[derive(Debug)]
pub enum NotificationAction {
    DismissVisible,
    ToggleScope,
    SelectPrev,
    SelectNext,
    CopySelected,
    Copy(u64),
    ScrollUp(usize),
    ScrollDown(usize),
}

#[derive(Debug)]
pub enum SlashAction {
    New,
    Fork(Option<String>),
    Rename(String),
    Import(String),
    Delete,
    Login(Option<String>),
    Logout(Option<String>),
    Compact,
    ListProcesses,
    ListMcpStatus,
    RequestDiff,
}

#[derive(Debug)]
pub enum AgentPanelAction {
    Subscribe {
        agent_instance_id: String,
        agent_id: String,
    },
}

impl From<AgentPanelAction> for Action {
    fn from(action: AgentPanelAction) -> Self {
        Self::AgentPanel(action)
    }
}

impl From<PointerAction> for Action {
    fn from(action: PointerAction) -> Self {
        Self::Pointer(action)
    }
}

impl From<AppAction> for Action {
    fn from(action: AppAction) -> Self {
        Self::App(action)
    }
}

impl From<EditorAction> for Action {
    fn from(action: EditorAction) -> Self {
        Self::Editor(action)
    }
}

impl From<TimelineAction> for Action {
    fn from(action: TimelineAction) -> Self {
        Self::Timeline(action)
    }
}

impl From<ThoughtInspectorAction> for Action {
    fn from(action: ThoughtInspectorAction) -> Self {
        Self::ThoughtInspector(action)
    }
}

impl From<SurfaceAction> for Action {
    fn from(action: SurfaceAction) -> Self {
        Self::Surface(action)
    }
}

impl From<SessionAction> for Action {
    fn from(action: SessionAction) -> Self {
        Self::Session(action)
    }
}

impl From<ModelAction> for Action {
    fn from(action: ModelAction) -> Self {
        Self::Model(action)
    }
}

impl From<TreeAction> for Action {
    fn from(action: TreeAction) -> Self {
        Self::Tree(action)
    }
}

impl From<ApprovalAction> for Action {
    fn from(action: ApprovalAction) -> Self {
        Self::Approval(action)
    }
}

impl From<ToolInteractionAction> for Action {
    fn from(action: ToolInteractionAction) -> Self {
        Self::ToolInteraction(action)
    }
}

impl From<NotificationAction> for Action {
    fn from(action: NotificationAction) -> Self {
        Self::Notifications(action)
    }
}

impl From<SlashAction> for Action {
    fn from(action: SlashAction) -> Self {
        Self::Slash(action)
    }
}
