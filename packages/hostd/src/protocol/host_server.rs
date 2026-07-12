use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::api::{Command, ServerMessage};
use crate::application::HostApp;
use crate::domain::config::HostSettings;
use crate::infra::storage::JsonlSessionRepository;
use crate::ports::TurnRunner;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::send_event;

/// Thin composition/delivery wrapper around [`HostApp`].
///
/// `HostServer` owns command routing and transport framing; all use-case
/// state and orchestration lives on `HostApp` (see `application::host_app`).
/// Field/method access on `HostApp` is available here through `Deref` /
/// `DerefMut`, but the streaming command dispatch below calls into the
/// wrapped `HostApp` explicitly (`self.0.method(...)`) to keep the
/// protocol → application call boundary visible.
#[derive(Clone)]
pub struct HostServer(pub(crate) HostApp);

impl Deref for HostServer {
    type Target = HostApp;

    fn deref(&self) -> &HostApp {
        &self.0
    }
}

impl DerefMut for HostServer {
    fn deref_mut(&mut self) -> &mut HostApp {
        &mut self.0
    }
}

impl Default for HostServer {
    fn default() -> Self {
        Self::new()
    }
}

impl HostServer {
    pub fn new() -> Self {
        Self(HostApp::new())
    }

    pub fn with_storage(storage: JsonlSessionRepository) -> Self {
        Self(HostApp::with_storage(storage))
    }

    pub fn with_turn_runner(turn_runner: Arc<dyn TurnRunner>) -> Self {
        Self(HostApp::with_turn_runner(turn_runner))
    }

    pub fn with_storage_and_runner(
        storage: JsonlSessionRepository,
        turn_runner: Arc<dyn TurnRunner>,
    ) -> Self {
        Self(HostApp::with_storage_and_runner(storage, turn_runner))
    }

    pub fn with_storage_runner_settings(
        storage: JsonlSessionRepository,
        turn_runner: Arc<dyn TurnRunner>,
        settings: HostSettings,
    ) -> Self {
        Self(HostApp::with_storage_runner_settings(
            storage,
            turn_runner,
            settings,
        ))
    }

    pub async fn handle_command(&self, command: Command) -> Vec<ServerMessage> {
        let mut rx = self.handle_command_stream(command);
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        events
    }

    pub fn handle_command_stream(&self, command: Command) -> UnboundedReceiver<ServerMessage> {
        let command_id = command.command_id().to_string();
        let server = self.clone();
        let (tx, rx) = unbounded_channel();
        tokio::spawn(async move {
            if let Err(err) = server
                .apply_command_stream(command, command_id.clone(), &tx)
                .await
            {
                send_event(
                    &tx,
                    ServerMessage::CommandResponse {
                        command_id: command_id.clone(),
                        result: Err(err.to_string()),
                    },
                );
            }
        });
        rx
    }
}
