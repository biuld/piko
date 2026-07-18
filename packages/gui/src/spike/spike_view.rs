//! Primary spike view demonstrating all required GPUI primitives.

use gpui::*;
use gpui_component::Root;
use gpui_component::WindowExt;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::list::ListItem;
use gpui_component::notification::{Notification, NotificationType};
use gpui_component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_component::scroll::ScrollableElement;
use gpui_component::tree::{Tree, TreeItem, TreeState};

actions!(spike, [FocusComposer, OpenApproval, ShowNotification]);

pub struct SpikeView {
    composer_input: Entity<InputState>,
    tree_state: Entity<TreeState>,
    focus_handle: FocusHandle,
    log_lines: Vec<String>,
}

impl SpikeView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let composer_input = cx.new(|cx| {
            InputState::new(window, cx)
                .auto_grow(1, 8)
                .placeholder("Type message… (Shift+Enter for newline, Enter to submit)")
        });

        cx.subscribe_in(
            &composer_input,
            window,
            |this, _state, event, window, cx| {
                if let InputEvent::PressEnter { secondary } = event
                    && !secondary
                {
                    this.handle_submit(window, cx);
                }
            },
        )
        .detach();

        let tree_state = cx.new(|cx| {
            let mut state = TreeState::new(cx);
            state.set_items(sample_tree_items(), cx);
            state.set_selected_index(Some(0), cx);
            state
        });

        Self {
            composer_input,
            tree_state,
            focus_handle: cx.focus_handle(),
            log_lines: vec![
                "Spike initialized.".into(),
                "Scroll this panel to verify Scrollable.".into(),
                "Press Cmd+L to focus composer.".into(),
                "Press Cmd+Shift+A for approval dialog.".into(),
                "Press Cmd+Shift+N for notification.".into(),
            ],
        }
    }

    fn handle_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer_input.read(cx).value().to_string();
        if text.is_empty() {
            return;
        }
        self.log_lines.push(format!("> {text}"));
        self.composer_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        window.push_notification((NotificationType::Success, "Message submitted"), cx);
        cx.notify();
    }

    fn action_focus_composer(
        &mut self,
        _: &FocusComposer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer_input.update(cx, |state, cx| {
            state.focus(window, cx);
        });
        self.log_lines.push("[action] FocusComposer".into());
        cx.notify();
    }

    fn action_open_approval(
        &mut self,
        _: &OpenApproval,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.log_lines
            .push("[action] OpenApproval — non-dismissable dialog".into());
        cx.notify();
        window.open_dialog(cx, |dialog, _window, _cx| {
            dialog
                .title("Approval Required")
                .overlay(true)
                .overlay_closable(false)
                .keyboard(false)
                .close_button(false)
                .child(
                    "This dialog cannot be dismissed with Escape or backdrop click.\n\
                     Only the Approve/Reject buttons resolve it.",
                )
                .footer(|_dialog, _window, _cx, _| {
                    vec![
                        Button::new("approve").primary().label("Approve").on_click(
                            |_, window, cx| {
                                window.push_notification((NotificationType::Info, "Approved"), cx);
                                window.close_dialog(cx);
                            },
                        ),
                        Button::new("reject")
                            .label("Reject")
                            .on_click(|_, window, cx| {
                                window
                                    .push_notification((NotificationType::Warning, "Rejected"), cx);
                                window.close_dialog(cx);
                            }),
                    ]
                })
        });
    }

    fn action_show_notification(
        &mut self,
        _: &ShowNotification,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.log_lines.push("[action] ShowNotification".into());
        cx.notify();
        window.push_notification(
            Notification::new()
                .title("Spike Notification")
                .message("Notification primitive works.")
                .with_type(NotificationType::Info)
                .autohide(true),
            cx,
        );
    }
}

