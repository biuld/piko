//! Desktop shell root: two-column composition, host pumping, and the
//! Timeline surface (D-59 Slices 1–3).

mod composer;
mod keyboard;
mod layers;
mod lifecycle;
mod rows;
mod sidebar;
mod tabs;
mod timeline;
mod view;
mod workspace;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, AsyncApp, Context, FocusHandle, IntoElement, KeyDownEvent, Render,
    ScrollHandle, Styled, Subscription, Window, div, px,
};
use gpui_base::input::{InputEvent, TextareaState};
use island::components::list::ListKeyboard;
use island::platform::material::{MaterialPreference, WindowMaterialHost};
use island::theme::{
    RoleAccent, SurfaceRole, TextRole, fill, hairline, highlight, metrics, text, tokens,
};

use piko_client_core::{
    ClientIntent, ClientMsg, TransportObservation,
    state::{PendingOp, SessionPhase},
};
use piko_protocol::Command;

use crate::cli::CliArgs;
use crate::connection::DesktopConnection;
use crate::focus::{FocusOwner, LayerKind, TemporaryLayers};
use crate::prefs::{DesktopPrefs, WindowRect};
use crate::state::DesktopState;
use crate::transport::{
    CommandIds, DRAIN_LIMIT, DesktopHostClient, PUMP_INTERVAL, bootstrap_messages, reduce,
    reduce_line,
};

pub struct Shell {
    host: Option<DesktopHostClient>,
    state: DesktopState,
    command_ids: CommandIds,
    material: WindowMaterialHost,

