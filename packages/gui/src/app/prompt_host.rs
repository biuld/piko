//! DesktopApp prompt dialog host: open/refresh/close via GPUI Root layers.

use std::rc::Rc;

use gpui::*;
use gpui_component::WindowExt;
use piko_client_core::{ClientIntent, find_approval, find_interaction};
use piko_protocol::{ApprovalDecision, UserInteractionResponse};

use crate::overlays::{
    PromptFront, PromptKind, derive_prompt_front, open_approval_dialog, open_interaction_dialog,
    prompt_fingerprint,
};
use crate::workbench::{ActivityItem, derive_activity};

use super::desktop_app::DesktopApp;

impl DesktopApp {
    pub(crate) fn expanded_tools(&self) -> &std::collections::HashSet<String> {
        &self.expanded_tools
    }

    pub(crate) fn activity_expanded(&self) -> bool {
        self.activity_expanded
    }

    pub(crate) fn toggle_activity_expanded(&mut self) {
        self.activity_expanded = !self.activity_expanded;
        self.activity_user_toggled = true;
    }

    pub(crate) fn toggle_tool_detail(&mut self, row_id: String) {
        if !self.expanded_tools.remove(&row_id) {
            self.expanded_tools.insert(row_id);
        }
    }

    pub(crate) fn sync_activity_expand(&mut self) {
        let vm = derive_activity(self.bridge_state());
        let fp: String = vm
            .items
            .iter()
            .filter(|i| i.actionable)
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>()
            .join("|");
        if fp != self.activity_actionable_fp {
            self.activity_actionable_fp = fp;
            self.activity_user_toggled = false;
            if vm.prefer_expanded {
                self.activity_expanded = true;
            }
        }
        if !vm.prefer_expanded && !self.activity_user_toggled {
            self.activity_expanded = false;
        }
    }

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
        cx.notify();
    }

    pub(crate) fn sync_prompts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let front = derive_prompt_front(self.bridge_state());
        let fp = prompt_fingerprint(front.as_ref());
        let flight = front.as_ref().map(|f| f.response_in_flight);

        if fp == self.open_prompt_fp && flight == self.open_prompt_flight {
            return;
        }

        if front.is_none() {
            if self.open_prompt_fp.is_some() {
                window.close_dialog(cx);
                self.open_prompt_fp = None;
                self.open_prompt_flight = None;
            }
            return;
        }

        let front = front.unwrap();
        self.open_or_refresh_prompt(window, cx, &front);
        self.open_prompt_fp = prompt_fingerprint(Some(&front));
        self.open_prompt_flight = Some(front.response_in_flight);
    }

    fn open_or_refresh_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        front: &PromptFront,
    ) {
        let Some(session) = self.bridge_state().live_session.clone() else {
            return;
        };
        let entity = cx.entity().downgrade();
        let remaining = front.remaining;

        match front.kind {
            PromptKind::Approval => {
                let Some(approval) = find_approval(&session, &front.id).cloned() else {
                    return;
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
                open_approval_dialog(window, cx, approval, remaining, on_decide);
            }
            PromptKind::Interaction => {
                let Some(interaction) = find_interaction(&session, &front.id).cloned() else {
                    return;
                };
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
                open_interaction_dialog(window, cx, interaction, remaining, on_respond);
            }
        }
    }
}
