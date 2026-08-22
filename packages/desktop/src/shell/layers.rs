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
                    LayerKind::Model => self
                        .state
                        .core
                        .model
                        .providers
                        .iter()
                        .flat_map(|provider| {
                            provider.models.iter().map(move |model| {
                                (
                                    format!("{} · {}", provider.provider, model.name),
                                    ClientIntent::SetModel {
                                        provider: provider.provider.clone(),
                                        model_id: model.id.clone(),
                                    },
                                )
                            })
                        })
                        .collect::<Vec<_>>(),
                    LayerKind::Thinking => [
                        piko_protocol::ThinkingLevel::Off,
                        piko_protocol::ThinkingLevel::Minimal,
                        piko_protocol::ThinkingLevel::Low,
                        piko_protocol::ThinkingLevel::Medium,
                        piko_protocol::ThinkingLevel::High,
                        piko_protocol::ThinkingLevel::XHigh,
                        piko_protocol::ThinkingLevel::Max,
                    ]
                    .into_iter()
                    .map(|level| {
                        (
                            level.as_str().to_string(),
                            ClientIntent::SetThinkingLevel { level },
                        )
                    })
                    .collect(),
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
                    LayerKind::Settings => Vec::new(),
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
        let title = match layer {
            LayerKind::Model => "Choose model",
            LayerKind::Thinking => "Choose thinking level",
            LayerKind::Attention => "Needs attention",
            LayerKind::Settings => "Settings",
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
}