    /// Agent this shell has subscribed to for live stream updates.
    subscribed_agent: Option<String>,
    pending_agent: Option<String>,
    selection_error: Option<String>,
    /// Timeline scroll viewport (shell-local; never product-authoritative).
    scroll: ScrollHandle,
    /// Sidebar viewport, used to keep keyboard navigation visible.
    sidebar_scroll: ScrollHandle,
    /// A wheel event arrived since the last frame (candidate Reading flip).
    wheel_seen: bool,
    /// Tail-following versus reading (F-42 Timeline).
    following: bool,
    /// Narrow-window temporary navigation layer is open.
    narrow_overlay_open: bool,
    sidebar_keyboard: ListKeyboard,
    sidebar_keyboard_focused: Option<sidebar::NavId>,
    /// Recoverable draft editor; its selection and undo state survive paints.
    composer_input: gpui::Entity<TextareaState>,
    drafts: HashMap<String, String>,
    draft_key: String,
    pending_submission: Option<composer::PendingSubmission>,
    clear_accepted_draft: Option<String>,
    composer_error: Option<String>,
    focus_owner: FocusOwner,
    agent_tabs_focus: FocusHandle,
    layers: TemporaryLayers,
    prefs_path: PathBuf,
    prefs: DesktopPrefs,
    last_saved_window: Option<WindowRect>,
    warm_reopen_attempted: bool,
    workspace_cwd: String,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        args: CliArgs,
        prefs_path: PathBuf,
        prefs: DesktopPrefs,
    ) -> Self {
        let material = WindowMaterialHost::install(window, MaterialPreference::NativeOrOpaque);
        let composer_input = composer::new_input(window, cx);
        let composer_events = cx.subscribe_in(
            &composer_input,
            window,
            |shell, input, event: &InputEvent, window, cx| {
                if composer::is_submit_event(event) {
                    shell.submit_composer(window, cx);
                } else if matches!(event, InputEvent::Change) {
                    shell.set_focus_owner(FocusOwner::Composer, window, cx);
                    shell
                        .drafts
                        .insert(shell.draft_key.clone(), input.read(cx).value().to_string());
                    shell.composer_error = None;
                    cx.notify();
                }
            },
        );
        let mut shell = Self {
            host: None,
            state: DesktopState::new(),
            command_ids: CommandIds::default(),
            material,
            subscribed_agent: None,
            pending_agent: None,
            selection_error: None,
            scroll: ScrollHandle::new(),
            sidebar_scroll: ScrollHandle::new(),
            wheel_seen: false,
            following: true,
            narrow_overlay_open: false,
            sidebar_keyboard: ListKeyboard::new(),
            sidebar_keyboard_focused: None,
            composer_input,
            drafts: HashMap::new(),
            draft_key: "no-session".to_string(),
            pending_submission: None,
            clear_accepted_draft: None,
            composer_error: None,
            focus_owner: FocusOwner::Timeline,
            agent_tabs_focus: cx.focus_handle(),
            layers: TemporaryLayers::default(),
            prefs_path,
            last_saved_window: prefs.window,
            warm_reopen_attempted: false,
            workspace_cwd: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            prefs,
            _subscriptions: vec![composer_events],
        };
        match DesktopHostClient::spawn(
            args.hostd_command,
            args.hostd_args,
            args.log_level.as_deref(),
        ) {
            Ok(host) => {
                shell.host = Some(host);
                shell.bootstrap(cx);
            }
            Err(err) => {
                shell.state.on_spawn_failure(err.to_string());
            }
        }
        shell.start_host_pump(cx);
        shell
    }

    fn bootstrap(&mut self, cx: &mut Context<Self>) {
        self.state.on_spawned();
        let mut commands = Vec::new();
        for msg in bootstrap_messages() {
            commands.extend(reduce(&mut self.state, &mut self.command_ids, msg));
        }
        self.send_commands(commands);
        self.maintain_subscription();
        cx.notify();
    }

    fn start_host_pump(&mut self, cx: &mut Context<Self>) {
        cx.spawn(
            async move |shell: gpui::WeakEntity<Shell>, cx: &mut AsyncApp| {
                loop {
                    let alive = shell.update(cx, |shell, cx| shell.service_host(cx)).is_ok();
                    if !alive {
                        break;
                    }
                    cx.background_executor().timer(PUMP_INTERVAL).await;
                }
            },
        )
        .detach();
    }

    fn service_host(&mut self, cx: &mut Context<Self>) {
        let Some(lines) = self.host.as_mut().map(|host| host.drain_up_to(DRAIN_LIMIT)) else {
            return;
        };
        if lines.is_empty() {
            return;
        }
        let mut reconnect = false;
        for line in lines {
            if matches!(line, piko_comms::HostLine::Message(_)) {
                reconnect = true;
            }
            for msg in reduce_line(&mut self.state, line) {
                let commands = reduce(&mut self.state, &mut self.command_ids, msg);
                self.send_commands(commands);
            }
            self.reconcile_submission();
            self.reconcile_agent_selection(cx);
        }
        if reconnect {
            // New authoritative traffic after a decode error recovers the
            // shell state machine inside `reduce_line`.
        }
        if self.state.connection == DesktopConnection::Disconnected {
            self.subscribed_agent = None;
        }
        self.maybe_warm_reopen();
        self.finish_hydration_if_ready();
        self.remember_live_session();
        self.maintain_subscription();
        cx.notify();
    }

    fn send_commands(&mut self, commands: Vec<Command>) {
        let Some(host) = self.host.as_mut() else {
            return;
        };
        for command in commands {
            if let Err(err) = host.send(command) {
                let detail = err.to_string();
                self.state.on_send_failure(detail.clone());
                let _ = reduce(
                    &mut self.state,
                    &mut self.command_ids,
                    ClientMsg::Transport(TransportObservation::SendFailure { detail }),
                );
            }
        }
    }

    fn dispatch_intents(&mut self, cx: &mut Context<Self>, intents: Vec<ClientIntent>) {
        for intent in intents {
            let commands = reduce(
                &mut self.state,
                &mut self.command_ids,
                ClientMsg::Intent(intent),
            );
            self.send_commands(commands);
        }
        cx.notify();
    }

    fn submit_composer(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.state.connection != DesktopConnection::Live
            || self.state.core.session_phase != SessionPhase::Live
            || self.pending_agent.is_some()
            || self.pending_submission.is_some()
        {
            return;
        }
        let value = self.composer_input.read(cx).value().to_string();
        let text = value.trim().to_string();
        if text.is_empty() {
            return;
        }
        let before: HashSet<_> = self.state.core.pending_commands.keys().cloned().collect();
        let commands = reduce(
            &mut self.state,
            &mut self.command_ids,
            ClientMsg::Intent(ClientIntent::SubmitTurn { text: text.clone() }),
        );
        let command_id = self
            .state
            .core
            .pending_commands
            .iter()
            .find_map(|(id, operation)| {
                (!before.contains(id) && matches!(operation, PendingOp::Submit)).then(|| id.clone())
            });
        let Some(command_id) = command_id else {
            return;
        };
        self.pending_submission = Some(composer::PendingSubmission { command_id, text });
        self.composer_error = None;
        self.send_commands(commands);
        cx.notify();
    }

    fn reconcile_submission(&mut self) {
        let Some(pending) = self.pending_submission.as_ref() else {
            return;
        };
        if self
            .state
            .core
            .pending_commands
            .contains_key(&pending.command_id)
        {
            return;
        }
        if let Some(failure) = self
            .state
            .core
            .command_failures
            .iter()
            .rev()
            .find(|failure| failure.command_id == pending.command_id)
        {
            self.composer_error = Some(format!("Send failed: {}", failure.message));
        } else {
            self.clear_accepted_draft = Some(pending.text.clone());
        }
        self.pending_submission = None;
    }

    fn reconcile_agent_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(target) = self.pending_agent.as_deref() {
            let listed = self
                .state
                .core
                .live_session
                .as_ref()
                .is_some_and(|session| {
                    session
                        .agents
                        .iter()
                        .any(|agent| agent.agent_instance_id == target)
                });
            if !listed {
                self.pending_agent = None;
                return;
            }
        }
        let Some(target) = self.pending_agent.clone() else {
            return;
        };
        let still_pending = self
            .state
            .core
            .pending_commands
            .values()
            .any(|operation| matches!(operation, PendingOp::SelectAgent { .. }));
        if still_pending {
            return;
        }
        self.selection_error = self
            .state
            .core
            .command_failures
            .iter()
            .rev()
            .find_map(|failure| match &failure.operation {
                PendingOp::SelectAgent { agent_instance_id } if agent_instance_id == &target => {
                    Some(failure.message.clone())
                }
                _ => None,
            });
        let host_selected = self
            .state
            .core
            .live_session
            .as_ref()
            .and_then(|session| session.selected_agent.clone());
        if self.selection_error.is_some() {
            self.subscribed_agent = host_selected;
            self.pending_agent = None;
            return;
        }
        if host_selected.as_deref() == Some(target.as_str()) {
            self.pending_agent = None;
            return;
        }
        self.dispatch_intents(
            cx,
            vec![ClientIntent::SelectAgent {
                agent_instance_id: target,
            }],
        );
    }

    fn cancel_turn(&mut self, cx: &mut Context<Self>) {
        self.dispatch_intents(cx, vec![ClientIntent::CancelTurn]);
    }

    fn open_layer(&mut self, layer: LayerKind, initiating: FocusOwner, cx: &mut Context<Self>) {
        self.layers.open(layer, initiating);
        cx.notify();
    }

    fn close_layer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.layers.close() {
            Some(FocusOwner::Composer) => {
                self.set_focus_owner(FocusOwner::Composer, window, cx);
            }
            Some(FocusOwner::AgentTabs) => {
                self.set_focus_owner(FocusOwner::AgentTabs, window, cx);
            }
            Some(owner) => {
                self.set_focus_owner(owner, window, cx);
            }
            None => {}
        }
        cx.notify();
    }

    fn current_draft_key(&self) -> String {
        let Some(session) = self.state.core.live_session.as_ref() else {
            return "no-session".to_string();
        };
        let agent = self
            .pending_agent
            .as_deref()
            .or(session.selected_agent.as_deref())
            .unwrap_or("session");
        format!("{}:{}", session.session_id, agent)
    }

    fn reconcile_draft_target(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next_key = self.current_draft_key();
        if next_key != self.draft_key {
            let current = self.composer_input.read(cx).value().to_string();
            self.drafts.insert(self.draft_key.clone(), current);
            let next = self.drafts.get(&next_key).cloned().unwrap_or_default();
            self.composer_input
                .update(cx, |input, cx| input.set_value(next, window, cx));
            self.draft_key = next_key;
            self.composer_error = None;
        }
        if let Some(submitted) = self.clear_accepted_draft.take() {
            let current = self.composer_input.read(cx).value().to_string();
            if composer::should_clear_accepted_draft(&current, &submitted) {
                self.composer_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.drafts.insert(self.draft_key.clone(), String::new());
            }
        }
    }

    fn persist_presentation(&mut self, window: &Window) {
        let rect = WindowRect::from_window(window.window_bounds());
        if self.last_saved_window == Some(rect) {
            return;
        }
        self.prefs.window = Some(rect);
        if self.prefs.save(&self.prefs_path).is_ok() {
            self.last_saved_window = Some(rect);
        }
    }

    /// Subscribe to the live selected agent once a session reconciles so
    /// StreamItem updates reach the projection (F-22).
    fn maintain_subscription(&mut self) {
        if self.state.core.session_phase != SessionPhase::Live {
            return;
        }
        let desired = self
            .state
            .core
            .live_session
            .as_ref()
            .and_then(|session| session.selected_agent.clone());
        let Some(desired) = desired else {
            return;
        };
        if self.subscribed_agent.as_deref() == Some(desired.as_str()) {
            return;
        }
        let commands = reduce(
            &mut self.state,
            &mut self.command_ids,
            ClientMsg::Intent(ClientIntent::SelectAgent {
                agent_instance_id: desired.clone(),
            }),
        );
        self.subscribed_agent = Some(desired);
        self.send_commands(commands);
    }
}