impl Render for SpikeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Listeners first — avoid Rust 2024 borrow conflicts with panel trees.
        let on_focus = cx.listener(Self::action_focus_composer);
        let on_approval = cx.listener(Self::action_open_approval);
        let on_notify = cx.listener(Self::action_show_notification);
        let entity = cx.entity().downgrade();

        let tree_state = self.tree_state.clone();
        let tree_entity = entity.clone();
        let left_panel = div()
            .size_full()
            .p_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .mb_2()
                    .child("Agent Tree"),
            )
            .child(Tree::new(
                &tree_state,
                move |ix, entry, _selected, _window, _cx| {
                    let tree_entity = tree_entity.clone();
                    let item = entry.item();
                    let label = item.label.clone();
                    ListItem::new(ix)
                        .pl(px(16.) * entry.depth() as f32)
                        .child(label.clone())
                        .on_click(move |_, _window, cx| {
                            if let Some(view) = tree_entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.log_lines.push(format!("[tree] selected: {label}"));
                                    cx.notify();
                                });
                            }
                        })
                },
            ));

        let center_panel = div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div().flex_1().overflow_y_scrollbar().p_2().child(
                    div().flex().flex_col().gap_1().children(
                        self.log_lines
                            .iter()
                            .map(|line| div().text_sm().child(line.clone())),
                    ),
                ),
            )
            .child(
                div()
                    .p_2()
                    .border_t_1()
                    .child(Input::new(&self.composer_input)),
            );

        let approval_entity = entity.clone();
        let notify_entity = entity.clone();
        let right_panel = div()
            .size_full()
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Inspector"),
            )
            .child(
                Button::new("btn-approval")
                    .label("Open Approval Dialog")
                    .on_click(move |_, window, cx| {
                        if let Some(view) = approval_entity.upgrade() {
                            view.update(cx, |this, cx| {
                                this.action_open_approval(&OpenApproval, window, cx);
                            });
                        }
                    }),
            )
            .child(
                Button::new("btn-sheet")
                    .label("Open Sheet")
                    .on_click(|_, window, cx| {
                        window.open_sheet(cx, |sheet, _window, _cx| {
                            sheet
                                .title("Settings Sheet")
                                .child("Sheet primitive works. Close to restore focus.")
                        });
                    }),
            )
            .child(
                Button::new("btn-notify")
                    .label("Push Notification")
                    .on_click(move |_, window, cx| {
                        if let Some(view) = notify_entity.upgrade() {
                            view.update(cx, |this, cx| {
                                this.action_show_notification(&ShowNotification, window, cx);
                            });
                        }
                    }),
            );

        let map_panel = div().size_full().p_2().child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Conversation Map (placeholder)"),
        );

        div()
            .id("spike-root")
            .track_focus(&self.focus_handle)
            .on_action(on_focus)
            .on_action(on_approval)
            .on_action(on_notify)
            .key_context("SpikeView")
            .size_full()
            .flex()
            .flex_col()
            .bg(gpui::rgb(0x1e1e2e))
            .text_color(gpui::rgb(0xcdd6f4))
            .child(
                div().flex_1().child(
                    h_resizable("main-h")
                        .child(resizable_panel().size(px(220.)).child(left_panel))
                        .child(resizable_panel().size(px(500.)).child(center_panel))
                        .child(
                            resizable_panel().size(px(280.)).child(
                                v_resizable("right-v")
                                    .child(resizable_panel().size(px(200.)).child(right_panel))
                                    .child(resizable_panel().size(px(200.)).child(map_panel)),
                            ),
                        ),
                ),
            )
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

impl Focusable for SpikeView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn sample_tree_items() -> Vec<TreeItem> {
    vec![
        TreeItem::new("root", "Root Agent")
            .expanded(true)
            .children(vec![
                TreeItem::new("research", "Research Agent")
                    .expanded(true)
                    .children(vec![
                        TreeItem::new("web", "Web Search"),
                        TreeItem::new("code", "Code Analysis"),
                    ]),
                TreeItem::new("editor", "Editor Agent"),
                TreeItem::new("reviewer", "Reviewer Agent"),
            ]),
    ]
}
