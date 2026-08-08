use piko_orchd::tools::{
    UserInteractionCallbacks, UserInteractionProvider, UserInteractionRequest,
};
use piko_protocol::tools::{ToolSet, ToolSetToolRef};

use super::*;

impl OrchAgentRunRunner {
    pub(super) fn register_session_context(&self, session_id: String, cwd: String) {
        self.session_contexts
            .lock()
            .unwrap()
            .insert(session_id, cwd);
    }

    pub(super) fn session_attach_lock(&self, session_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.session_attach_locks.lock().unwrap();
        Arc::clone(
            locks
                .entry(session_id.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    }

    pub(super) fn session_cwd(&self, session_id: &str) -> Option<String> {
        self.session_contexts
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
    }

    pub(super) fn release_session_context_if_idle(&self, session_id: &str) {
        let active = self
            .active_agent_runs
            .lock()
            .unwrap()
            .keys()
            .any(|(active_session_id, _)| active_session_id == session_id);
        if !active {
            self.session_contexts.lock().unwrap().remove(session_id);
            self.agent_hubs
                .lock()
                .unwrap()
                .retain(|(hub_session_id, _), _| hub_session_id != session_id);
        }
    }

    pub(super) fn get_approval_store(&self, cwd: &str) -> Arc<ApprovalStore> {
        let mut stores = self.approval_stores.lock().unwrap();
        stores
            .entry(cwd.to_string())
            .or_insert_with(|| Arc::new(ApprovalStore::new(cwd)))
            .clone()
    }

    async fn request_user_interaction(
        &self,
        request: UserInteractionRequest,
    ) -> UserInteractionResponse {
        let _prompt_turn = self.prompt_gate.lock().await;
        if !self
            .observation_router
            .has_route(&request.session_id, &request.agent_instance_id)
        {
            return UserInteractionResponse::Cancel {
                reason: Some("No TUI event channel available".into()),
            };
        }
        let interaction_id = format!(
            "interaction_{}_{}",
            request.tool_call_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let (tx, rx) = piko_comms::reply::<piko_comms::contracts::InteractionReply, _>();
        let session_id = request.session_id.clone();
        let snapshot = crate::api::UserInteractionSnapshot {
            interaction_id: interaction_id.clone(),
            agent_instance_id: request.agent_instance_id.clone(),
            agent_id: request.agent_id.clone(),
            tool_call_id: request.tool_call_id.clone(),
            status: crate::api::UserInteractionStatus::Pending,
            title: request.title.clone(),
            questions: request.questions.clone(),
            require_confirm: request.require_confirm,
            auto_resolution_ms: request.auto_resolution_ms,
        };
        {
            let mut pending = self.pending_interactions.lock().unwrap();
            pending.insert(
                interaction_id.clone(),
                PendingInteractionEntry {
                    session_id: Some(session_id.clone()),
                    snapshot: snapshot.clone(),
                    tx,
                },
            );
        }
        self.observation_router
            .publish(
                &session_id,
                &request.agent_instance_id,
                &request.agent_id,
                piko_protocol::agent_runtime::SessionEvent::InteractionRequested {
                    interaction: snapshot,
                },
            )
            .await;
        let response = match rx.await {
            Ok(response) => response,
            Err(_) => UserInteractionResponse::Cancel {
                reason: Some("Interaction channel closed".into()),
            },
        };
        {
            let mut pending = self.pending_interactions.lock().unwrap();
            pending.remove(&interaction_id);
        }
        response
    }

    pub(super) async fn register_user_interaction_tools_on_execution(
        &self,
        gateway_runner: &OrchAgentRunRunner,
    ) {
        let user_provider = UserInteractionProvider::new();
        let runner = gateway_runner.clone();
        user_provider
            .set_callbacks(UserInteractionCallbacks {
                request_user_input: Some(Arc::new(move |request| {
                    let runner = runner.clone();
                    Box::pin(async move { runner.request_user_interaction(request).await })
                })),
                request_approval: None,
            })
            .await;
        self.agent_runtime
            .register_tool_provider(Box::new(user_provider))
            .await;
        self.agent_runtime
            .register_tool_set(ToolSet {
                id: "user_interaction".into(),
                name: "User Interaction Tools".into(),
                description: Some("Tools that ask the user for input through hostd/TUI".into()),
                metadata: None,
                policy: None,
                tools: vec![ToolSetToolRef::ProviderNamespace {
                    provider_id: "user_interaction".into(),
                    namespace: "".into(),
                    alias: None,
                    policy: None,
                }],
            })
            .await;
    }
}
