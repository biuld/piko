use super::*;

impl DesktopApp {
    pub fn new(
        bridge: ClientBridge,
        cwd: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer_input = cx.new(|cx| {
            InputState::new(window, cx)
                .auto_grow(3, 12)
                .placeholder(crate::t!("composer.placeholder"))
        });

        cx.subscribe_in(
            &composer_input,
            window,
            |this, _state, event, window, cx| {
                if let InputEvent::PressEnter { secondary } = event
                    && !secondary
                {
                    this.submit_composer(window, cx);
                }
            },
        )
        .detach();

        let host = cx.entity().downgrade();
        let sessions = cx.new(|cx| SessionsIsland::new(host.clone(), window, cx));
        sessions.update(cx, |island, cx| {
            island.subscribe_search(window, cx);
        });
        let timeline = cx.new(|cx| TimelineIsland::new(host.clone(), cx));
        let composer = cx.new(|cx| ComposerIsland::new(host.clone(), composer_input.clone(), cx));
        let agents = cx.new(|cx| AgentsIsland::new(host.clone(), cx));
        let tree = cx.new(|cx| TreeIsland::new(host.clone(), cx));

        let mut island_focus_table = IslandFocusTable::new();
        island_focus_table.register(IslandId::Sessions, sessions.clone());
        island_focus_table.register(IslandId::Timeline, timeline.clone());
        island_focus_table.register(IslandId::Composer, composer.clone());
        island_focus_table.register(IslandId::Agents, agents.clone());
        island_focus_table.register(IslandId::Tree, tree.clone());
        // Fail fast if a focusable island is missing from the chrome table.
        island_focus_table.assert_covers(&ALL_ISLAND_IDS);
        debug_assert_eq!(
            crate::app::archipelago::workbench_workspace()
                .focus_order
                .as_slice(),
            ALL_ISLAND_IDS.as_slice(),
            "workspace focus_order must list every IslandId in stable Tab sequence"
        );

        let section0 = SettingsSection::default();
        let settings_nav = cx.new(|cx| SettingsNavIsland::new(host.clone(), section0, cx));
        let settings_panel = cx.new(|cx| SettingsPanelIsland::new(host.clone(), section0, cx));
        let mut settings_focus_table = IslandFocusTable::new();
        settings_focus_table.register(SettingsIslandId::Nav, settings_nav.clone());
        settings_focus_table.register(SettingsIslandId::Panel, settings_panel.clone());
        settings_focus_table.assert_covers(
            crate::app::archipelago::settings_workspace()
                .focus_order
                .as_slice(),
        );
        debug_assert_eq!(
            crate::app::archipelago::settings_workspace()
                .focus_order
                .as_slice(),
            SETTINGS_FOCUS_ORDER.as_slice(),
        );
        debug_assert_eq!(
            crate::app::archipelago::workbench_workspace()
                .focus_order
                .as_slice(),
            ALL_ISLAND_IDS.as_slice(),
        );

        let entity = cx.entity().downgrade();
        let poll_entity = entity.clone();
        cx.spawn_in(window, async move |_window, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let Ok(()) = cx.update(|_, cx| {
                    if let Some(view) = poll_entity.upgrade() {
                        view.update(cx, |this, cx| {
                            let before_err = this.bridge.state().last_error.clone();
                            if this.bridge.poll() {
                                this.sync_gui_config(cx);
                                this.sync_host_runtime_config();
                                this.sync_command_catalog(cx);
                                this.on_bridge_polled(before_err);
                                this.sync_timeline_follow(cx);
                                if this.bridge.state().is_live() {
                                    this.prune_map_state_after_reconcile();
                                }
                                this.refresh_islands(cx);
                                let chrome_fp = this.chrome_fingerprint();
                                if this.last_chrome_fp.as_ref() != Some(&chrome_fp) {
                                    this.last_chrome_fp = Some(chrome_fp);
                                    cx.notify();
                                }
                            }
                        });
                    }
                }) else {
                    break;
                };
            }
        })
        .detach();

        cx.spawn_in(window, async move |_window, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(60))
                    .await;
                let Ok(()) = cx.update(|_, cx| {
                    let Some(view) = entity.upgrade() else {
                        return;
                    };
                    if !view.read(cx).notifications.records().is_empty() {
                        view.update(cx, |_, cx| cx.notify());
                    }
                }) else {
                    break;
                };
            }
        })
        .detach();

        Self {
            bridge,
            cwd,
            focus_handle: cx.focus_handle(),
            sessions,
            timeline,
            composer,
            agents,
            tree,
            composer_input,
            drafts: HashMap::new(),
            no_session_draft: String::new(),
            follow_bottom: HashMap::new(),
            timeline_offsets: HashMap::new(),
            last_selected_agent: None,
            last_timeline_fp: TimelineContentFp::default(),
            pending_scroll_bottom: false,
            submit_recovery: SubmitRecovery::default(),
            pending_first_submit: FirstSubmitRecovery::default(),
            clear_composer_on_render: false,
            overlay: OverlayHost::default(),
            overlay_focus_restore: None,
            interaction_form: None,
            command_palette: None,
            layout: LayoutState::default(),
            tree_preview_entry_id: None,
            tree_expanded_by_agent: HashMap::new(),
            pending_timeline_scroll_id: None,
            ux_prefs: GuiUxPrefs::default(),
            last_notified_error: None,
            last_connection_connected: true,
            notifications: AppNotificationCenter::default(),
            last_live_session_for_draft: None,
            gui_config_fingerprint: None,
            host_config_fingerprint: None,
            host_runtime: HostRuntimeSettings::default(),
            island_focus: IslandFocusRing::default(),
            island_focus_table,
            settings_nav,
            settings_panel,
            settings_focus: SettingsFocusRing::default(),
            settings_focus_table,
            fp_sessions: None,
            fp_timeline: None,
            fp_composer: None,
            fp_agents: None,
            fp_tree: None,
            last_chrome_fp: None,
            archipelago: AppArchipelago::new(ArchipelagoId::Workbench),
            last_settings_section: SettingsSection::default(),
            pinned_session_ids: HashSet::new(),
            session_last_used_at_ms: HashMap::new(),
            session_rename_input: None,
        }
    }

    pub(super) fn chrome_fingerprint(&self) -> String {
        format!(
            "{:?}|{:?}|{}|{}|{}|{:?}|{:?}",
            self.bridge.state().shell.connection,
            self.bridge.state().last_error,
            self.layout.sessions_open,
            self.layout.agents_open,
            self.layout.tree_open,
            self.island_focus.focused(),
            self.archipelago.active(),
        )
    }

    pub fn bootstrap(&mut self) {
        self.bridge.intent(ClientIntent::DiscoverSessions {
            scope: SessionListScope::All,
            cwd: None,
        });
        self.bridge.intent(ClientIntent::ListModels);
        self.bridge.intent(ClientIntent::SyncModelConfig);
        self.bridge.request_gui_config();
        self.bridge.request_host_config();
    }

    pub(crate) fn bridge_state(&self) -> &ClientState {
        self.bridge.state()
    }

    pub(crate) fn bridge_mut(&mut self) -> &mut ClientBridge {
        &mut self.bridge
    }

    pub(super) fn sync_gui_config(&mut self, cx: &mut Context<Self>) {
        let Some(value) = self.bridge.gui_config().cloned() else {
            return;
        };
        let fingerprint = value.to_string();
        if self.gui_config_fingerprint.as_ref() == Some(&fingerprint) {
            return;
        }
        match serde_json::from_value::<crate::config::GuiSettings>(value) {
            Ok(settings) => {
                self.layout.session_width = settings.session_width;
                self.layout.right_column_width = settings.right_column_width;
                self.layout.sessions_open = settings.session_open;
                self.layout.agents_open = settings.right_column_open;
                self.layout.tree_open = settings.right_column_open;
                self.ux_prefs.prefer_reduced_motion = settings.reduced_motion;
                self.ux_prefs.hide_thinking_block = settings.hide_thinking_block;
                let palette = crate::app::ux_prefs::parse_island_palette(&settings.island_palette);
                if self.ux_prefs.island_palette != palette {
                    self.ux_prefs.island_palette = palette;
                    island::theme::apply(cx, palette);
                }
                self.sync_session_prefs_from_gui(&settings);
                self.gui_config_fingerprint = Some(fingerprint);
            }
            Err(error) => {
                log::warn!("invalid [gui] settings ignored: {error}");
            }
        }
    }

    pub(crate) fn persist_gui_config(&mut self) {
        let mut settings = crate::config::GuiSettings {
            session_width: self.layout.session_width,
            right_column_width: self.layout.right_column_width,
            session_open: self.layout.sessions_open,
            right_column_open: self.layout.right_column_pref_open(),
            reduced_motion: self.ux_prefs.prefer_reduced_motion,
            island_palette: crate::app::ux_prefs::island_palette_key(self.ux_prefs.island_palette)
                .to_string(),
            hide_thinking_block: self.ux_prefs.hide_thinking_block,
            pinned_session_ids: Vec::new(),
            session_last_used_at_ms: HashMap::new(),
        };
        self.session_prefs_into_gui(&mut settings);
        if let Ok(value) = serde_json::to_value(settings) {
            self.gui_config_fingerprint = Some(value.to_string());
            self.bridge.update_gui_config(value);
        }
    }
}
