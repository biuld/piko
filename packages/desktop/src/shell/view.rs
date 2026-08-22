use super::rows::render_row;
use super::*;
use island::components::panel::{
    IslandPanel, IslandPlaceholder, PanelPresentation, PanelSurfaceRole,
};
use island::theme::IslandIcon;

/// Device-dependent wheel rounding still considered the Timeline tail.
const FOLLOW_EPSILON: f32 = 4.0;

fn is_at_tail(offset_y: f32, max_offset_y: f32) -> bool {
    max_offset_y <= FOLLOW_EPSILON || (max_offset_y + offset_y).abs() <= FOLLOW_EPSILON
}

impl Shell {
    fn sync_follow(&mut self) {
        let max = self.scroll.max_offset().y;
        let y = self.scroll.offset().y;
        let at_bottom = is_at_tail(f32::from(y), f32::from(max));
        if self.wheel_seen && !at_bottom {
            self.following = false;
        } else if at_bottom {
            self.following = true;
        }
        self.wheel_seen = false;
        if self.following && self.state.connection == DesktopConnection::Live {
            self.scroll.scroll_to_bottom();
        }
    }

    fn connection_color(&self) -> RoleAccent {
        match self.state.connection {
            DesktopConnection::Connecting | DesktopConnection::Hydrating => RoleAccent::Info,
            DesktopConnection::Live => RoleAccent::Success,
            DesktopConnection::Disconnected => RoleAccent::Danger,
            DesktopConnection::DecodeError => RoleAccent::Warning,
        }
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.reconcile_draft_target(window, cx);
        self.persist_presentation(window);
        let t = tokens();
        let width = window.bounds().size.width;
        let persistent_sidebar =
            sidebar::uses_persistent_sidebar(f32::from(width), self.prefs.sidebar_collapsed);
        let narrow = width < px(sidebar::SIDEBAR_WIDTH + sidebar::MIN_TIMELINE_WIDTH);

        let on_activate = {
            let entity = cx.entity().downgrade();
            std::rc::Rc::new(
                move |id: sidebar::NavId, _window: &mut Window, app: &mut App| {
                    if let Some(shell) = entity.upgrade() {
                        shell.update(app, |shell, cx| shell.activate_nav(id, cx));
                    }
                },
            )
        };

        self.sync_follow();
        let timeline_state = if self.pending_agent.is_some() {
            timeline::TimelineState::Loading
        } else if let Some(error) = self.selection_error.clone() {
            timeline::TimelineState::Error(error)
        } else {
            timeline::timeline_state(&self.state.core)
        };

        use island::components::chrome::ChromeZones;
        use island::components::workspace::{
            WindowChromeFrame, WorkspaceChrome, WorkspacePresentation,
        };

        let content = self.render_timeline_region(timeline_state, cx);
        let mut frame = WindowChromeFrame::new(content, self.render_chrome(cx, narrow))
            .presentation(WorkspacePresentation::Detached)
            .material(self.material);
        if persistent_sidebar {
            frame = frame.sidebar(
                sidebar::render_sidebar_content(
                    sidebar::nav_model(&self.state.core, self.sidebar_keyboard_focused),
                    &self.sidebar_scroll,
                    on_activate.clone(),
                ),
                WorkspaceChrome::new(ChromeZones::leading(
                    text(TextRole::PlaceholderTitle).child("Sessions"),
                ))
                .surface_role(SurfaceRole::Sidebar)
                .material(self.material),
                px(sidebar::SIDEBAR_WIDTH),
            );
        }

        div()
            .id("piko-shell")
            .size_full()
            .relative()
            .bg(fill(SurfaceRole::Canvas, self.material))
            .text_color(t.fg_rgba())
            .child(frame)
            .when(!persistent_sidebar && self.narrow_overlay_open, |root| {
                root.child(
                    div()
                        .absolute()
                        .left_2()
                        .top_2()
                        .bottom_2()
                        .w(px(sidebar::SIDEBAR_WIDTH))
                        .child(sidebar::render_sidebar_surface(
                            sidebar::nav_model(&self.state.core, self.sidebar_keyboard_focused),
                            self.material,
                            &self.sidebar_scroll,
                            on_activate,
                        )),
                )
            })
            .when_some(self.layers.active(), |root, layer| {
                root.child(self.render_temporary_layer(layer, window, cx))
            })
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                this.handle_shell_key(event, narrow, window, cx);
            }))
    }
}

