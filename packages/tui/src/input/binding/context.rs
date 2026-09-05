use std::fmt;

use crate::{
    app::{AppMode, AppState, SurfaceId},
    terminal::TerminalProfile,
};

/// Ordered scope names accepted by the settings schema.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ScopeKind {
    Application,
    Workspace,
    Timeline,
    Editor,
    Suggestions,
    Selection,
    Tree,
    Sessions,
    Models,
    Agents,
    Thinking,
    Settings,
    Auth,
    Diagnostics,
    History,
    Todos,
    Usage,
    Notifications,
    ThoughtInspector,
    Mcp,
    Processes,
    Approval,
    ToolInteraction,
    Summary,
}

impl ScopeKind {
    pub const ALL: &'static [Self] = &[
        Self::Application,
        Self::Workspace,
        Self::Timeline,
        Self::Editor,
        Self::Suggestions,
        Self::Selection,
        Self::Tree,
        Self::Sessions,
        Self::Models,
        Self::Agents,
        Self::Thinking,
        Self::Settings,
        Self::Auth,
        Self::Diagnostics,
        Self::History,
        Self::Todos,
        Self::Usage,
        Self::Notifications,
        Self::ThoughtInspector,
        Self::Mcp,
        Self::Processes,
        Self::Approval,
        Self::ToolInteraction,
        Self::Summary,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::Workspace => "workspace",
            Self::Timeline => "timeline",
            Self::Editor => "editor",
            Self::Suggestions => "suggestions",
            Self::Selection => "selection",
            Self::Tree => "tree",
            Self::Sessions => "sessions",
            Self::Models => "models",
            Self::Agents => "agents",
            Self::Thinking => "thinking",
            Self::Settings => "settings",
            Self::Auth => "auth",
            Self::Diagnostics => "diagnostics",
            Self::History => "history",
            Self::Todos => "todos",
            Self::Usage => "usage",
            Self::Notifications => "notifications",
            Self::ThoughtInspector => "thought_inspector",
            Self::Mcp => "mcp",
            Self::Processes => "processes",
            Self::Approval => "approval",
            Self::ToolInteraction => "tool_interaction",
            Self::Summary => "summary",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase();
        Self::ALL
            .iter()
            .copied()
            .find(|scope| scope.as_str() == value)
    }

    pub const fn for_surface(surface: SurfaceId) -> Self {
        match surface {
            SurfaceId::Tree => Self::Tree,
            SurfaceId::Sessions => Self::Sessions,
            SurfaceId::Models => Self::Models,
            SurfaceId::Agents => Self::Agents,
            SurfaceId::Thinking => Self::Thinking,
            SurfaceId::Settings => Self::Settings,
            SurfaceId::AuthSelector => Self::Auth,
            SurfaceId::Diagnostics => Self::Diagnostics,
            SurfaceId::History => Self::History,
            SurfaceId::Todos => Self::Todos,
            SurfaceId::Usage => Self::Usage,
            SurfaceId::Notifications => Self::Notifications,
            SurfaceId::ThoughtInspector => Self::ThoughtInspector,
            SurfaceId::Mcp => Self::Mcp,
            SurfaceId::Processes => Self::Processes,
            SurfaceId::Approval => Self::Approval,
            SurfaceId::ToolInteraction => Self::ToolInteraction,
            SurfaceId::SummaryPrompt => Self::Summary,
        }
    }
}

