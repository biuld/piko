use piko_protocol::{ApprovalDecision, CommandCatalogAction};

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
    Config(ConfigAction),
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
pub enum ConfigAction {
    SetThinkingLevel { level: String },
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
    Subscribe { task_id: String, agent_id: String },
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

impl From<ConfigAction> for Action {
    fn from(action: ConfigAction) -> Self {
        Self::Config(action)
    }
}

impl From<SlashAction> for Action {
    fn from(action: SlashAction) -> Self {
        Self::Slash(action)
    }
}

#[derive(Default)]
pub struct CommandActionArgs {
    pub fork_entry_id: Option<String>,
    pub provider: Option<String>,
    pub clear_notifications_and_close: bool,
}

pub fn action_for_command_catalog(
    action: &CommandCatalogAction,
    args: CommandActionArgs,
) -> Option<Action> {
    Some(match action {
        CommandCatalogAction::Help => SurfaceAction::OpenHelp.into(),
        CommandCatalogAction::Commands => EditorAction::OpenCommands.into(),
        CommandCatalogAction::Sessions => SessionAction::RequestList.into(),
        CommandCatalogAction::Models => ModelAction::RequestList.into(),
        CommandCatalogAction::Agents => AgentAction::RequestList.into(),
        CommandCatalogAction::Thinking => SurfaceAction::OpenThinking.into(),
        CommandCatalogAction::Tree => SurfaceAction::OpenTree.into(),
        CommandCatalogAction::Settings => SurfaceAction::OpenSettings.into(),
        CommandCatalogAction::Status => SurfaceAction::OpenStatus.into(),
        CommandCatalogAction::NewSession => SlashAction::New.into(),
        CommandCatalogAction::ForkSession => SlashAction::Fork(args.fork_entry_id).into(),
        CommandCatalogAction::CloneSession => SlashAction::Clone.into(),
        CommandCatalogAction::Login => SlashAction::Login(args.provider).into(),
        CommandCatalogAction::Logout => SlashAction::Logout(args.provider).into(),
        CommandCatalogAction::Compact => SlashAction::Compact.into(),
        CommandCatalogAction::SetThinking { level } => ConfigAction::SetThinkingLevel {
            level: level.clone(),
        }
        .into(),
        CommandCatalogAction::ToggleToolsExpanded => TimelineAction::ToggleToolsExpanded.into(),
        CommandCatalogAction::ClearNotifications => {
            if args.clear_notifications_and_close {
                NotificationAction::ClearAndClose.into()
            } else {
                NotificationAction::Clear.into()
            }
        }
        CommandCatalogAction::Quit => AppAction::Quit.into(),
        CommandCatalogAction::RenameSession
        | CommandCatalogAction::ImportSession
        | CommandCatalogAction::ExportSession
        | CommandCatalogAction::DeleteSession => return None,
    })
}