impl Shell {
    fn render_chrome(
        &self,
        cx: &mut Context<Self>,
        narrow: bool,
    ) -> island::components::workspace::WorkspaceChrome {
        use island::components::chrome::ChromeZones;
        use island::components::workspace::WorkspaceChrome;

        let t = tokens();
        let m = metrics();
        let live_chrome = self.state.connection == DesktopConnection::Live;
        let session_count = self.state.session_count();
        let sidebar_visible = !narrow && !self.prefs.sidebar_collapsed;
        let attention_count = self
            .state
            .core
            .live_session
            .as_ref()
            .map(|session| session.pending_approvals.len() + session.pending_interactions.len())
            .unwrap_or(0);
        let navigation_label = if sidebar_visible || self.narrow_overlay_open {
            "Hide Sidebar"
        } else {
            "Show Sidebar"
        };

        let trailing = div()
            .flex()
            .items_center()
            .gap(m.space_sm)
            .child(
                text(TextRole::Meta)
                    .text_color(t.muted_fg_rgba())
                    .child(self.state.status.clone()),
            )
            .when(live_chrome, |bar| {
                bar.child(
                    text(TextRole::Meta)
                        .text_color(t.muted_fg_rgba())
                        .child(format!("{session_count} sessions")),
                )
            })
            .child(
                text(TextRole::Meta)
                    .text_color(t.role_accent(self.connection_color()))
                    .child(self.state.connection.label()),
            )
            .when(attention_count > 0, |bar| {
                bar.child(
                    div()
                        .id("piko-attention")
                        .px(m.space_sm)
                        .py(px(2.))
                        .rounded_sm()
                        .border_1()
                        .border_color(t.role_accent(RoleAccent::Warning))
                        .cursor_pointer()
                        .hover(|style| style.bg(highlight()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_layer(LayerKind::Attention, this.focus_owner, cx);
                        }))
                        .child(
                            text(TextRole::Label)
                                .child(format!("Needs attention · {attention_count}")),
                        ),
                )
            })
            .child(
                div()
                    .id("piko-sessions-toggle")
                    .px(m.space_sm)
                    .py(px(2.))
                    .rounded_sm()
                    .border_1()
                    .border_color(hairline(SurfaceRole::Chrome))
                    .cursor_pointer()
                    .hover(|style| style.bg(highlight()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if narrow {
                            this.narrow_overlay_open = !this.narrow_overlay_open;
                        } else {
                            this.prefs.sidebar_collapsed = !this.prefs.sidebar_collapsed;
                            this.narrow_overlay_open = false;
                            let _ = this.prefs.save(&this.prefs_path);
                        }
                        cx.notify();
                    }))
                    .child(text(TextRole::Label).child(navigation_label)),
            );

        WorkspaceChrome::new(
            ChromeZones::new(None, Some(trailing.into_any_element()), None)
                .principal(text(TextRole::PlaceholderTitle).child("piko")),
        )
        .material(self.material)
    }

    fn render_timeline_region(
        &mut self,
        state: timeline::TimelineState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let m = metrics();
        let input = self.composer_input.read(cx);
        let draft = input.value().to_string();
        let measured_input_height = f32::from(input.input_bounds().size.height);
        let composer_padding =
            composer::footprint_for_text(&draft, measured_input_height, !self.following);

        let panel = match &state {
            timeline::TimelineState::NoSession => IslandPanel::empty(
                "piko-timeline",
                IslandPlaceholder::new("No session selected")
                    .chrome_icon(IslandIcon::MessageSquare)
                    .subtitle("Pick a session in the sidebar to open its conversation."),
            ),
            timeline::TimelineState::Loading => IslandPanel::loading(
                "piko-timeline",
                IslandPlaceholder::new("Loading")
                    .chrome_icon(IslandIcon::CircleDashed)
                    .subtitle("Opening the selected session…"),
            ),
            timeline::TimelineState::Error(error) => IslandPanel::empty(
                "piko-timeline",
                IslandPlaceholder::new("Could not load conversation")
                    .chrome_icon(IslandIcon::TriangleAlert)
                    .subtitle(error.clone()),
            ),
            timeline::TimelineState::Empty => IslandPanel::empty(
                "piko-timeline",
                IslandPlaceholder::new("No messages yet")
                    .chrome_icon(IslandIcon::MessageSquare)
                    .subtitle("Start the conversation from the composer."),
            ),
            timeline::TimelineState::Ready(rows) => IslandPanel::new(
                "piko-timeline",
                div()
                    .id("piko-timeline-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .on_scroll_wheel(cx.listener(move |this, _, _, _| {
                        this.wheel_seen = true;
                        this.focus_owner = FocusOwner::Timeline;
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(m.space_md)
                            .px(m.space_lg)
                            .py(m.space_lg)
                            .pb(px(composer_padding))
                            .children(rows.iter().map(|row| render_row(row, &self.material))),
                    ),
            )
            .scroll(false),
        }
        .material(self.material)
        .surface_role(PanelSurfaceRole::Content)
        .presentation(PanelPresentation::Detached);

        let show_return = matches!(
            &state,
            timeline::TimelineState::Ready(_) | timeline::TimelineState::Empty
        ) && !self.following;

        // Fill the WindowChromeFrame content slot. `flex_1` here collapses
        // because that slot is not a flex column; a zero-height parent then
        // pins the absolutely positioned Composer into the title band.
        div()
            .id("piko-timeline-region")
            .size_full()
            .min_w(px(0.))
            .min_h(px(0.))
            .relative()
            .child(panel)
            .when(show_return, |region| {
                region.child(
                    div()
                        .id("piko-return-to-latest")
                        .absolute()
                        .bottom(px(composer_padding - 18.0))
                        .left(px(12.))
                        .right(px(12.))
                        .px(m.space_sm)
                        .py(px(2.))
                        .rounded_sm()
                        .border_1()
                        .border_color(hairline(SurfaceRole::Chrome))
                        .bg(fill(SurfaceRole::Elevated, self.material))
                        .cursor_pointer()
                        .hover(|style| style.bg(highlight()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.following = true;
                            this.scroll.scroll_to_bottom();
                            cx.notify();
                        }))
                        .child(text(TextRole::Meta).child("↓ Latest")),
                )
            })
            .child(self.render_composer(cx))
            .into_any_element()
    }

    fn render_composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let live = self.state.connection == DesktopConnection::Live
            && self.state.core.session_phase == SessionPhase::Live;
        let selected_agent = self
            .state
            .core
            .live_session
            .as_ref()
            .and_then(|session| session.selected_agent.as_deref());
        let running = self
            .state
            .core
            .live_session
            .as_ref()
            .is_some_and(|session| {
                session
                    .active_turns
                    .iter()
                    .any(|turn| Some(turn.agent_instance_id.as_str()) == selected_agent)
            });
        let model = self
            .state
            .core
            .model
            .model_id
            .clone()
            .unwrap_or_else(|| "Default model".to_string());
        let thinking = self
            .state
            .core
            .model
            .thinking_level
            .clone()
            .unwrap_or_else(|| "Default thinking".to_string());
        let context = self
            .state
            .core
            .live_session
            .as_ref()
            .and_then(|session| session.last_context_tokens)
            .zip(self.state.core.model.active_context_window())
            .map(|(used, window)| format!("Context {}%", used.saturating_mul(100) / window.max(1)))
            .unwrap_or_else(|| "Context —".to_string());
        let entity = cx.entity().downgrade();
        let model_layer = std::rc::Rc::new(move |_window: &mut Window, app: &mut App| {
            if let Some(shell) = entity.upgrade() {
                shell.update(app, |shell, cx| {
                    shell.focus_owner = FocusOwner::Composer;
                    shell.open_layer(LayerKind::Model, FocusOwner::Composer, cx);
                });
            }
        });
        let entity = cx.entity().downgrade();
        let thinking_layer = std::rc::Rc::new(move |_window: &mut Window, app: &mut App| {
            if let Some(shell) = entity.upgrade() {
                shell.update(app, |shell, cx| {
                    shell.focus_owner = FocusOwner::Composer;
                    shell.open_layer(LayerKind::Thinking, FocusOwner::Composer, cx);
                });
            }
        });
        let entity = cx.entity().downgrade();
        let submit = std::rc::Rc::new(move |window: &mut Window, app: &mut App| {
            if let Some(shell) = entity.upgrade() {
                shell.update(app, |shell, cx| shell.submit_composer(window, cx));
            }
        });
        let entity = cx.entity().downgrade();
        let cancel = std::rc::Rc::new(move |_window: &mut Window, app: &mut App| {
            if let Some(shell) = entity.upgrade() {
                shell.update(app, |shell, cx| shell.cancel_turn(cx));
            }
        });

        composer::ComposerView {
            input: self.composer_input.clone(),
            material: self.material,
            enabled: live && selected_agent.is_some(),
            running,
            pending: self.pending_submission.is_some(),
            model,
            thinking,
            context,
            error: self.composer_error.clone(),
            on_model: model_layer,
            on_thinking: thinking_layer,
            on_submit: submit,
            on_cancel: cancel,
        }
        .render()
    }
}

#[cfg(test)]
mod tests {
    use super::is_at_tail;

    #[test]
    fn gpui_negative_scroll_offsets_detect_the_tail() {
        assert!(is_at_tail(-100.0, 100.0));
        assert!(is_at_tail(-97.0, 100.0));
        assert!(!is_at_tail(-70.0, 100.0));
        assert!(is_at_tail(0.0, 0.0));
    }
}
