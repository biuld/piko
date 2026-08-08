use piko_protocol::Command;

use crate::app::{
    AppMode, AppState, SurfaceId,
    command::{
        Action, AgentAction, AppAction, ApprovalAction, EditorAction, ModelAction,
        NotificationAction, SessionAction, SlashAction, SurfaceAction, TimelineAction,
        ToolInteractionAction, TreeAction,
    },
    command_id,
    effect::Effect,
};

mod actions;
mod selection;
mod slash;

impl AppState {
    // ── action dispatch ───────────────────────────────────────────────────────

    /// Main entry point for all intents.  Called from `main.rs` after
    /// translating raw key events or surface selections into `Action` values.
    pub fn dispatch(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::App(action) => self.dispatch_app_action(action),
            Action::Editor(action) => self.dispatch_editor_action(action),
            Action::Timeline(action) => self.dispatch_timeline_action(action),
            Action::Surface(action) => self.dispatch_surface_action(action),
            Action::Session(action) => self.dispatch_session_action(action),
            Action::Model(action) => self.dispatch_model_action(action),
            Action::AgentList(action) => self.dispatch_agent_list_action(action),
            Action::Tree(action) => self.dispatch_tree_action(action),
            Action::Approval(action) => self.dispatch_approval_action(action),
            Action::ToolInteraction(action) => self.dispatch_tool_interaction_action(action),
            Action::Notifications(action) => self.dispatch_notification_action(action),
            Action::Slash(action) => self.dispatch_slash_action(action),
            Action::AgentPanel(action) => self.dispatch_agent_panel_action(action),
        }
    }

    fn dispatch_app_action(&mut self, action: AppAction) -> Vec<Effect> {
        match action {
            AppAction::Quit => self.quit = true,
        }
        Vec::new()
    }
}
