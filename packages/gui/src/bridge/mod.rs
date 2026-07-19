//! Client Core bridge: owns `ClientState` + `HostTransport` and mediates
//! all foreground updates. The reader thread never mutates Core; only
//! `bridge.poll()` on the GPUI foreground does.
//!
//! Test builds may construct a bridge with `Option<HostTransport>` = None
//! and inject messages manually.

mod id_source;

#[cfg(test)]
mod tests;

pub use id_source::UuidCommandIdSource;

use anyhow::Result;
use piko_client_core::{
    ClientEffect, ClientIntent, ClientMsg, ClientState, CommandIdSource, TransportObservation,
    UpdateContext, update,
};
use piko_protocol::{Command, HostCommandDescriptor};

use crate::transport::{HostTransport, TransportEvent};

/// Wraps `ClientState` and an optional `HostTransport`, exposing a
/// foreground-only update surface for GPUI entities.
pub struct ClientBridge {
    state: ClientState,
    transport: Option<HostTransport>,
    id_source: Box<dyn CommandIdSource>,
    gui_config: Option<serde_json::Value>,
    host_config: Option<serde_json::Value>,
    command_catalog: Option<Vec<HostCommandDescriptor>>,
    /// Commands sent since the last `take_sent()` (diagnostic/test hook).
    #[cfg(test)]
    sent_log: Vec<Command>,
}

impl ClientBridge {
    /// Construct with a live transport (normal startup path).
    pub fn with_transport(transport: HostTransport, id_source: Box<dyn CommandIdSource>) -> Self {
        Self {
            state: ClientState::default(),
            transport: Some(transport),
            id_source,
            gui_config: None,
            host_config: None,
            command_catalog: None,
            #[cfg(test)]
            sent_log: Vec::new(),
        }
    }

    /// Construct without a transport (unit tests inject messages via `apply`).
    #[cfg(test)]
    pub fn headless(id_source: Box<dyn CommandIdSource>) -> Self {
        Self {
            state: ClientState::default(),
            transport: None,
            id_source,
            gui_config: None,
            host_config: None,
            command_catalog: None,
            sent_log: Vec::new(),
        }
    }

    // ── Read access ─────────────────────────────────────────────────────

    pub fn state(&self) -> &ClientState {
        &self.state
    }

    pub fn gui_config(&self) -> Option<&serde_json::Value> {
        self.gui_config.as_ref()
    }

    pub fn host_config(&self) -> Option<&serde_json::Value> {
        self.host_config.as_ref()
    }

    pub fn command_catalog(&self) -> Option<&Vec<HostCommandDescriptor>> {
        self.command_catalog.as_ref()
    }

    // ── Intent / dispatch ───────────────────────────────────────────────

    /// Apply a product intent through the Core reducer and execute effects.
    pub fn intent(&mut self, intent: ClientIntent) {
        self.apply(ClientMsg::Intent(intent));
    }

    /// Apply an arbitrary `ClientMsg` (used by `poll` and tests).
    pub fn apply(&mut self, msg: ClientMsg) {
        let mut ctx = UpdateContext {
            command_ids: self.id_source.as_mut(),
        };
        let (new_state, effects) = update(self.state.clone(), msg, &mut ctx);
        self.state = new_state;
        self.execute_effects(effects);
    }

    // ── Poll transport ──────────────────────────────────────────────────

    /// Non-blocking drain of transport events → Core messages. Returns true
    /// if any message was processed (callers may want to `cx.notify()`).
    pub fn poll(&mut self) -> bool {
        let Some(transport) = &self.transport else {
            return false;
        };
        let events = transport.drain();
        if events.is_empty() {
            return false;
        }
        for event in events {
            self.apply_transport_event(event);
        }
        true
    }

