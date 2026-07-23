//! Overlay presentation builders; lifecycle and routing live in `overlay_sync`.

use std::rc::Rc;

use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use piko_client_core::{ClientIntent, find_approval};
use piko_protocol::ApprovalDecision;

use crate::app::desktop_app::DesktopApp;
use crate::features::{
    PaletteConfirm, PromptKind, approval_title, interaction_title, render_approval_body,
};
use crate::shell::{
    LocalConfirmKind, OverlayPanelSpec, OverlayPanelStyle, TransientKind, render_overlay_layer,
};
use crate::theme::{TextRole, text, tokens};

impl DesktopApp {
    pub(crate) fn render_active_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let layer = self.overlay.visible_layer()?;
        match layer {
            crate::shell::OverlayLayer::HostPrompt => {
                Some(self.render_host_prompt_overlay(window, cx))
            }
            crate::shell::OverlayLayer::LocalConfirm(kind) => match kind {
                LocalConfirmKind::QuitBusy => Some(self.render_quit_confirm_overlay(window, cx)),
                LocalConfirmKind::DeleteSession {
                    session_id,
                    display_name,
                } => Some(self.render_delete_session_confirm_overlay(
                    session_id,
                    display_name,
                    window,
                    cx,
                )),
            },
            crate::shell::OverlayLayer::Transient(kind) => match kind {
                TransientKind::CommandPalette => Some(self.render_palette_overlay(window, cx)),
                TransientKind::SessionRename {
                    session_id,
                    initial_name: _,
                } => Some(self.render_session_rename_overlay(session_id, window, cx)),
            },
        }
    }

    fn render_host_prompt_overlay(
        &mut self,
        window: &Window,
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
                        viewport: Some(window.viewport_size()),
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
                        viewport: Some(window.viewport_size()),
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

    fn render_quit_confirm_overlay(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
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
                            .on_click(|_, _, cx| cx.quit()),
                    ),
            );
        let entity_backdrop = cx.entity().downgrade();
        render_overlay_layer(
            OverlayPanelSpec {
                title: crate::t!("dialog.quit.title").into(),
                width: px(420.),
                viewport: Some(window.viewport_size()),
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
        let on_confirm = cx.listener(|this, _: &PaletteConfirm, window, cx| {
            this.run_selected_palette_command(window, cx);
        });
        let panel = div().on_action(on_confirm).child(palette.clone());
        let entity_backdrop = cx.entity().downgrade();
        render_overlay_layer(
            OverlayPanelSpec {
                title: title.into(),
                width: px(480.),
                viewport: Some(window.viewport_size()),
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

    fn render_delete_session_confirm_overlay(
        &mut self,
        session_id: String,
        display_name: String,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity = cx.entity().downgrade();
        let sid = session_id.clone();
        let body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(text(TextRole::Body).child(crate::t!(
                "island.sessions.delete.body",
                name = display_name
            )))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .justify_end()
                    .child({
                        let entity = entity.clone();
                        Button::new("delete-cancel")
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
                    .child({
                        let entity = entity.clone();
                        let sid = sid.clone();
                        Button::new("delete-confirm")
                            .danger()
                            .label(crate::t!("island.sessions.delete.confirm"))
                            .on_click(move |_, window, cx| {
                                if let Some(view) = entity.upgrade() {
                                    view.update(cx, |this, cx| {
                                        this.confirm_session_delete(&sid, cx);
                                        this.restore_overlay_focus(window, cx);
                                    });
                                }
                            })
                    }),
            );
        let entity_backdrop = cx.entity().downgrade();
        render_overlay_layer(
            OverlayPanelSpec {
                title: crate::t!("island.sessions.delete.title").into(),
                width: px(420.),
                viewport: Some(window.viewport_size()),
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

    fn render_session_rename_overlay(
        &mut self,
        session_id: String,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity = cx.entity().downgrade();
        let sid = session_id.clone();
        let input = self
            .session_rename_input
            .clone()
            .expect("rename input entity");
        let body = div().flex().flex_col().gap_3().child(input.clone()).child(
            div()
                .flex()
                .gap_2()
                .justify_end()
                .child({
                    let entity = entity.clone();
                    Button::new("rename-cancel")
                        .label(crate::t!("dialog.action.cancel"))
                        .on_click(move |_, window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.overlay.close_transient();
                                    this.restore_overlay_focus(window, cx);
                                    cx.notify();
                                });
                            }
                        })
                })
                .child({
                    let entity = entity.clone();
                    let sid = sid.clone();
                    let input = input.clone();
                    Button::new("rename-save")
                        .primary()
                        .label(crate::t!("island.sessions.rename.save"))
                        .on_click(move |_, window, cx| {
                            let name = input.read(cx).value().to_string();
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.confirm_session_rename(&sid, name, window, cx);
                                });
                            }
                        })
                }),
        );
        let entity_backdrop = cx.entity().downgrade();
        render_overlay_layer(
            OverlayPanelSpec {
                title: crate::t!("island.sessions.rename.title").into(),
                width: px(420.),
                viewport: Some(window.viewport_size()),
                backdrop_dismiss: true,
                style: OverlayPanelStyle::Dialog,
            },
            body,
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
