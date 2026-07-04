use piko_protocol::ApprovalDecision;

/// All intents that can be produced by key events, slash commands, or palette
/// selections. `main.rs` translates `KeyAction` → `Action`, and `AppState`
/// dispatches on `Action` without knowing which surface or key triggered it.
#[derive(Debug)]
pub enum Action {
    // ── lifecycle ───────────────────────────────────────────────────────────
    Quit,

    // ── turn / chat ─────────────────────────────────────────────────────────
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

    // ── timeline ────────────────────────────────────────────────────────────
    TimelineScrollUp(usize),
    TimelineScrollDown(usize),
    TimelineJumpLatest,

    // ── surface navigation ──────────────────────────────────────────────────
    OpenHelp,
    OpenCommands,
    OpenSettings,
    OpenStatus,
    OpenTree,
    RequestSessions,
    RequestModels,
    OpenThinking,
    CloseSurface,

    // ── list selection (used by all overlay surfaces) ────────────────────────
    SelectNext,
    SelectPrev,
    ConfirmSelection,
    FilterAppend(char),
    FilterBackspace,
    SessionToggleScope,
    SessionToggleNamed,
    SessionTogglePath,

    // ── tree ───────────────────────────────────────────────────────────────
    TreeFoldOrUp,
    TreeUnfoldOrDown,
    TreeEditLabel,
    TreeToggleLabelTimestamp,
    TreeFilterCycleForward,
    TreeFilterCycleBackward,

    // ── approval ─────────────────────────────────────────────────────────────
    ApprovalRespond(ApprovalDecision),

    // ── tool interaction ─────────────────────────────────────────────────────
    ToolInteractionSubmit,
    ToolInteractionCancel,
    ToolInteractionNextStep,
    ToolInteractionPrevStep,
    ToolInteractionChoice(usize),

    // ── notifications ────────────────────────────────────────────────────────
    ClearNotifications,

    // ── slash-command actions (produced by try_slash_command) ────────────────
    SlashNew,
    SlashFork(Option<String>),
    SlashClone,
    SlashRename(String),
    SlashImport(String),
    SlashDelete,
    SlashLogin(Option<String>),
    SlashLogout(Option<String>),
    SlashCompact,
}
