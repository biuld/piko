//! DesktopApp overlay wiring: HostPrompt, LocalConfirm, Transient, Escape.

use std::rc::Rc;

use gpui::*;
use gpui_component::WindowExt;
use gpui_component::button::{Button, ButtonVariants};
use piko_client_core::{ClientIntent, find_approval, find_interaction};
use piko_protocol::{ApprovalDecision, UserInteractionResponse};

use crate::app::model_cycle::catalog_models;
use crate::chrome::{
    EscapeOutcome, IslandId, LocalConfirmKind, OverlayPanelSpec, OverlayPanelStyle, PaletteConfirm,
    TransientKind, render_overlay_layer,
};
use crate::islands::ActivityItem;
use crate::overlays::{
    InteractionForm, PromptKind, approval_title, derive_prompt_front, interaction_title,
    render_approval_body,
};
use crate::theme::{TextRole, text, tokens};

use super::desktop_app::{CloseTransientOverlay, DesktopApp, OpenCommandPalette};

impl DesktopApp {
    pub(crate) fn handle_activity_item(
        &mut self,
        item: ActivityItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(agent_id) = item.agent_instance_id.clone() {
            self.handle_select_agent(agent_id, window, cx);
        }
        if item.prompt_id.is_some() {
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
        front: &crate::overlays::PromptFront,
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

    fn ensure_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_palette.is_some() {
            return;
        }
        self.command_palette = Some(cx.new(|cx| crate::chrome::CommandPalette::new(window, cx)));
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

    fn save_overlay_focus_if_needed(&mut self, id: IslandId) {
        if !self.overlay.focus_saved {
            self.island_focus.save_and_focus(id);
            self.overlay.focus_saved = true;
        }
    }

    pub(crate) fn restore_overlay_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay.visible_layer().is_some() {
            return;
        }
        if self.overlay.focus_saved {
            self.overlay.focus_saved = false;
            self.island_focus.restore();
            self.apply_island_focus_chrome(cx);
            self.focus_island(self.island_focus.focused(), window, cx);
        }
    }

    pub(crate) fn render_active_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let layer = self.overlay.visible_layer()?;
        match layer {
            crate::chrome::OverlayLayer::HostPrompt => {
                Some(self.render_host_prompt_overlay(window, cx))
            }
            crate::chrome::OverlayLayer::LocalConfirm(LocalConfirmKind::QuitBusy) => {
                Some(self.render_quit_confirm_overlay(cx))
            }
            crate::chrome::OverlayLayer::Transient(TransientKind::CommandPalette) => {
                Some(self.render_palette_overlay(window, cx))
            }
        }
    }

    fn render_host_prompt_overlay(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let front = self.overlay.host_prompt.clone().expect("host prompt layer");
        let Some(session) = self.bridge_state().live_session.clone() else {
            return div().into_any_element();
        };
        let entity = cx.entity().downgrade();

        match front.kind {
            PromptKind::Approval => {
                let Some(approval) = find_approval(&session, &front.id).cloned() else {
                    return div().into_any_element();
                };
                let approval_id = approval.approval_id.clone();
                let on_decide = Rc::new(
                    move |decision: ApprovalDecision, _w: &mut Window, cx: &mut App| {
                        if let Some(view) = entity.upgrade() {
                            view.update(cx, |this, cx| {
                                this.bridge_mut().intent(ClientIntent::RespondApproval {
                                    approval_id: approval_id.clone(),
                                    decision,
                                    note: None,
                                });
                                cx.notify();
                            });
                        }
                    },
                );
                render_overlay_layer(
                    OverlayPanelSpec {
                        title: approval_title(front.remaining).into(),
                        width: px(560.),
                        backdrop_dismiss: false,
                        style: OverlayPanelStyle::Dialog,
                    },
                    render_approval_body(&approval, on_decide),
                    |_, _, _| {},
                )
                .into_any_element()
            }
            PromptKind::Interaction => {
                let Some(form) = self.interaction_form.clone() else {
                    return div().into_any_element();
                };
                render_overlay_layer(
                    OverlayPanelSpec {
                        title: interaction_title(front.remaining).into(),
                        width: px(620.),
                        backdrop_dismiss: false,
                        style: OverlayPanelStyle::Dialog,
                    },
                    form,
                    |_, _, _| {},
                )
                .into_any_element()
            }
        }
    }

    fn render_quit_confirm_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = tokens();
        let entity = cx.entity().downgrade();
        let body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                text(TextRole::Body)
                    .text_color(t.fg_rgba())
                    .child(crate::t!("dialog.quit.body")),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .justify_end()
                    .child({
                        let entity = entity.clone();
                        Button::new("quit-cancel")
                            .label(crate::t!("dialog.action.cancel"))
                            .on_click(move |_, window, cx| {
                                if let Some(view) = entity.upgrade() {
                                    view.update(cx, |this, cx| {
                                        this.overlay.close_local_confirm();
                                        this.restore_overlay_focus(window, cx);
                                        cx.notify();
                                    });
                                }
                            })
                    })
                    .child(
                        Button::new("quit-ok")
                            .danger()
                            .label(crate::t!("dialog.quit.confirm"))
                            .on_click(|_, _, cx| {
                                cx.quit();
                            }),
                    ),
            );
        let entity_backdrop = cx.entity().downgrade();
        render_overlay_layer(
            OverlayPanelSpec {
                title: crate::t!("dialog.quit.title").into(),
                width: px(420.),
                backdrop_dismiss: true,
                style: OverlayPanelStyle::Dialog,
            },
            body,
            move |_, window, cx| {
                if let Some(view) = entity_backdrop.upgrade() {
                    view.update(cx, |this, cx| {
                        this.overlay.close_local_confirm();
                        this.restore_overlay_focus(window, cx);
                        cx.notify();
                    });
                }
            },
        )
        .into_any_element()
    }

    fn render_palette_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ensure_command_palette(window, cx);
        let palette = self.command_palette.clone().expect("palette entity");
        let title = palette.read(cx).frame_title();
        let entity = cx.entity().downgrade();
        let on_confirm = cx.listener(|this, _: &PaletteConfirm, window, cx| {
            this.run_selected_palette_command(window, cx);
        });
        let panel = div().on_action(on_confirm).child(palette.clone());
        let entity_backdrop = entity.clone();
        render_overlay_layer(
            OverlayPanelSpec {
                title: title.into(),
                width: px(480.),
                backdrop_dismiss: true,
                style: OverlayPanelStyle::Palette,
            },
            panel,
            move |_, window, cx| {
                if let Some(view) = entity_backdrop.upgrade() {
                    view.update(cx, |this, cx| {
                        this.overlay.close_transient();
                        this.restore_overlay_focus(window, cx);
                        cx.notify();
                    });
                }
            },
        )
        .into_any_element()
    }
}