impl fmt::Display for ScopeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Propagation {
    Continue,
    Stop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextSink {
    Editor,
    Surface,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveScope {
    pub kind: ScopeKind,
    pub propagation: Propagation,
    pub text_sink: Option<TextSink>,
}

impl ActiveScope {
    pub const fn continue_at(kind: ScopeKind, text_sink: Option<TextSink>) -> Self {
        Self {
            kind,
            propagation: Propagation::Continue,
            text_sink,
        }
    }

    pub const fn blocking(kind: ScopeKind, text_sink: Option<TextSink>) -> Self {
        Self {
            kind,
            propagation: Propagation::Stop,
            text_sink,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScopeStack {
    scopes: Vec<ActiveScope>,
}

impl ScopeStack {
    pub fn new(scopes: Vec<ActiveScope>) -> Self {
        Self { scopes }
    }

    pub fn iter(&self) -> impl Iterator<Item = &ActiveScope> {
        self.scopes.iter()
    }

    pub fn text_sink(&self) -> Option<TextSink> {
        self.scopes.iter().find_map(|scope| scope.text_sink)
    }

    #[allow(dead_code)]
    pub fn kinds(&self) -> impl Iterator<Item = ScopeKind> + '_ {
        self.scopes.iter().map(|scope| scope.kind)
    }
}

/// Closed, typed facts captured once for one input event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingContext {
    pub active_mode: AppMode,
    pub editor_empty: bool,
    pub editor_multiline: bool,
    pub history_browsing: bool,
    pub suggest_visible: bool,
    pub turn_running: bool,
    pub agent_running: bool,
    pub notice_visible: bool,
    pub terminal_enhanced: bool,
    pub text_input_active: bool,
    pub timeline_selection_active: bool,
}

impl BindingContext {
    pub fn from_app(app: &AppState, profile: &TerminalProfile) -> Self {
        Self {
            active_mode: app.mode(),
            editor_empty: app.editor.is_empty(),
            editor_multiline: app.tui_config.editor.multiline,
            history_browsing: app.editor.is_browsing_history(),
            suggest_visible: app.has_suggestions(),
            turn_running: app.viewed_agent_is_busy(),
            agent_running: app.viewed_agent_is_running(),
            notice_visible: app
                .notifications
                .row_visible_for(
                    app.last_tick,
                    app.session.id.as_deref(),
                    app.agent_panel.active_agent_instance_id.as_deref(),
                )
                .is_some(),
            terminal_enhanced: profile.key_reachability.enhanced_keyboard,
            text_input_active: app.active_text_box_is_present()
                || (app.mode() == AppMode::Surface(SurfaceId::History)
                    && app.history.filter_editing),
            timeline_selection_active: app.timeline().has_selection(),
        }
    }

    pub fn has(&self, atom: ContextAtom) -> bool {
        match atom {
            ContextAtom::EditorEmpty => self.editor_empty,
            ContextAtom::EditorMultiline => self.editor_multiline,
            ContextAtom::HistoryBrowsing => self.history_browsing,
            ContextAtom::SuggestVisible => self.suggest_visible,
            ContextAtom::TurnRunning => self.turn_running,
            ContextAtom::AgentRunning => self.agent_running,
            ContextAtom::NoticeVisible => self.notice_visible,
            ContextAtom::TerminalEnhancedKeyboard => self.terminal_enhanced,
            ContextAtom::TextInputActive => self.text_input_active,
            ContextAtom::TimelineSelectionActive => self.timeline_selection_active,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ContextAtom {
    EditorEmpty,
    EditorMultiline,
    HistoryBrowsing,
    SuggestVisible,
    TurnRunning,
    AgentRunning,
    NoticeVisible,
    TerminalEnhancedKeyboard,
    TextInputActive,
    TimelineSelectionActive,
}

impl ContextAtom {
    pub fn parse(value: &str) -> Option<(Self, bool)> {
        let (negated, name) = value
            .trim()
            .strip_prefix('!')
            .map_or((false, value.trim()), |value| (true, value.trim()));
        let atom = match name {
            "editor.empty" => Self::EditorEmpty,
            "editor.multiline" => Self::EditorMultiline,
            "editor.historyBrowsing" => Self::HistoryBrowsing,
            "suggest.visible" => Self::SuggestVisible,
            "turn.running" => Self::TurnRunning,
            "agent.running" => Self::AgentRunning,
            "notice.visible" => Self::NoticeVisible,
            "terminal.enhancedKeyboard" => Self::TerminalEnhancedKeyboard,
            "text.inputActive" => Self::TextInputActive,
            "timeline.selectionActive" => Self::TimelineSelectionActive,
            _ => return None,
        };
        Some((atom, negated))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Condition {
    pub atom: ContextAtom,
    pub negated: bool,
}

impl Condition {
    pub fn matches(self, context: &BindingContext) -> bool {
        context.has(self.atom) != self.negated
    }
}

/// Build the ordered product scope stack. A blocking workflow intentionally
/// has no parent scope, so neither editor commands nor text can leak through.
pub fn active_scope_stack(app: &AppState) -> ScopeStack {
    if let Some(surface) = app.pending_decide() {
        return ScopeStack::new(vec![ActiveScope::blocking(
            ScopeKind::for_surface(surface),
            if matches!(surface, SurfaceId::ToolInteraction) && app.active_text_box_is_present() {
                Some(TextSink::Surface)
            } else {
                None
            },
        )]);
    }

    match app.mode() {
        AppMode::Chat => {
            let mut scopes = Vec::new();
            if app.has_suggestions() {
                scopes.push(ActiveScope::continue_at(ScopeKind::Suggestions, None));
            }
            scopes.push(ActiveScope::continue_at(
                ScopeKind::Editor,
                Some(TextSink::Editor),
            ));
            scopes.push(ActiveScope::continue_at(ScopeKind::Timeline, None));
            scopes.push(ActiveScope::continue_at(ScopeKind::Workspace, None));
            scopes.push(ActiveScope::continue_at(ScopeKind::Application, None));
            ScopeStack::new(scopes)
        }
        AppMode::Surface(surface) => {
            let kind = ScopeKind::for_surface(surface);
            let sink = matches!(
                surface,
                SurfaceId::Sessions
                    | SurfaceId::Tree
                    | SurfaceId::Models
                    | SurfaceId::Agents
                    | SurfaceId::Thinking
                    | SurfaceId::Settings
                    | SurfaceId::AuthSelector
            )
            .then_some(TextSink::Surface)
            .or_else(|| {
                (surface == SurfaceId::History && app.history.filter_editing)
                    .then_some(TextSink::Surface)
            })
            .or_else(|| {
                app.active_text_box_is_present()
                    .then_some(TextSink::Surface)
            });
            ScopeStack::new(vec![
                ActiveScope::continue_at(kind, sink),
                ActiveScope::continue_at(ScopeKind::Selection, None),
                ActiveScope::continue_at(ScopeKind::Workspace, None),
                ActiveScope::continue_at(ScopeKind::Application, None),
            ])
        }
    }
}
