use piko_protocol::{ApprovalDecision, HostCommandDescriptor};

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
}

#[derive(Debug)]
pub enum AppAction {
    Quit,
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
    OpenCommands,
}

#[derive(Debug)]
pub enum TimelineAction {
    ScrollUp(usize),
    ScrollDown(usize),
    JumpLatest,
}

#[derive(Debug)]
pub enum SurfaceAction {
    OpenSettings,
    OpenStatus,
    OpenTree,
    OpenThinking,
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
    Respond(ApprovalDecision),
}

#[derive(Debug)]
pub enum ToolInteractionAction {
    Submit,
    Cancel,
    NextStep,
    PrevStep,
    Choice(usize),
}

#[derive(Debug)]
pub enum NotificationAction {
    Clear,
}

#[derive(Debug)]
pub enum SlashAction {
    New,
    Fork(Option<String>),
    Clone,
    Rename(String),
    Import(String),
    Delete,
    Login(Option<String>),
    Logout(Option<String>),
    Compact,
    ListProcesses,
    KillProcess(String),
    ListMcpStatus,
    RequestDiff,
    RequestPromptDebug,
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
    Status,
    Sessions,
    Models,
    Thinking,
    Clear,
    Agents,
    /// Open last/active turn workspace diff.
    Diff,
    /// Fetch latest prompt-assembly debug snapshot.
    PromptDebug,
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
        "/models",
        LocalCommandId::Models,
        "Models",
        "List and set default model",
    ),
    (
        "/settings",
        LocalCommandId::Settings,
        "Settings",
        "Open hostd-backed runtime settings",
    ),
    (
        "/status",
        LocalCommandId::Status,
        "Status",
        "Show turn, queue, approval, and tool state",
    ),
    (
        "/thinking",
        LocalCommandId::Thinking,
        "Thinking level",
        "List and set default thinking/reasoning level",
    ),
    (
        "/clear",
        LocalCommandId::Clear,
        "Clear session",
        "Start a new session and clear the timeline",
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
    (
        "/prompt-debug",
        LocalCommandId::PromptDebug,
        "Prompt debug",
        "Show latest prompt assembly and model-input diagnostics",
    ),
    ("/quit", LocalCommandId::Quit, "Quit", "Exit the TUI"),
];

/// TUI-chosen slash aliases for neutral host catalog ids. A host id only
/// becomes slash-addressable once hostd actually advertises it.
const HOST_SLASH_TABLE: &[(&str, &str)] = &[
    ("/new", "session.new"),
    ("/fork", "session.fork"),
    ("/clone", "session.clone"),
    ("/rename", "session.rename"),
    ("/import", "session.import"),
    ("/export", "session.export"),
    ("/delete", "session.delete"),
    ("/login", "auth.login"),
    ("/logout", "auth.logout"),
    ("/compact", "session.compact"),
    ("/ps", "process.list"),
    ("/kill", "process.stop"),
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
            target: CommandTarget::Local(*id),
        })
        .collect();
    for (slash, id) in HOST_SLASH_TABLE {
        if let Some(descriptor) = host.iter().find(|d| d.id == *id) {
            entries.push(TuiCommandEntry {
                slash: slash.to_string(),
                title: descriptor.title.clone(),
                detail: descriptor.detail.clone(),
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
        LocalCommandId::Thinking => SurfaceAction::OpenThinking.into(),
        LocalCommandId::Tree => SurfaceAction::OpenTree.into(),
        LocalCommandId::Settings => SurfaceAction::OpenSettings.into(),
        LocalCommandId::Status => SurfaceAction::OpenStatus.into(),
        LocalCommandId::Clear => SlashAction::New.into(),
        LocalCommandId::Diff => SlashAction::RequestDiff.into(),
        LocalCommandId::PromptDebug => SlashAction::RequestPromptDebug.into(),
        LocalCommandId::Quit => AppAction::Quit.into(),
    }
}

/// Mapping for neutral host ids that need no dedicated argument parsing
/// beyond `HostCommandArgs`. Ids with bespoke text parsing (rename, import,
/// delete-confirm, export) are handled directly in `slash.rs`.
pub fn action_for_host_command(id: &str, args: HostCommandArgs) -> Option<Action> {
    Some(match id {
        "session.new" => SlashAction::New.into(),
        "session.fork" => SlashAction::Fork(args.fork_entry_id).into(),
        "process.list" => SlashAction::ListProcesses.into(),
        "mcp.status" => SlashAction::ListMcpStatus.into(),
        "session.clone" => SlashAction::Clone.into(),
        "auth.login" => SlashAction::Login(args.provider).into(),
        "auth.logout" => SlashAction::Logout(args.provider).into(),
        "session.compact" => SlashAction::Compact.into(),
        _ => return None,
    })
}
