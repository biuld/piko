//! Keyboard traversal and sidebar list routing.

use super::*;
use island::components::list::ListKeyIntent;
use island::components::source_list::{SourceListEffect, apply_source_list_intent};

impl Shell {
    pub(super) fn activate_nav(&mut self, id: sidebar::NavId, cx: &mut Context<Self>) {
        let intents = match id {
            sidebar::NavId::NewSession => vec![ClientIntent::CreateSession {
                cwd: self.workspace_cwd.clone(),
            }],
            sidebar::NavId::Session(index) => self
                .state
                .core
                .session_list
                .sessions
                .get(index)
                .cloned()
                .map(|summary| {
                    let already_live = self
                        .state
                        .core
                        .live_session
                        .as_ref()
                        .is_some_and(|session| session.session_id == summary.session_id);
                    if already_live {
                        Vec::new()
                    } else {
                        self.open_session(summary.session_id, summary.session_path)
                    }
                })
                .unwrap_or_default(),
            sidebar::NavId::Agent(index) => self
                .state
                .core
                .live_session
                .as_ref()
                .and_then(|live| live.agents.get(index))
                .map(|agent| agent.agent_instance_id.clone())
                .map(|agent_instance_id| {
                    let already_selected = self
                        .state
                        .core
                        .live_session
                        .as_ref()
                        .and_then(|session| session.selected_agent.as_deref())
                        == Some(agent_instance_id.as_str());
                    if already_selected {
                        return Vec::new();
                    }
                    self.pending_agent = Some(agent_instance_id.clone());
                    self.subscribed_agent = Some(agent_instance_id.clone());
                    self.selection_error = None;
                    vec![ClientIntent::SelectAgent { agent_instance_id }]
                })
                .unwrap_or_default(),
            sidebar::NavId::Settings => {
                self.open_layer(LayerKind::Settings, FocusOwner::Sidebar, cx);
                Vec::new()
            }
        };
        self.narrow_overlay_open = false;
        self.focus_owner = FocusOwner::Sidebar;
        self.dispatch_intents(cx, intents);
    }

    pub(super) fn handle_shell_key(
        &mut self,
        event: &KeyDownEvent,
        narrow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "escape" {
            if self.layers.active().is_some() {
                self.close_layer(window, cx);
            } else if self.narrow_overlay_open {
                self.narrow_overlay_open = false;
                cx.notify();
            }
            return;
        }
        if event.keystroke.key == "tab" {
            self.focus_owner = self.focus_owner.next();
            if self.focus_owner == FocusOwner::Composer {
                self.composer_input
                    .update(cx, |input, cx| input.focus(window, cx));
            } else if self.focus_owner == FocusOwner::Sidebar && narrow {
                self.narrow_overlay_open = true;
            }
            cx.notify();
            return;
        }
        if self.focus_owner != FocusOwner::Sidebar || self.layers.active().is_some() {
            return;
        }
        let intent = match event.keystroke.key.as_str() {
            "up" => Some(ListKeyIntent::Prev),
            "down" => Some(ListKeyIntent::Next),
            "home" => Some(ListKeyIntent::Home),
            "end" => Some(ListKeyIntent::End),
            "enter" | "space" => Some(ListKeyIntent::Activate),
            _ => None,
        };
        let Some(intent) = intent else {
            return;
        };
        let model = sidebar::nav_model(&self.state.core, self.sidebar_keyboard_focused);
        match apply_source_list_intent(&model.sections, &mut self.sidebar_keyboard, intent) {
            SourceListEffect::CursorMoved { id } => {
                self.sidebar_keyboard_focused = Some(id);
                sidebar::reveal_keyboard_focus(&self.sidebar_scroll, &model, id);
            }
            SourceListEffect::Activate { id } => self.activate_nav(id, cx),
            SourceListEffect::None => {}
        }
        cx.notify();
    }
}
