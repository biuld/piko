//! Approval dialog: non-dismissable; Escape/backdrop do not close.

use std::rc::Rc;

use gpui::*;
use gpui_component::Disableable;
use gpui_component::WindowExt;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::scroll::ScrollableElement;
use piko_client_core::state::PendingApproval;
use piko_protocol::ApprovalDecision;

type DecideFn = Rc<dyn Fn(ApprovalDecision, &mut Window, &mut App) + 'static>;

/// Open (or replace) the host approval dialog. Does not close on Escape/backdrop.
pub fn open_approval_dialog(
    window: &mut Window,
    cx: &mut App,
    approval: PendingApproval,
    remaining: usize,
    on_decide: DecideFn,
) {
    let tool_name = approval.tool_name.clone();
    let args_text = serde_json::to_string_pretty(&approval.tool_args)
        .unwrap_or_else(|_| approval.tool_args.to_string());
    let in_flight = approval.response_in_flight;
    let title = if remaining > 1 {
        format!("Approval ({remaining} pending)")
    } else {
        "Approval".into()
    };

    window.open_dialog(cx, move |dialog, _window, _cx| {
        let on_decide = on_decide.clone();
        dialog
            .title(title.clone())
            .overlay_closable(false)
            .close_button(false)
            .keyboard(false)
            .on_cancel(|_, _, _| false)
            .w(px(560.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Tool: {tool_name}")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(gpui::rgb(0xa6adc8))
                            .child("Arguments"),
                    )
                    .child(
                        div()
                            .p_2()
                            .rounded_md()
                            .bg(gpui::rgb(0x181825))
                            .max_h(px(240.))
                            .overflow_y_scrollbar()
                            .child(
                                div()
                                    .text_xs()
                                    .font_family("monospace")
                                    .text_color(gpui::rgb(0xcdd6f4))
                                    .child(args_text.clone()),
                            ),
                    )
                    .child(decision_row(in_flight, on_decide)),
            )
            .footer(|_, _, _, _| Vec::<AnyElement>::new())
    });
}

fn decision_row(in_flight: bool, on_decide: DecideFn) -> impl IntoElement {
    let mk = |id: &'static str, label: &'static str, decision: ApprovalDecision| {
        let on_decide = on_decide.clone();
        Button::new(id)
            .label(label)
            .disabled(in_flight)
            .on_click(move |_, window, cx| {
                on_decide(decision.clone(), window, cx);
            })
    };

    div()
        .flex()
        .flex_wrap()
        .gap_2()
        .child(mk("appr-accept", "Accept once", ApprovalDecision::Accept))
        .child(mk(
            "appr-session",
            "Accept session",
            ApprovalDecision::AcceptSession,
        ))
        .child(mk(
            "appr-workspace",
            "Accept workspace",
            ApprovalDecision::AcceptWorkspace,
        ))
        .child(mk(
            "appr-forever",
            "Accept forever",
            ApprovalDecision::AcceptPermanent,
        ))
        .child(
            Button::new("appr-decline")
                .danger()
                .label("Decline")
                .disabled(in_flight)
                .on_click({
                    let on_decide = on_decide.clone();
                    move |_, window, cx| {
                        on_decide(ApprovalDecision::Decline, window, cx);
                    }
                }),
        )
}
