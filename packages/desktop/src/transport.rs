//! Host transport pipeline: lines become client-core messages, the reducer
//! produces effects, and the shell writes the resulting commands.

use std::time::Duration;

use piko_client_core::{ClientIntent, ClientMsg, CommandIdSource, UpdateContext, update};
use piko_comms::contracts::DesktopHostBridge;
use piko_comms::{HostLine, HostdClient};
use piko_protocol::{Command, SessionListScope};

use crate::state::DesktopState;

/// Maximum host lines consumed per pump cycle; a streaming host cannot
/// starve the window loop.
pub const DRAIN_LIMIT: usize = 256;

/// Pump cadence for the GPUI foreground task.
pub const PUMP_INTERVAL: Duration = Duration::from_millis(16);

/// Host client bound to the desktop process bridge.
pub type DesktopHostClient = HostdClient<DesktopHostBridge>;

/// Deterministic command ids for the client-core reducer.
#[derive(Debug, Default)]
pub struct CommandIds(u64);

impl CommandIdSource for CommandIds {
    fn next_command_id(&mut self) -> String {
        self.0 += 1;
        format!("desktop-{0}", self.0)
    }
}

/// Bootstrap intents: discover sessions, load the model catalog, and pull
/// host defaults before the shell presents anything as current.
pub fn bootstrap_messages() -> Vec<ClientMsg> {
    vec![
        ClientMsg::Intent(ClientIntent::DiscoverSessions {
            scope: SessionListScope::All,
            cwd: None,
        }),
        ClientMsg::Intent(ClientIntent::ListModels),
        ClientMsg::Intent(ClientIntent::SyncModelConfig),
    ]
}

/// Apply one client-core message to the projection, returning the commands
/// the frontend must write to the host.
pub fn reduce(
    state: &mut DesktopState,
    command_ids: &mut CommandIds,
    msg: ClientMsg,
) -> Vec<Command> {
    let mut ctx = UpdateContext { command_ids };
    let (core, effects) = update(std::mem::take(&mut state.core), msg, &mut ctx);
    state.core = core;
    effects
        .into_iter()
        .map(|effect| match effect {
            piko_client_core::ClientEffect::Send(command) => command,
        })
        .collect()
}

/// Map one drained host line to client-core messages and shell observation
/// side effects.
pub fn reduce_line(state: &mut DesktopState, line: HostLine) -> Vec<ClientMsg> {
    match line {
        HostLine::Message(message) => {
            state.on_host_message();
            vec![ClientMsg::Host(message)]
        }
        HostLine::DecodeError(detail) => {
            state.on_decode_error(detail.clone());
            vec![ClientMsg::Transport(
                piko_client_core::TransportObservation::DecodeFailure { detail },
            )]
        }
        HostLine::Closed => {
            state.on_closed();
            vec![ClientMsg::Transport(
                piko_client_core::TransportObservation::Closed,
            )]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_produces_three_intents() {
        assert_eq!(bootstrap_messages().len(), 3);
    }

    #[test]
    fn command_ids_are_deterministic_and_unique() {
        let mut ids = CommandIds::default();
        let first = ids.next_command_id();
        let second = ids.next_command_id();
        assert_ne!(first, second);
    }

    #[test]
    fn decode_error_maps_to_transport_observation() {
        let mut state = DesktopState::new();
        state.on_spawned();
        let messages = reduce_line(&mut state, HostLine::DecodeError("bad".to_string()));
        assert_eq!(
            state.connection,
            crate::connection::DesktopConnection::DecodeError
        );
        assert!(matches!(
            messages.as_slice(),
            [ClientMsg::Transport(
                piko_client_core::TransportObservation::DecodeFailure { .. }
            )]
        ));
    }

    #[test]
    fn closed_maps_to_disconnected_observation() {
        let mut state = DesktopState::new();
        state.on_spawned();
        let messages = reduce_line(&mut state, HostLine::Closed);
        assert_eq!(
            state.connection,
            crate::connection::DesktopConnection::Disconnected
        );
        assert!(matches!(
            messages.as_slice(),
            [ClientMsg::Transport(
                piko_client_core::TransportObservation::Closed
            )]
        ));
    }

    #[test]
    fn recorded_bootstrap_jsonl_reaches_an_empty_host_projection() {
        let mut state = DesktopState::new();
        state.on_spawned();
        let mut ids = CommandIds::default();
        for message in bootstrap_messages() {
            let _ = reduce(&mut state, &mut ids, message);
        }

        for line in include_str!("../tests/fixtures/bootstrap.jsonl").lines() {
            let host_line = piko_comms::decode_host_line(line);
            for message in reduce_line(&mut state, host_line) {
                let commands = reduce(&mut state, &mut ids, message);
                assert!(commands.is_empty());
            }
        }

        assert!(state.core.session_list.sessions.is_empty());
        assert!(state.core.model.providers.is_empty());
        assert!(state.core.pending_commands.is_empty());
        assert_eq!(
            state.connection,
            crate::connection::DesktopConnection::Hydrating
        );
    }
}
