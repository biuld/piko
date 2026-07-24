//! Map Command Palette selections → DesktopApp behavior.
//!
//! Two id spaces meet here (see `docs/host-command-catalog-design.md`):
//! - host catalog ids (`RunHost`) map to a real `Command` / `ClientIntent`
//! - GUI-local ids (`RunLocal`) map to frontend-only presentation behavior
//!   (focus/dock a panel, open Settings, quit, clear notifications)

use gpui::*;
use piko_client_core::ClientIntent;

use crate::app::desktop_app::{DesktopApp, OpenSettings};
use crate::app::model_cycle::{THINKING_LEVELS, catalog_models};
use crate::shell::IslandId;

use super::{LocalCommandId, PaletteConfirmResult, RootSubmenu};

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
        if let Some(submenu) = palette.read(cx).selected_root_submenu() {
            match submenu {
                RootSubmenu::Models => {
                    if self.bridge_state().model.providers.is_empty() {
                        self.bridge.intent(ClientIntent::ListModels);
                    }
                    let models = catalog_models(&self.bridge_state().model.providers);
                    palette.update(cx, |p, cx| p.push_models(models, window, cx));
                    cx.notify();
                    return;
                }
                RootSubmenu::Thinking => {
                    palette.update(cx, |p, cx| p.push_thinking(THINKING_LEVELS, window, cx));
                    cx.notify();
                    return;
                }
            }
        }

        let result = palette.update(cx, |p, _| p.confirm());
        match result {
            PaletteConfirmResult::None | PaletteConfirmResult::StayOpen => {}
            PaletteConfirmResult::RunHost(id) => {
                self.run_host_command(&id, window, cx);
            }
            PaletteConfirmResult::RunLocal(id) => {
                self.run_local_command(id, window, cx);
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

    /// Run a neutral host catalog id. Only ids the GUI has a real flow for
    /// reach here (see `GUI_RUNNABLE_HOST_IDS` in the palette module);
    /// everything else is disabled in the palette itself.
    fn run_host_command(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay.close_transient();
        self.restore_overlay_focus(window, cx);

        if id == "session.new" {
            self.trigger_new_session(window, cx);
        }
        cx.notify();
    }

    fn run_local_command(
        &mut self,
        id: LocalCommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.overlay.close_transient();
        self.restore_overlay_focus(window, cx);

        match id {
            LocalCommandId::FocusSessions => self.focus_or_open_sessions(window, cx),
            LocalCommandId::FocusAgents | LocalCommandId::FocusTree => {
                self.focus_or_open_right_column(window, cx);
            }
            LocalCommandId::OpenSettings => {
                if self
                    .archipelago
                    .is_active(crate::app::archipelago::ArchipelagoId::Workbench)
                {
                    self.action_open_settings(&OpenSettings, window, cx);
                }
            }
            LocalCommandId::ClearNotifications => self.clear_notification_center(window, cx),
            LocalCommandId::Quit => self.request_quit_from_palette(window, cx),
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
        if crate::app::quit::is_quit_busy(self.bridge_state()) {
            self.request_busy_quit_confirm(window, cx);
        } else {
            cx.quit();
        }
    }
}
