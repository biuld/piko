//! DesktopApp overlay wiring: HostPrompt, LocalConfirm, Transient, Escape.

use std::rc::Rc;

use gpui::*;
use gpui_component::WindowExt;
use piko_client_core::{ClientIntent, find_interaction};
use piko_protocol::UserInteractionResponse;

use crate::app::archipelago::{ArchipelagoFocusTarget, ArchipelagoId};
use crate::app::model_cycle::catalog_models;
use crate::features::{InteractionForm, PromptKind, derive_prompt_front};
use crate::shell::{EscapeOutcome, IslandId};

use crate::app::desktop_app::{CloseTransientOverlay, DesktopApp, OpenCommandPalette};

impl DesktopApp {
    pub(crate) fn handle_activity_item(
        &mut self,
        agent_instance_id: Option<String>,
        prompt_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(agent_id) = agent_instance_id {
            self.handle_select_agent(agent_id, window, cx);
        }
        if prompt_id.is_some() {
            self.sync_prompts(window, cx);
        }
        self.refresh_islands(cx);
        cx.notify();
    }

    pub(crate) fn sync_prompts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let front = derive_prompt_front(self.bridge_state());
        if !self.overlay.sync_host_prompt(front.clone()) {
            return;
        }
        self.interaction_form = None;
        if let Some(front) = front {
            self.close_notification_center(cx);
            self.ensure_host_prompt_body(window, cx, &front);
            self.save_overlay_focus_if_needed(IslandId::Composer);
        } else {
            self.restore_overlay_focus(window, cx);
        }
        cx.notify();
    }

    fn ensure_host_prompt_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        front: &crate::features::PromptFront,
    ) {
        if front.kind != PromptKind::Interaction {
            return;
        }
        let Some(session) = self.bridge_state().live_session.clone() else {
            return;
        };
        let Some(interaction) = find_interaction(&session, &front.id).cloned() else {
            return;
        };
        let entity = cx.entity().downgrade();
        let interaction_id = interaction.interaction_id.clone();
        let on_respond = Rc::new(
            move |response: UserInteractionResponse, _w: &mut Window, cx: &mut App| {
                if let Some(view) = entity.upgrade() {
                    view.update(cx, |this, cx| {
                        this.bridge_mut().intent(ClientIntent::RespondInteraction {
                            interaction_id: interaction_id.clone(),
                            response,
                        });
                        cx.notify();
                    });
                }
            },
        );
        self.interaction_form = Some(InteractionForm::new(window, cx, interaction, on_respond));
    }

    pub(crate) fn request_busy_quit_confirm(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay.try_open_quit_confirm() {
            self.close_notification_center(cx);
            self.save_overlay_focus_if_needed(self.island_focus.focused());
            cx.notify();
        }
    }

    pub(crate) fn action_open_command_palette(
        &mut self,
        _: &OpenCommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.overlay.try_open_command_palette() {
            return;
        }
        self.close_notification_center(cx);
        self.save_overlay_focus_if_needed(self.island_focus.focused());
        self.ensure_command_palette(window, cx);
        self.bridge.request_command_catalog();
        if let Some(palette) = &self.command_palette {
            let catalog = self.bridge.command_catalog().cloned().unwrap_or_default();
            palette.update(cx, |p, cx| {
                p.set_catalog(catalog, cx);
                p.reset_to_root(window, cx);
                p.focus_filter(window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn action_close_transient_overlay(
        &mut self,
        _: &CloseTransientOverlay,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.close_notification_center(cx) {
            return;
        }

        // Palette submenu: Escape pops one level before closing the Transient.
        if self.overlay.is_command_palette_open()
            && let Some(palette) = self.command_palette.clone()
            && palette.update(cx, |p, cx| p.try_pop_submenu(window, cx))
        {
            cx.notify();
            return;
        }

        match self.overlay.handle_escape() {
            EscapeOutcome::Swallowed => {}
            EscapeOutcome::CancelInteraction => {
                if let Some(form) = self.interaction_form.clone() {
                    form.update(cx, |form, cx| {
                        form.send_cancel(window, cx);
                    });
                }
            }
            EscapeOutcome::Closed => {
                self.restore_overlay_focus(window, cx);
                cx.notify();
            }
            EscapeOutcome::NotHandled => {
                if self.try_close_settings_on_escape(window, cx) {
                    return;
                }
                if window.has_active_sheet(cx) {
                    window.close_sheet(cx);
                    self.island_focus.restore();
                    self.apply_island_focus_chrome(cx);
                    self.focus_island(self.island_focus.focused(), window, cx);
                    cx.notify();
                }
            }
        }
    }

    pub(super) fn ensure_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_palette.is_some() {
            return;
        }
        self.command_palette = Some(cx.new(|cx| crate::features::CommandPalette::new(window, cx)));
    }

    pub(crate) fn sync_command_catalog(&mut self, cx: &mut Context<Self>) {
        let Some(palette) = self.command_palette.clone() else {
            return;
        };
        if !self.overlay.is_command_palette_open() {
            return;
        }
        if let Some(catalog) = self.bridge.command_catalog().cloned() {
            palette.update(cx, |p, cx| p.set_catalog(catalog, cx));
        }
        let models = catalog_models(&self.bridge_state().model.providers);
        if !models.is_empty() {
            palette.update(cx, |p, cx| p.refresh_models_if_open(models, cx));
        }
    }

    /// Capture the active archipelago's focus target on the first overlay open.
    ///
    /// `workbench_preferred` preserves product policy such as returning a host
    /// prompt to Composer. It is ignored when the overlay opens from Settings.
    pub(crate) fn save_overlay_focus_if_needed(&mut self, workbench_preferred: IslandId) {
        if self.overlay.begin_focus_session() {
            self.overlay_focus_restore = Some(ArchipelagoFocusTarget::capture(
                self.archipelago.active(),
                workbench_preferred,
                self.settings_focus.focused(),
            ));
        }
    }

    /// Restore the focus target captured from the archipelago that opened the
    /// overlay. Never focus an entity hidden behind another archipelago.
    pub(crate) fn restore_overlay_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay.end_focus_session_if_idle() {
            let target = self.overlay_focus_restore.take();
            match (self.archipelago.active(), target) {
                (ArchipelagoId::Workbench, Some(ArchipelagoFocusTarget::Workbench(id))) => {
                    self.focus_island(id, window, cx)
                }
                (ArchipelagoId::Settings, Some(ArchipelagoFocusTarget::Settings(id))) => {
                    self.focus_settings_island(id, window, cx)
                }
                // If navigation changed while an overlay was open, restore the
                // current archipelago's own ring rather than a hidden entity.
                (ArchipelagoId::Workbench, _) => {
                    self.focus_island(self.island_focus.focused(), window, cx)
                }
                (ArchipelagoId::Settings, _) => {
                    self.focus_settings_island(self.settings_focus.focused(), window, cx)
                }
            }
        }
    }
}

#[cfg(test)]
mod e5_viewport_tests {
    /// Roadmap E5: product overlay paints pass window viewport into chrome
    /// geometry. This module is the only GUI construction site for
    /// `OverlayPanelSpec` product layers.
    #[test]
    fn all_overlay_panel_specs_pass_window_viewport() {
        let production = include_str!("overlay_render.rs");
        let viewport_sites = production
            .matches("viewport: Some(window.viewport_size())")
            .count();
        // Palette, host prompt, local confirm, session delete, rename, …
        assert!(
            viewport_sites >= 6,
            "expected every OverlayPanelSpec to pass viewport; found {viewport_sites}"
        );
        // No product path should omit viewport when constructing a panel.
        assert!(
            !production.contains("viewport: None"),
            "product overlay path must not omit viewport"
        );
    }
}
