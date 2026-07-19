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
    AgentList(AgentAction),
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
    ToggleToolsExpanded,
}

#[derive(Debug)]
pub enum SurfaceAction {
    OpenHelp,
    OpenSettings,
    OpenStatus,
    OpenTree,
    OpenThinking,
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
pub enum AgentAction {
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
    ClearAndClose,
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

impl From<AgentAction> for Action {
    fn from(action: AgentAction) -> Self {
        Self::AgentList(action)
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
// `docs/host-command-catalog-design.md`). The TUI keeps slash commands as a
// *local* mapping layer on top of that neutral catalog plus its own
// presentation-only commands. Slash strings never leave this module.

/// TUI-local presentation command ids. These are never sent to hostd as a
/// catalog id — hostd does not own Help/Settings/Tree/Models-opener/Quit/etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalCommandId {
    Help,
    Settings,
    Tree,
    Status,
    Sessions,
    Models,
    Thinking,
    ToolsToggle,
    ClearNotifications,
    Agents,
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
        "/help",
        LocalCommandId::Help,
        "Help",
        "Show keyboard shortcuts and slash commands",
    ),
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
        "/tools",
        LocalCommandId::ToolsToggle,
        "Toggle tool details",
        "Switch between folded and expanded tool result rendering",
    ),
    (
        "/clear",
        LocalCommandId::ClearNotifications,
        "Clear notifications",
        "Dismiss all notification messages",
    ),
    (
        "/agents",
        LocalCommandId::Agents,
        "Agents",
        "List available named agents and their capabilities",
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
        LocalCommandId::Help => SurfaceAction::OpenHelp.into(),
        LocalCommandId::Sessions => SessionAction::RequestList.into(),
        LocalCommandId::Models => ModelAction::RequestList.into(),
        LocalCommandId::Agents => AgentAction::RequestList.into(),
        LocalCommandId::Thinking => SurfaceAction::OpenThinking.into(),
        LocalCommandId::Tree => SurfaceAction::OpenTree.into(),
        LocalCommandId::Settings => SurfaceAction::OpenSettings.into(),
        LocalCommandId::Status => SurfaceAction::OpenStatus.into(),
        LocalCommandId::ToolsToggle => TimelineAction::ToggleToolsExpanded.into(),
        LocalCommandId::ClearNotifications => NotificationAction::ClearAndClose.into(),
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
        "session.clone" => SlashAction::Clone.into(),
        "auth.login" => SlashAction::Login(args.provider).into(),
        "auth.logout" => SlashAction::Logout(args.provider).into(),
        "session.compact" => SlashAction::Compact.into(),
        _ => return None,
    })
}
