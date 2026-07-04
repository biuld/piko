use ratatui::{Frame, layout::Rect};

use crate::{
    theme::Theme,
    app::AppState,
    ui::components::hierarchical_menu::{HierarchicalMenu, MenuNode, MenuConfirmResult},
};

/// Action applied when a settings option is confirmed.
#[derive(Clone, Debug)]
pub enum SettingsAction {
    Thinking(&'static str),
    HideThinking(bool),
    Compaction(bool),
    CompactionKeep(u64),
    CompactionReserve(u64),
    Theme(&'static str),
    Transport(&'static str),
    Sandbox(bool),
    Retry(bool),
    EnableAllTools,
    DisableTools,
}

pub type SettingsNode = MenuNode<SettingsAction>;
pub type SettingsConfirmResult = MenuConfirmResult<SettingsAction>;

/// Settings panel: hierarchical menu of runtime-configurable options.
pub struct SettingsPanel {
    pub menu: HierarchicalMenu<SettingsAction>,
}

impl SettingsPanel {
    pub fn new() -> Self {
        Self {
            menu: HierarchicalMenu::new(Self::build_settings_tree()),
        }
    }

    /// Build the default nested settings menu tree.
    fn build_settings_tree() -> SettingsNode {
        let group = |title: &str, detail: &str, children: Vec<SettingsNode>| {
            SettingsNode::Group {
                title: title.to_string(),
                detail: detail.to_string(),
                children,
            }
        };

        let action = |title: &str, detail: &str, act: SettingsAction| {
            SettingsNode::Action {
                title: title.to_string(),
                detail: detail.to_string(),
                action: act,
            }
        };

        group("settings", "Configure runtime parameters", vec![
            group("Thinking Level", "Select assistant reasoning and thinking budget", vec![
                action("off", "Disable assistant thinking/reasoning", SettingsAction::Thinking("off")),
                action("minimal", "Minimal reasoning budget", SettingsAction::Thinking("minimal")),
                action("low", "Low reasoning budget", SettingsAction::Thinking("low")),
                action("medium", "Medium reasoning budget", SettingsAction::Thinking("medium")),
                action("high", "High reasoning budget", SettingsAction::Thinking("high")),
                action("xhigh", "Extra high reasoning budget (maximum)", SettingsAction::Thinking("xhigh")),
            ]),
            group("Thinking Blocks", "Show or hide thinking content in future rendering", vec![
                action("Hide thinking blocks", "Hide thinking content in future rendering where supported", SettingsAction::HideThinking(true)),
                action("Show thinking blocks", "Show thinking content in future rendering where supported", SettingsAction::HideThinking(false)),
            ]),
            group("Automatic Compaction", "Configure compaction settings and reserve token sizes", vec![
                action("Enable compaction", "Enable hostd automatic compaction", SettingsAction::Compaction(true)),
                action("Disable compaction", "Disable hostd automatic compaction", SettingsAction::Compaction(false)),
                group("Reserve Tokens", "Set number of tokens to reserve for context window", vec![
                    action("Reserve 8k tokens", "Reserve 8,192 tokens for system context", SettingsAction::CompactionReserve(8192)),
                    action("Reserve 16k tokens (Default)", "Reserve 16,384 tokens for system context", SettingsAction::CompactionReserve(16384)),
                    action("Reserve 32k tokens", "Reserve 32,768 tokens for system context", SettingsAction::CompactionReserve(32768)),
                ]),
                group("Keep Recent Tokens", "Set number of recent tokens to preserve in full detail", vec![
                    action("Keep 10k tokens", "Keep 10,000 recent tokens intact", SettingsAction::CompactionKeep(10000)),
                    action("Keep 20k tokens (Default)", "Keep 20,000 recent tokens intact", SettingsAction::CompactionKeep(20000)),
                    action("Keep 30k tokens", "Keep 30,000 recent tokens intact", SettingsAction::CompactionKeep(30000)),
                    action("Keep 50k tokens", "Keep 50,000 recent tokens intact", SettingsAction::CompactionKeep(50000)),
                ]),
            ]),
            group("API Retries", "Enable/disable automatic API connection retries", vec![
                action("Enable retries", "Enable automatic retries on LLM API failure", SettingsAction::Retry(true)),
                action("Disable retries", "Disable automatic retries on LLM API failure", SettingsAction::Retry(false)),
            ]),
            group("Tool Sandbox", "Enable/disable sandboxed tool execution execution limits", vec![
                action("Enable sandbox", "Enable filesystem & shell sandboxing rules", SettingsAction::Sandbox(true)),
                action("Disable sandbox", "Disable filesystem & shell sandboxing rules", SettingsAction::Sandbox(false)),
            ]),
            group("UI Theme", "Set UI color theme preference", vec![
                action("Theme dark", "Set TUI theme to dark theme", SettingsAction::Theme("dark")),
                action("Theme light", "Set TUI theme to light theme", SettingsAction::Theme("light")),
            ]),
            group("Transport Preference", "Set host transport preference", vec![
                action("Transport stdio", "Set host transport preference to stdio", SettingsAction::Transport("stdio")),
            ]),
            group("Active Tools Mode", "Configure active tools allowance", vec![
                action("Enable all tools", "Reset and allow all discovered tools", SettingsAction::EnableAllTools),
                action("Disable all tools", "Set active tools to an empty list", SettingsAction::DisableTools),
            ]),
        ])
    }

    /// Reset the stack to contain only the root Settings menu.
    pub fn open_root(&mut self) {
        self.menu.open(Self::build_settings_tree());
    }

    /// Reset the stack to contain only the Thinking Level selection menu.
    pub fn open_thinking(&mut self) {
        self.menu.stack.clear();
        // Traverse settings tree to find the "Thinking Level" group and push it.
        if let SettingsNode::Group { children, .. } = Self::build_settings_tree() {
            for child in children {
                if child.title() == "Thinking Level" {
                    self.menu.push_node(child);
                    return;
                }
            }
        }
        // Fallback if not found: open root
        self.open_root();
    }

    pub fn pop(&mut self) -> bool {
        self.menu.pop()
    }

    pub fn select_next(&mut self, filter: &str) {
        self.menu.select_next(filter);
    }

    pub fn select_prev(&mut self, filter: &str) {
        self.menu.select_prev(filter);
    }

    pub fn confirm(&mut self, filter_text: &mut String) -> SettingsConfirmResult {
        self.menu.confirm(filter_text)
    }

    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        filter: &str,
        app: &AppState,
        theme: &Theme,
    ) {
        self.menu.render(
            frame,
            area,
            filter,
            |action| match action {
                SettingsAction::Thinking(level) => {
                    app.active_thinking_level.as_deref() == Some(*level)
                }
                SettingsAction::HideThinking(value) => {
                    app.timeline.thinking_visible != *value
                }
                SettingsAction::Theme(value) => app.tui_config.theme.name == *value,
                SettingsAction::EnableAllTools => !app.initial_options.no_tools,
                SettingsAction::DisableTools => app.initial_options.no_tools,
                _ => false,
            },
            theme,
        );
    }
}
