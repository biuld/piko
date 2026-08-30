use super::canvas::row_gap_before;
use super::*;
use gpui::{FollowMode, list};
use island::components::chrome::{ChromeTextEmphasis, GhostTextButton};
use island::components::panel::{
    IslandPanel, IslandPlaceholder, PanelPresentation, PanelSurfaceRole,
};
use island::theme::IslandIcon;

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
                move |id: sidebar::NavId, window: &mut Window, app: &mut App| {
                    if let Some(shell) = entity.upgrade() {
                        shell.update(app, |shell, cx| shell.activate_nav(id, window, cx));
                    }
                },
            )
        };

        use island::components::chrome::ChromeZones;
        use island::components::workspace::{
            WindowChromeFrame, WorkspaceChrome, WorkspacePresentation,
        };

        let content = self.render_timeline_region(cx);
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
                WorkspaceChrome::new(
                    ChromeZones::empty().pinned(self.sidebar_toggle_icon(cx, false)),
                )
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
    fn toggle_sidebar(&mut self, narrow: bool, cx: &mut Context<Self>) {
        if narrow {
            self.narrow_overlay_open = !self.narrow_overlay_open;
        } else {
            self.prefs.sidebar_collapsed = !self.prefs.sidebar_collapsed;
            self.narrow_overlay_open = false;
            let _ = self.prefs.save(&self.prefs_path);
        }
        cx.notify();
    }

    fn sidebar_toggle_icon(&self, cx: &mut Context<Self>, show: bool) -> AnyElement {
        use island::components::chrome::GhostIconButton;
        let icon = if show {
            IslandIcon::PanelLeft
        } else {
            IslandIcon::PanelLeftFilled
        };
        let tooltip = if show { "Show Sidebar" } else { "Hide Sidebar" };
        let t = tokens();
        GhostIconButton::new("piko-sessions-toggle", icon, t.fg_rgba())
            .tooltip(tooltip)
            .material(self.material)
            .on_click(cx.listener(move |this, _, window, cx| {
                let narrow = f32::from(window.bounds().size.width)
                    < sidebar::SIDEBAR_WIDTH + sidebar::MIN_TIMELINE_WIDTH;
                this.toggle_sidebar(narrow, cx);
            }))
            .into_any_element()
    }

    fn render_chrome(
        &self,
        cx: &mut Context<Self>,
        narrow: bool,
    ) -> island::components::workspace::WorkspaceChrome {
        use island::components::chrome::ChromeZones;
        use island::components::workspace::WorkspaceChrome;

        let mut zones = ChromeZones::empty();
        if let Some(tabs) = self.agent_tab_group(cx) {
            // Principal is unwrapped so TabGroup can paint a single hugging capsule.
            zones = zones.principal(tabs);
        }
        // Model/thinking pickers moved into the composer header (F-47).
        if let Some(attention) = self.attention_control(cx) {
            zones = zones.append_trailing(attention);
        }
        if narrow || self.prefs.sidebar_collapsed {
            zones = zones.prepend_leading(self.sidebar_toggle_icon(cx, true));
        }

        WorkspaceChrome::new(zones).material(self.material)
    }

    /// Header control to jump back to the newest message. Replaces the floating
    /// "↓ Latest" pill; shown only after the reader has scrolled off the tail.
    fn latest_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.following() || self.timeline_list.item_count() == 0 {
            return None;
        }
        let entity = cx.entity().downgrade();
        Some(
            GhostTextButton::new("piko-latest", "Latest")
                .emphasis(ChromeTextEmphasis::Foreground)
                .leading_icon(IslandIcon::ArrowDown)
                .capsule(true)
                .tooltip("Jump to the latest message")
                .material(self.material)
                .on_click(move |_, _, app| {
                    if let Some(shell) = entity.upgrade() {
                        shell.update(app, |shell, cx| {
                            shell.view_local().following = true;
                            shell.timeline_list.set_follow_mode(FollowMode::Tail);
                            cx.notify();
                        });
                    }
                })
                .into_any_element(),
        )
    }

    fn render_timeline_region(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let input = self.composer_input.read(cx);
        let draft = input.value().to_string();
        let composer_padding = composer::footprint_for_text(&draft);
        let (state, frame) = if self.pending_agent.is_some() {
            (
                timeline::TimelineState::Loading,
                timeline::FrameTimeline::empty(composer_padding),
            )
        } else if let Some(error) = self.selection_error.clone() {
            (
                timeline::TimelineState::Error(error),
                timeline::FrameTimeline::empty(composer_padding),
            )
        } else {
            timeline::frame_timeline(&self.state.core, composer_padding)
        };
        // Shared with list item callbacks for this frame; they must not
        // rebuild the projection per visible row (scroll performance).
        self.frame_timeline = frame.clone();
        let ready = matches!(&state, timeline::TimelineState::Ready);

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
            timeline::TimelineState::Ready => {
                let n = frame.total();
                self.sync_timeline_list_len(n);
                if frame.streaming {
                    self.timeline_list.remeasure_items(n.saturating_sub(2)..n);
                }
                let entity = cx.entity().downgrade();
                IslandPanel::new(
                    "piko-timeline",
                    div()
                        .id("piko-timeline-scroll")
                        .size_full()
                        .min_h(px(0.))
                        .min_w(px(0.))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.set_focus_owner(FocusOwner::Timeline, window, cx);
                            }),
                        )
                        .child(
                            list(self.timeline_list.clone(), move |ix, window, cx| {
                                entity
                                    .upgrade()
                                    .map(|shell| {
                                        shell.update(cx, |shell, cx| {
                                            shell.render_timeline_index(ix, window, cx)
                                        })
                                    })
                                    .unwrap_or_else(|| div().into_any_element())
                            })
                            .w_full()
                            .h_full(),
                        ),
                )
                .scroll(false)
            }
        }
        .material(self.material)
        .surface_role(PanelSurfaceRole::Content)
        .presentation(PanelPresentation::Hosted);

        div()
            .id("piko-timeline-region")
            .size_full()
            .min_w(px(0.))
            .min_h(px(0.))
            .relative()
            .overflow_hidden()
            .child(
                div()
                    .size_full()
                    .overflow_hidden()
                    .when(!ready, |body| body.pb(px(composer_padding)))
                    .child(panel),
            )
            .child(self.render_composer(cx))
            .into_any_element()
    }

    fn sync_timeline_list_len(&self, n: usize) {
        let old = self.timeline_list.item_count();
        if n == old {
            return;
        }
        if n > old {
            self.timeline_list.splice(old..old, n - old);
        } else {
            self.timeline_list.reset(n);
        }
    }

    fn render_timeline_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Payloads map on demand for this row (and its predecessor for the
        // gap); the projection is never rebuilt per visible item.
        let frame = self.frame_timeline.clone();
        let Some((prev, row)) = timeline::rows_around(&self.state.core, &frame, ix) else {
            return div().into_any_element();
        };
        let m = metrics();
        let gap = row_gap_before(prev.as_ref(), &row);
        let total = frame.total();
        let padding = frame.composer_padding;
        let row_el = self.render_timeline_row(&row, gap, _window, cx);
        div()
            .w_full()
            .flex()
            .justify_center()
            .child(
                div()
                    .w_full()
                    .max_w(m.reading_width)
                    .px(m.space_lg)
                    .when(ix == 0, |el| el.pt(m.space_lg))
                    .when(ix + 1 == total, |el| el.pb(px(padding)))
                    .child(row_el),
            )
            .into_any_element()
    }

    fn render_composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let live = self.state.connection == DesktopConnection::Live
            && self.state.core.session_phase == SessionPhase::Live;
        let view_key = self.view_key();
        let running = self
            .state
            .core
            .live_session
            .as_ref()
            .is_some_and(|session| {
                view_key.is_some_and(|id| {
                    session
                        .agent_work
                        .get(id)
                        .is_some_and(|work| work.active_work.is_some())
                })
            });
        let context = self
            .state
            .core
            .live_session
            .as_ref()
            .and_then(|session| session.last_context_tokens)
            .zip(self.state.core.model.active_context_window())
            .map(|(used, window)| composer::ComposerContext::Fill { used, window })
            .unwrap_or(composer::ComposerContext::Unknown);
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

        let entity = cx.entity().downgrade();
        let on_attach: composer::ComposerAction = std::rc::Rc::new(move |_, app| {
            let Some(shell) = entity.upgrade() else {
                return;
            };
            let rx = app.prompt_for_paths(gpui::PathPromptOptions {
                files: true,
                directories: false,
                multiple: true,
                prompt: None,
            });
            app.spawn(async move |cx: &mut gpui::AsyncApp| {
                let picked = match rx.await {
                    Ok(Ok(Some(paths))) => Some(paths),
                    _ => None,
                };
                if let Some(paths) = picked
                    && !paths.is_empty()
                {
                    shell.update(cx, |shell, cx| shell.add_attachments(paths, cx));
                }
            })
            .detach();
        });
        let entity = cx.entity().downgrade();
        let on_remove_attachment: composer::RemoveAttachmentAction =
            std::rc::Rc::new(move |id, _, app| {
                if let Some(shell) = entity.upgrade() {
                    shell.update(app, |shell, cx| shell.remove_attachment(id, cx));
                }
            });
        let model_button = self.model_picker(cx);
        let thinking_button = self.thinking_picker(cx);

        composer::ComposerView {
            input: self.composer_input.clone(),
            input_height: {
                let draft = self.composer_input.read(cx).value().to_string();
                composer::input_box_height(&draft)
            },
            latest_button: self.latest_button(cx),
            material: self.material,
            enabled: live && view_key.is_some() && self.pending_agent.is_none(),
            running,
            pending: self.pending_submission().is_some(),
            context,
            error: self.composer_error(),
            attachments: self.view_attachments(),
            model_button,
            thinking_button,
            on_submit: submit,
            on_cancel: cancel,
            on_attach,
            on_remove_attachment,
        }
        .render()
    }
}
