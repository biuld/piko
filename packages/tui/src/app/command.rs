use piko_protocol::{ApprovalDecision, HostCommandDescriptor, HostCommandInvoke};
use piko_tui_layout::{ComponentHit, PointerGesture};

use crate::{
    app::HitId,
    navigation::{Region, SurfaceId},
};

/// Root user intent. This is intentionally only a router over smaller intent
/// domains; feature-specific behavior should live in the nested action types.
#[derive(Debug)]
pub enum Action {
    App(AppAction),
    Editor(EditorAction),
    Timeline(TimelineAction),
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
    Cancel,
    CancelSuggestions,
    InsertChar(char),
    InsertPaste(String),
    InsertNewline,
    DeleteBackward,
    DeleteForward,
    CursorLeft,
    CursorRight,
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
    ToggleTool(usize),
}

#[derive(Debug)]
pub enum SurfaceAction {
    OpenSettings,
    OpenUsage,
    OpenNotifications,
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

// ── Command catalog adapter ─────────────────────────────────────────────────
//
// hostd's catalog (`HostCommandDescriptor`) is frontend-neutral: id + title +
// detail + invoke kind only, no slash names (see
// `packages/protocol/src/command_catalog.rs`). The TUI keeps slash commands as a
// *local* mapping layer on top of that neutral catalog plus its own
// presentation-only commands. Slash strings never leave this module.

/// TUI-local presentation command ids. These are never sent to hostd as a
/// catalog id — hostd does not own Settings/Tree/Models-opener/Quit/etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalCommandId {
    Settings,
    Tree,
    Usage,
    Notifications,
    Sessions,
    Models,
    Agents,
    /// Open last/active turn workspace diff.
    Diff,
    Quit,
}

/// Where a merged command row's confirm/slash-submit should be routed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandTarget {
    /// TUI-local presentation command.
    Local(LocalCommandId),
    /// Neutral host catalog id, e.g. `"session.new"`.
    Host(String),
}

/// One slash-addressable row in the merged TUI command list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiCommandEntry {
    pub slash: String,
    pub title: String,
    pub detail: String,
    pub invoke: HostCommandInvoke,
    pub target: CommandTarget,
}

/// Local slash aliases and their target — always present, independent of
/// what hostd advertises.
const LOCAL_SLASH_TABLE: &[(&str, LocalCommandId, &str, &str)] = &[
    (
        "/resume",
        LocalCommandId::Sessions,
        "Sessions",
        "List and open hostd sessions",
    ),
    (
        "/tree",
        LocalCommandId::Tree,
        "Session tree",
        "Inspect and navigate the current session branch tree",
    ),
    (
        "/model",
        LocalCommandId::Models,
        "Model",
        "Select a model and compatible thinking level",
    ),
    (
        "/settings",
        LocalCommandId::Settings,
        "Settings",
        "Open hostd-backed runtime settings",
    ),
    (
        "/usage",
        LocalCommandId::Usage,
        "Usage",
        "Show per-agent time, token, and cost totals",
    ),
    (
        "/noti",
        LocalCommandId::Notifications,
        "Notifications",
        "Show the in-memory notification queue",
    ),
    (
        "/agents",
        LocalCommandId::Agents,
        "Agents",
        "List agents in the current session and switch the viewed agent",
    ),
    (
        "/diff",
        LocalCommandId::Diff,
        "Turn diff",
        "Show workspace diff for the last or active turn",
    ),
    ("/quit", LocalCommandId::Quit, "Quit", "Exit the TUI"),
];

/// TUI-chosen slash aliases for neutral host catalog ids. A host id only
/// becomes slash-addressable once hostd actually advertises it.
const HOST_SLASH_TABLE: &[(&str, &str)] = &[
    ("/new", "session.new"),
    ("/fork", "session.fork"),
    ("/rename", "session.rename"),
    ("/import", "session.import"),
    ("/delete", "session.delete"),
    ("/login", "auth.login"),
    ("/logout", "auth.logout"),
    ("/compact", "session.compact"),
    ("/top", "process.list"),
    ("/mcp", "mcp.status"),
];

/// Merge TUI-local presentation commands with the fetched neutral host
/// catalog into one slash-addressable list.
pub fn merge_command_catalog(host: &[HostCommandDescriptor]) -> Vec<TuiCommandEntry> {
    let mut entries: Vec<TuiCommandEntry> = LOCAL_SLASH_TABLE
        .iter()
        .map(|(slash, id, title, detail)| TuiCommandEntry {
            slash: slash.to_string(),
            title: title.to_string(),
            detail: detail.to_string(),
            invoke: HostCommandInvoke::Immediate,
            target: CommandTarget::Local(*id),
        })
        .collect();
    for (slash, id) in HOST_SLASH_TABLE {
        if let Some(descriptor) = host.iter().find(|d| d.id == *id) {
            entries.push(TuiCommandEntry {
                slash: slash.to_string(),
                title: descriptor.title.clone(),
                detail: descriptor.detail.clone(),
                invoke: descriptor.invoke.clone(),
                target: CommandTarget::Host(descriptor.id.clone()),
            });
        }
    }
    entries
}

/// Extra arguments a host command may need beyond its slash text (resolved
/// locally by the TUI: current tree selection, active provider, ...).
#[derive(Default)]
pub struct HostCommandArgs {
    pub fork_entry_id: Option<String>,
    pub provider: Option<String>,
}

/// Always-available mapping for a TUI-local presentation command.
pub fn action_for_local_command(id: LocalCommandId) -> Action {
    match id {
        LocalCommandId::Sessions => SessionAction::RequestList.into(),
        LocalCommandId::Models => ModelAction::RequestList.into(),
        LocalCommandId::Agents => SurfaceAction::OpenAgents.into(),
        LocalCommandId::Tree => SurfaceAction::OpenTree.into(),
        LocalCommandId::Settings => SurfaceAction::OpenSettings.into(),
        LocalCommandId::Usage => SurfaceAction::OpenUsage.into(),
        LocalCommandId::Notifications => SurfaceAction::OpenNotifications.into(),
        LocalCommandId::Diff => SlashAction::RequestDiff.into(),
        LocalCommandId::Quit => AppAction::Quit.into(),
    }
}

/// Mapping for neutral host ids that need no dedicated argument parsing
/// beyond `HostCommandArgs`. Ids with bespoke text parsing (rename, import,
/// delete-confirm) are handled directly in `slash.rs`.
pub fn action_for_host_command(id: &str, args: HostCommandArgs) -> Option<Action> {
    Some(match id {
        "session.new" => SlashAction::New.into(),
        "session.fork" => SlashAction::Fork(args.fork_entry_id).into(),
        "process.list" => SlashAction::ListProcesses.into(),
        "mcp.status" => SlashAction::ListMcpStatus.into(),
        "auth.login" => SlashAction::Login(args.provider).into(),
        "auth.logout" => SlashAction::Logout(args.provider).into(),
        "session.compact" => SlashAction::Compact.into(),
        _ => return None,
    })
}
