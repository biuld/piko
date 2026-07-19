//! Primary Surface open/close and Settings section selection.

use gpui::*;

use crate::chrome::primary_surface::{PrimarySurface, SettingsSection};

use super::desktop_app::{DesktopApp, OpenSettings};

impl DesktopApp {
    /// TitleBar gear / `Cmd+,`: toggle Settings ↔ Workbench.
    pub(crate) fn action_open_settings(
        &mut self,
        _: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.primary_surface.is_workbench() {
            self.island_focus.save_for_restore();
            self.primary_surface = PrimarySurface::Settings {
                section: self.last_settings_section,
            };
            cx.notify();
            return;
        }
        self.close_settings_surface(window, cx);
    }

    pub(crate) fn select_settings_section(
        &mut self,
        section: SettingsSection,
        cx: &mut Context<Self>,
    ) {
        self.last_settings_section = section;
        self.primary_surface = PrimarySurface::Settings { section };
        cx.notify();
    }

    /// Escape with no overlay: pop Settings before sheet dismissal.
    pub(crate) fn try_close_settings_on_escape(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.primary_surface.is_workbench() || self.overlay.visible_layer().is_some() {
            return false;
        }
        self.close_settings_surface(window, cx);
        true
    }

    fn close_settings_surface(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.primary_surface = PrimarySurface::Workbench;
        self.island_focus.restore();
        self.apply_island_focus_chrome(cx);
        self.focus_island(self.island_focus.focused(), window, cx);
        cx.notify();
    }
}
