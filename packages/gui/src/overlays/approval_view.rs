//! Approval HostPrompt body (chrome OverlayHost surface).

use std::rc::Rc;

use gpui::*;
use gpui_component::Disableable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::scroll::ScrollableElement;
use piko_client_core::state::PendingApproval;
use piko_protocol::ApprovalDecision;

use crate::theme::{TextRole, text, tokens};

type DecideFn = Rc<dyn Fn(ApprovalDecision, &mut Window, &mut App) + 'static>;

pub fn approval_title(remaining: usize) -> String {
    if remaining > 1 {
        crate::t!("overlay.approval.title_pending", count = remaining)
    } else {
        crate::t!("overlay.approval.title")
    }
}

pub fn render_approval_body(approval: &PendingApproval, on_decide: DecideFn) -> impl IntoElement {
    let t = tokens();
    let tool_name = approval.tool_name.clone();
    let args_text = serde_json::to_string_pretty(&approval.tool_args)
        .unwrap_or_else(|_| approval.tool_args.to_string());
    let in_flight = approval.response_in_flight;

    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            text(TextRole::Label)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.fg_rgba())
                .child(format!(
                    "{}: {tool_name}",
                    crate::t!("overlay.approval.tool")
                )),
        )
        .child(
            text(TextRole::Meta)
                .text_color(t.muted_fg_rgba())
                .child(crate::t!("dialog.approval.arguments")),
        )
        .child(
            div()
                .p_2()
                .rounded_md()
                .bg(t.surface_rgba())
                .max_h(px(240.))
                .overflow_y_scrollbar()
                .child(
                    text(TextRole::BodyMono)
                        .text_color(t.fg_rgba())
                        .child(args_text),
                ),
        )
        .child(decision_row(in_flight, on_decide))
}

fn decision_row(in_flight: bool, on_decide: DecideFn) -> impl IntoElement {
    let mk = |id: &'static str, label: String, decision: ApprovalDecision| {
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
        .child(mk(
            "appr-accept",
            crate::t!("overlay.approval.accept_once"),
            ApprovalDecision::Accept,
        ))
        .child(mk(
            "appr-session",
            crate::t!("overlay.approval.accept_session"),
            ApprovalDecision::AcceptSession,
        ))
        .child(mk(
            "appr-workspace",
            crate::t!("overlay.approval.accept_workspace"),
            ApprovalDecision::AcceptWorkspace,
        ))
        .child(mk(
            "appr-forever",
            crate::t!("overlay.approval.accept_forever"),
            ApprovalDecision::AcceptPermanent,
        ))
        .child(
            Button::new("appr-decline")
                .danger()
                .label(crate::t!("dialog.approval.decline"))
                .disabled(in_flight)
                .on_click({
                    let on_decide = on_decide.clone();
                    move |_, window, cx| {
                        on_decide(ApprovalDecision::Decline, window, cx);
                    }
                }),
        )
}
