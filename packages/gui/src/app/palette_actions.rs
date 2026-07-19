//! Map Command Catalog / Palette submenu selections → DesktopApp intents.

use gpui::*;
use gpui_component::WindowExt;
use gpui_component::notification::{Notification, NotificationType};
use piko_client_core::ClientIntent;
use piko_protocol::CommandCatalogAction;

use crate::app::model_cycle::{THINKING_LEVELS, catalog_models};
use crate::chrome::IslandId;
use crate::chrome::overlay::palette::{PaletteConfirmResult, palette_runnable};

use super::desktop_app::DesktopApp;

impl DesktopApp {
    pub(crate) fn run_selected_palette_command(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(palette) = self.command_palette.clone() else {
            return;
        };

        // Submenu entry from root (Models / Thinking) before confirm consumes StayOpen.
        if let Some(action) = palette.read(cx).selected_root_action() {
            match action {
                CommandCatalogAction::Models => {
                    if self.bridge_state().model.providers.is_empty() {
                        self.bridge.intent(ClientIntent::ListModels);
                    }
                    let models = catalog_models(&self.bridge_state().model.providers);
                    palette.update(cx, |p, cx| p.push_models(models, window, cx));
                    cx.notify();
                    return;
                }
                CommandCatalogAction::Thinking => {
                    palette.update(cx, |p, cx| p.push_thinking(THINKING_LEVELS, window, cx));
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }

        let result = palette.update(cx, |p, _| p.confirm());
        match result {
            PaletteConfirmResult::None | PaletteConfirmResult::StayOpen => {}
            PaletteConfirmResult::RunCatalog(action) => {
                self.run_catalog_action(action, window, cx);
            }
            PaletteConfirmResult::SetModel { provider, model_id } => {
                self.overlay.close_transient();
                self.restore_overlay_focus(window, cx);
                self.bridge
                    .intent(ClientIntent::SetModel { provider, model_id });
                cx.notify();
            }
            PaletteConfirmResult::SetThinking(level) => {
                self.overlay.close_transient();
                self.restore_overlay_focus(window, cx);
                self.bridge.intent(ClientIntent::SetThinkingLevel { level });
                cx.notify();
            }
        }
    }

    fn run_catalog_action(
        &mut self,
        action: CommandCatalogAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(action, CommandCatalogAction::Commands) {
            return;
        }
        if !palette_runnable(&action) {
            return;
        }

        self.overlay.close_transient();
        self.restore_overlay_focus(window, cx);

        match action {
            CommandCatalogAction::Sessions => {
                self.focus_or_open_sessions(window, cx);
            }
            CommandCatalogAction::Agents | CommandCatalogAction::Tree => {
                self.focus_or_open_right_column(window, cx);
            }
            CommandCatalogAction::Quit => {
                self.request_quit_from_palette(window, cx);
            }
            CommandCatalogAction::ClearNotifications => {
                window.clear_notifications(cx);
            }
            CommandCatalogAction::NewSession => {
                window.push_notification(
                    Notification::new()
                        .title(crate::t!("palette.new_session.title"))
                        .message(crate::t!("palette.new_session.message"))
                        .with_type(NotificationType::Info)
                        .autohide(true),
                    cx,
                );
                self.focus_or_open_sessions(window, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    fn focus_or_open_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let live = self.bridge_state().is_live();
        if self.layout.is_docked_visible(IslandId::Sessions, live) {
            self.focus_island(IslandId::Sessions, window, cx);
            return;
        }
        if !self.layout.sessions_open {
            self.layout.set_open(IslandId::Sessions, true);
            self.persist_gui_config();
        }
        if self.layout.is_docked_visible(IslandId::Sessions, live) {
            self.focus_island(IslandId::Sessions, window, cx);
            cx.notify();
        } else {
            self.open_sessions_sheet(window, cx);
        }
    }

    fn focus_or_open_right_column(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let live = self.bridge_state().is_live();
        if self.layout.any_right_column_docked(live) {
            self.focus_island(IslandId::Agents, window, cx);
            return;
        }
        if !self.layout.right_column_pref_open() {
            self.layout.set_right_column_open(true);
            self.persist_gui_config();
        }
        if self.layout.any_right_column_docked(live) {
            self.focus_island(IslandId::Agents, window, cx);
            cx.notify();
        } else {
            self.open_right_column_sheet(window, cx);
        }
    }

    fn request_quit_from_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if super::quit::is_quit_busy(self.bridge_state()) {
            self.request_busy_quit_confirm(window, cx);
        } else {
            cx.quit();
        }
    }
}
