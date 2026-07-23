//! Archipelago open/close and Settings section selection.
//!
//! All router mutations go through [`piko_chrome::route_archipelago_nav`] so the
//! product path is not a parallel hand-roll of enter/leave/go (roadmap A3).
//! Focus remount / restore runs only on [`ArchipelagoTransition::Changed`].

use gpui::*;
use piko_chrome::{
    ArchipelagoNav, ArchipelagoTransition, FocusReason, FocusRing, route_archipelago_nav,
};

use crate::app::archipelago::{ArchipelagoId, settings_workspace, workbench_workspace};
use crate::app::desktop_app::{DesktopApp, OpenSettings};
use crate::features::{SettingsIslandId, SettingsSection};

impl DesktopApp {
    /// Apply an archipelago nav intent via chrome (single product entry point).
    pub(crate) fn route_archipelago(
        &mut self,
        nav: ArchipelagoNav<ArchipelagoId>,
    ) -> ArchipelagoTransition<ArchipelagoId> {
        route_archipelago_nav(&mut self.archipelago, nav)
    }

    /// TitleBar gear / `Cmd+,`: toggle Settings ↔ Workbench.
    pub(crate) fn action_open_settings(
        &mut self,
        _: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Save Workbench island focus before leaving it.
        if self.archipelago.is_active(ArchipelagoId::Workbench) {
            self.island_focus.save_for_restore();
        }

        let transition = self.route_archipelago(ArchipelagoNav::TogglePair {
            a: ArchipelagoId::Workbench,
            b: ArchipelagoId::Settings,
        });

        match transition {
            ArchipelagoTransition::Changed {
                to: ArchipelagoId::Settings,
                ..
            } => {
                self.sync_settings_islands(cx);
                self.focus_settings_island(SettingsIslandId::Nav, window, cx);
                cx.notify();
            }
            ArchipelagoTransition::Changed {
                to: ArchipelagoId::Workbench,
                ..
            } => {
                self.restore_workbench_island_focus(window, cx);
                cx.notify();
            }
            ArchipelagoTransition::Unchanged { .. } => {}
        }
    }

    pub(crate) fn select_settings_section(
        &mut self,
        section: SettingsSection,
        cx: &mut Context<Self>,
    ) {
        self.last_settings_section = section;
        if !self.archipelago.is_active(ArchipelagoId::Settings) {
            let transition = self.route_archipelago(ArchipelagoNav::Enter {
                id: ArchipelagoId::Settings,
            });
            // Entering Settings from a non-Settings place is a frame change;
            // section sync always runs so islands match app state.
            let _ = transition;
        }
        self.sync_settings_islands(cx);
        cx.notify();
    }

    /// Push active section into Settings Nav/Panel island Entities.
    pub(crate) fn sync_settings_islands(&mut self, cx: &mut Context<Self>) {
        let section = self.last_settings_section;
        self.settings_nav.update(cx, |island, cx| {
            island.apply(section, cx);
        });
        self.settings_panel.update(cx, |island, cx| {
            island.apply(section, cx);
        });
    }

    /// Focus a Settings body island (ring + keyboard handoff).
    pub(crate) fn focus_settings_island(
        &mut self,
        id: SettingsIslandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_settings_island_with_reason(id, FocusReason::Activate, window, cx);
    }

    /// Synchronize Settings chrome ownership without stealing focus from an
    /// inner form control on pointer-driven claims.
    pub(crate) fn claim_settings_island_focus(
        &mut self,
        id: SettingsIslandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_settings_island_with_reason(id, FocusReason::Claimed, window, cx);
    }

    fn focus_settings_island_with_reason(
        &mut self,
        id: SettingsIslandId,
        reason: FocusReason,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_focus_table
            .focus(&mut self.settings_focus, id, reason, window, cx);
    }

    #[allow(dead_code)] // used when re-syncing rings without keyboard handoff
    pub(crate) fn apply_settings_focus_chrome(&mut self, cx: &mut Context<Self>) {
        let focused = self.settings_focus.focused();
        self.settings_focus_table.apply_chrome_rings(focused, cx);
    }

    /// Escape with no overlay: leave Settings before sheet dismissal.
    pub(crate) fn try_close_settings_on_escape(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.archipelago.is_active(ArchipelagoId::Workbench)
            || self.overlay.visible_layer().is_some()
        {
            return false;
        }
        self.close_settings_archipelago(window, cx);
        true
    }

    fn close_settings_archipelago(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut transition = self.route_archipelago(ArchipelagoNav::Leave);
        if !transition.changed() {
            // No restore stack — hard-cut home.
            transition = self.route_archipelago(ArchipelagoNav::Go {
                id: ArchipelagoId::Workbench,
            });
        }
        if transition.changed() {
            self.restore_workbench_island_focus(window, cx);
            cx.notify();
        }
    }

    fn restore_workbench_island_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.island_focus.restore();
        self.apply_island_focus_chrome(cx);
        self.focus_island(self.island_focus.focused(), window, cx);
    }
}

/// Newtype so we can `Default` to Nav without orphan-rule issues.
#[derive(Debug, Clone)]
pub struct SettingsFocusRing(pub FocusRing<SettingsIslandId>);

impl Default for SettingsFocusRing {
    fn default() -> Self {
        Self(FocusRing::new(SettingsIslandId::Nav))
    }
}

impl std::ops::Deref for SettingsFocusRing {
    type Target = FocusRing<SettingsIslandId>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SettingsFocusRing {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Tab order among Settings islands — from the Settings workspace declaration.
#[allow(dead_code)]
pub(crate) fn settings_focus_order() -> Vec<SettingsIslandId> {
    settings_workspace().focus_order
}

/// Tab order among Workbench islands when all are visible — from workspace.
#[allow(dead_code)]
pub(crate) fn workbench_focus_order() -> Vec<crate::shell::IslandId> {
    workbench_workspace().focus_order
}
