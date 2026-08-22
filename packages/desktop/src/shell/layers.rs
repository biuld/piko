use super::*;

impl Shell {
    pub(super) fn render_temporary_layer(
        &self,
        layer: LayerKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use island::components::overlay::{
            OverlayPanelSpec, OverlayPanelStyle, render_overlay_layer_on,
        };

        let m = metrics();
        let body = div()
            .flex()
            .flex_col()
            .gap(m.space_xs)
            .when(layer == LayerKind::Settings, |body| {
                body.child(
                    text(TextRole::Meta)
                        .text_color(tokens().muted_fg_rgba())
                        .child("Desktop settings are coming in a later feature slice."),
                )
            })
            .children(
                match layer {
                    LayerKind::Attention => self
                        .state
                        .core
                        .live_session
                        .as_ref()
                        .map(|session| {
                            let mut actions = Vec::new();
                            for approval in &session.pending_approvals {
                                actions.push((
                                    format!("Allow · {}", approval.tool_name),
                                    ClientIntent::RespondApproval {
                                        approval_id: approval.approval_id.clone(),
                                        decision: piko_protocol::ApprovalDecision::Accept,
                                        note: None,
                                    },
                                ));
                                actions.push((
                                    format!("Decline · {}", approval.tool_name),
                                    ClientIntent::RespondApproval {
                                        approval_id: approval.approval_id.clone(),
                                        decision: piko_protocol::ApprovalDecision::Decline,
                                        note: None,
                                    },
                                ));
                            }
                            for interaction in &session.pending_interactions {
                                let prompt = interaction
                                    .questions
                                    .first()
                                    .map(|question| question.prompt.as_str())
                                    .unwrap_or("interaction");
                                actions.push((
                                    format!("Dismiss · {prompt}"),
                                    ClientIntent::RespondInteraction {
                                        interaction_id: interaction.interaction_id.clone(),
                                        response: piko_protocol::UserInteractionResponse::Cancel {
                                            reason: None,
                                        },
                                    },
                                ));
                            }
                            actions
                        })
                        .unwrap_or_default(),
                    LayerKind::Settings | LayerKind::ChipDetail => Vec::new(),
                }
                .into_iter()
                .enumerate()
                .map(|(index, (label, intent))| {
                    let entity = cx.entity().downgrade();
                    div()
                        .id(("piko-layer-option", index))
                        .px(m.space_sm)
                        .py(px(6.))
                        .rounded_sm()
                        .cursor_pointer()
                        .hover(|style| style.bg(highlight()))
                        .on_click(move |_, window, app| {
                            if let Some(shell) = entity.upgrade() {
                                let intent = intent.clone();
                                shell.update(app, |shell, cx| {
                                    shell.dispatch_intents(cx, vec![intent]);
                                    shell.close_layer(window, cx);
                                });
                            }
                        })
                        .child(text(TextRole::Label).child(label))
                }),
            );
        let body = if layer == LayerKind::ChipDetail {
            self.render_chip_detail_body().into_any_element()
        } else {
            body.into_any_element()
        };
        let title = match layer {
            LayerKind::Attention => "Needs attention".to_string(),
            LayerKind::Settings => "Settings".to_string(),
            LayerKind::ChipDetail => self.chip_detail_title(),
        };
        let entity = cx.entity().downgrade();
        render_overlay_layer_on(
            OverlayPanelSpec {
                title: title.into(),
                width: px(460.0),
                viewport: Some(window.bounds().size),
                backdrop_dismiss: true,
                style: OverlayPanelStyle::Dialog,
            },
            self.material,
            body,
            move |_, window, app| {
                if let Some(shell) = entity.upgrade() {
                    shell.update(app, |shell, cx| shell.close_layer(window, cx));
                }
            },
        )
    }

    /// Chip-detail overlay content (F-46): thinking text snapshot or the
    /// structured tool body re-resolved from the live projection.
    fn render_chip_detail_body(&self) -> AnyElement {
        let m = metrics();
        match self.chip_detail.as_ref() {
            Some(crate::focus::ChipDetail::Thinking {
                text: thinking_text,
                ..
            }) => div()
                .id("piko-chip-thinking")
                .max_h(px(420.))
                .overflow_y_scroll()
                .child(
                    text(TextRole::BodyMono)
                        .text_color(tokens().muted_fg_rgba())
                        .child(thinking_text.clone()),
                )
                .into_any_element(),
            Some(crate::focus::ChipDetail::Tool { call_id, .. }) => {
                let Some(sections) = self.overlay_tool_sections(call_id) else {
                    return text(TextRole::Meta)
                        .text_color(tokens().muted_fg_rgba())
                        .child("This activity is no longer available.")
                        .into_any_element();
                };
                div()
                    .flex()
                    .flex_col()
                    .gap(m.space_xs)
                    .children(sections.iter().map(|(heading, kind)| {
                        div()
                            .flex()
                            .flex_col()
                            .gap(m.space_xs)
                            .child(
                                text(TextRole::Label)
                                    .text_color(tokens().muted_fg_rgba())
                                    .child(heading.clone()),
                            )
                            .child(self.tool_body_section_value(kind))
                    }))
                    .into_any_element()
            }
            None => text(TextRole::Meta)
                .text_color(tokens().muted_fg_rgba())
                .child("Nothing to show.")
                .into_any_element(),
        }
    }

    fn chip_detail_title(&self) -> String {
        match self.chip_detail.as_ref() {
            Some(crate::focus::ChipDetail::Thinking { .. }) => "Thinking".to_string(),
            Some(crate::focus::ChipDetail::Tool { name, status, .. }) => {
                format!("{name} · {status:?}")
            }
            None => "Activity".to_string(),
        }
    }
}

impl Shell {
    fn tool_body_section_value(&self, kind: &tool_body::ToolBodyKind) -> AnyElement {
        let m = metrics();
        let value: AnyElement = match kind {
            tool_body::ToolBodyKind::PrettyJson(json) => text(TextRole::BodyMono)
                .text_color(tokens().fg_rgba())
                .child(json.clone())
                .into_any_element(),
            tool_body::ToolBodyKind::Plain(body) => text(TextRole::BodyMono)
                .text_color(tokens().fg_rgba())
                .child(body.clone())
                .into_any_element(),
            tool_body::ToolBodyKind::KeyRows(rows) => {
                let mut col = div().flex().flex_col().gap(px(2.));
                for (key, value) in rows {
                    col = col.child(
                        div()
                            .flex()
                            .gap(m.space_xs)
                            .child(
                                text(TextRole::Label)
                                    .text_color(tokens().muted_fg_rgba())
                                    .child(format!("{key}:")),
                            )
                            .child(text(TextRole::Body).child(value.clone())),
                    );
                }
                col.into_any_element()
            }
        };
        div()
            .w_full()
            .px(m.space_sm)
            .py(m.space_xs)
            .rounded(m.radius_xs)
            .bg(fill(SurfaceRole::Content, self.material))
            .child(value)
            .into_any_element()
    }
}