    fn apply_transport_event(&mut self, event: TransportEvent) {
        if let TransportEvent::Message(message) = &event {
            match message.as_ref() {
                piko_protocol::ServerMessage::CommandResponse {
                    result: Ok(piko_protocol::CommandResult::ConfigEntry { namespace, value }),
                    ..
                } if namespace == "gui" => {
                    self.gui_config = Some(value.clone());
                }
                piko_protocol::ServerMessage::CommandResponse {
                    result: Ok(piko_protocol::CommandResult::ConfigEntry { namespace, value }),
                    ..
                } if namespace == "host" => {
                    self.host_config = Some(value.clone());
                }
                piko_protocol::ServerMessage::CommandResponse {
                    result: Ok(piko_protocol::CommandResult::CommandCatalogListed { commands, .. }),
                    ..
                } => {
                    self.command_catalog = Some(commands.clone());
                }
                _ => {}
            }
        }
        self.apply(transport_event_to_msg(event));
    }

    // ── Bootstrap ───────────────────────────────────────────────────────

    /// Emit `TransportObservation::Connected` after a successful spawn.
    pub fn mark_connected(&mut self) {
        self.apply(ClientMsg::Transport(TransportObservation::Connected));
    }

    pub fn request_gui_config(&mut self) {
        let command_id = self.id_source.next_command_id();
        self.execute_effects(vec![ClientEffect::Send(Command::ConfigGet {
            command_id,
            namespace: "gui".into(),
        })]);
    }

    pub fn request_host_config(&mut self) {
        let command_id = self.id_source.next_command_id();
        self.execute_effects(vec![ClientEffect::Send(Command::ConfigGet {
            command_id,
            namespace: "host".into(),
        })]);
    }

    pub fn request_command_catalog(&mut self) {
        let command_id = self.id_source.next_command_id();
        self.execute_effects(vec![ClientEffect::Send(Command::CommandCatalogGet {
            command_id,
        })]);
    }

    pub fn update_gui_config(&mut self, value: serde_json::Value) {
        let command_id = self.id_source.next_command_id();
        self.execute_effects(vec![ClientEffect::Send(Command::ConfigUpdate {
            command_id,
            patch: serde_json::json!({ "gui": value }),
        })]);
    }

    pub fn update_host_config(&mut self, patch: serde_json::Value) {
        let command_id = self.id_source.next_command_id();
        self.execute_effects(vec![ClientEffect::Send(Command::ConfigUpdate {
            command_id,
            patch,
        })]);
    }

    // ── Shutdown ────────────────────────────────────────────────────────

    /// Shut down the host transport if present.
    pub fn shutdown(&mut self) {
        if let Some(t) = &mut self.transport {
            t.shutdown();
        }
        self.transport = None;
    }

    // ── Test helpers ────────────────────────────────────────────────────

    /// Drain the log of commands sent since the last call.
    #[cfg(test)]
    pub fn take_sent(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.sent_log)
    }

    // ── Internal ────────────────────────────────────────────────────────

    fn execute_effects(&mut self, effects: Vec<ClientEffect>) {
        let mut send_failure = None;
        for effect in effects {
            match effect {
                ClientEffect::Send(command) => {
                    #[cfg(test)]
                    self.sent_log.push(command.clone());
                    if let Some(t) = &mut self.transport
                        && let Err(e) = t.send(&command)
                    {
                        send_failure = Some(format!("transport send failed: {e}"));
                        break;
                    }
                }
            }
        }
        if let Some(detail) = send_failure {
            let mut ctx = UpdateContext {
                command_ids: self.id_source.as_mut(),
            };
            let (new_state, _) = update(
                self.state.clone(),
                ClientMsg::Transport(TransportObservation::SendFailure { detail }),
                &mut ctx,
            );
            self.state = new_state;
        }
    }
}

/// Spawn hostd and return a connected bridge.
pub fn spawn_bridge(args: &[String], env: &[(&str, &str)]) -> Result<ClientBridge> {
    let transport = HostTransport::spawn(args, env)?;
    let id_source = Box::new(UuidCommandIdSource);
    let mut bridge = ClientBridge::with_transport(transport, id_source);
    bridge.mark_connected();
    Ok(bridge)
}

fn transport_event_to_msg(event: TransportEvent) -> ClientMsg {
    match event {
        TransportEvent::Message(msg) => ClientMsg::Host(msg),
        TransportEvent::DecodeFailed(err) => {
            ClientMsg::Transport(TransportObservation::DecodeFailure {
                detail: err.to_string(),
            })
        }
        TransportEvent::Closed => ClientMsg::Transport(TransportObservation::Closed),
    }
}
