use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::api::{Command, ServerMessage};
use crate::application::HostApp;
use crate::domain::config::HostSettings;
use crate::infra::storage::JsonlSessionRepository;
use crate::ports::AgentRunRunner;
use crate::util::{ClientEventReceiver, ClientEventSender};

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

    pub fn with_agent_runner(agent_runner: Arc<dyn AgentRunRunner>) -> Self {
        Self(HostApp::with_agent_runner(agent_runner))
    }

    pub fn with_storage_and_runner(
        storage: JsonlSessionRepository,
        agent_runner: Arc<dyn AgentRunRunner>,
    ) -> Self {
        Self(HostApp::with_storage_and_runner(storage, agent_runner))
    }

    pub fn with_storage_runner_settings(
        storage: JsonlSessionRepository,
        agent_runner: Arc<dyn AgentRunRunner>,
        settings: HostSettings,
    ) -> Self {
        Self(HostApp::with_storage_runner_settings(
            storage,
            agent_runner,
            settings,
        ))
    }

    /// Rebuild the orch turn runner from current settings and on-disk auth.
    ///
    /// Startup and model config changes share this path. Auth login/logout must
    /// also call it so an `ErrorAgentRunRunner` installed when credentials were
    /// missing is replaced after keys land in `auth.json`.
    ///
    /// The whole runtime bundle {runner, executor, active_model} is swapped
    /// atomically; a failed build clears the previous executor so compaction
    /// cannot keep calling a stale gateway (P2-5).
    pub(crate) async fn rebuild_agent_runner(&self) {
        let settings = self.settings.lock().await.clone();
        self.rebuild_agent_runner_with(settings).await;
    }

    /// [`Self::rebuild_agent_runner`] with caller-supplied settings — used by
    /// `apply_config_update`, which holds the settings lock while observers
    /// run and must rebuild against the candidate settings.
    pub(crate) async fn rebuild_agent_runner_with(&self, settings: HostSettings) {
        use crate::ports::ErrorAgentRunRunner;

        let (runner, executor, active_model) = super::build_orch_agent_runner(&settings)
            .await
            .unwrap_or_else(|e| {
                (
                    Arc::new(ErrorAgentRunRunner::new(e)) as Arc<dyn AgentRunRunner>,
                    None,
                    None,
                )
            });
        // Finish configuring the replacement before publishing it. A command
        // can capture the bundle immediately after the swap.
        self.wire_context_window_callback_for(&runner);
        self.wire_guardian_callback_for(&runner);
        self.swap_agent_runtime_bundle(runner, executor, active_model)
            .await;
    }

    pub async fn handle_command(&self, command: Command) -> Vec<ServerMessage> {
        let mut rx = self.handle_command_stream(command);
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        events
    }

    pub fn handle_command_stream(&self, command: Command) -> ClientEventReceiver {
        let server = self.clone();
        let (tx, rx): (ClientEventSender, ClientEventReceiver) =
            piko_comms::mailbox::<piko_comms::contracts::HostCommandOutput, _>();
        tokio::spawn(async move {
            server.handle_command_into(command, tx).await;
        });
        rx
    }

    pub async fn handle_command_into(&self, command: Command, tx: ClientEventSender) {
        let command_id = command.command_id().to_string();
        if let Err(err) = self
            .apply_command_stream(command, command_id.clone(), &tx)
            .await
        {
            send_event(
                &tx,
                ServerMessage::CommandResponse {
                    command_id,
                    result: Err(err.to_string()),
                },
            )
            .await;
        }
    }
}
