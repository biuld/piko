use std::sync::Arc;

use piko_orchd_api::{AgentInputRuntime, AgentRuntimeApi};
use piko_protocol::AgentInput;

use crate::api::ProtocolError;
use crate::ports::{AgentRunCompletion, ResumeAgent};

use super::OrchAgentRunRunner;
use super::run::root_agent_spec;

/// Stable key for one admitted input's live observation state. `input_id` is
/// the durable control identity (the root input id when applied as root).
type AgentInputKey = (String, String);

impl OrchAgentRunRunner {
    /// Register the root agent and attach the runtime session. Idempotent:
    /// repeated turn turnaround on the same session reuses the attached scope.
    pub(super) async fn ensure_session_runtime(
        &self,
        session_id: &str,
        cwd: &str,
        session_dir: &std::path::Path,
        resume_agent: Option<&ResumeAgent>,
    ) -> Result<(), ProtocolError> {
        self.agent_runtime
            .set_approval_gateway(Box::new(self.clone()))
            .await;
        self.agent_runtime
            .register_agent(root_agent_spec(cwd))
            .await;
        let root_spec = root_agent_spec(cwd);
        self.prepare_session_runtime(session_id, cwd, session_dir, &root_spec, resume_agent)
            .await
    }

    /// Admit one root input and register its live observation route.
    pub(super) async fn submit_runtime(
        &self,
        input: AgentInput,
        runtime: AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, ProtocolError> {
        let input_id = input.input_id.clone();
        let session_id = input.session_id.clone();
        let key: AgentInputKey = (session_id.clone(), input_id.clone());
        {
            let mut active = self.active_agent_inputs.lock().unwrap();
            if active.contains_key(&key) {
                return Err(ProtocolError::InvalidCommand(format!(
                    "Agent input already active: {input_id}"
                )));
            }
            let hub = self.register_input_route(&input, &input_id);
            let observation_cursor = hub.cursor();
            active.insert(
                key,
                super::ActiveAgentRunRuntime {
                    agent_instance_id: input.agent_instance_id.clone(),
                    observation: hub,
                    observation_cursor,
                    input_id: input_id.clone(),
                },
            );
        }
        let receipt = match self
            .agent_runtime
            .submit_runtime_agent_input(input, runtime)
            .await
        {
            Ok(receipt) => receipt,
            Err(error) => {
                self.finish_agent_input(&session_id, &input_id);
                return Err(ProtocolError::InvalidCommand(error.to_string()));
            }
        };
        Ok(receipt)
    }

    /// Register the live observation route for one admitted input and the hub
    /// that carries its realtime deltas. Uses the AgentInstance-scoped hub so
    /// concurrent Agents never replace each other's sink.
    pub(super) fn register_input_route(
        &self,
        input: &AgentInput,
        input_id: &str,
    ) -> Arc<piko_orchd::events::SessionOutputHub> {
        let hub = {
            let mut hubs = self.agent_hubs.lock().unwrap();
            Arc::clone(
                hubs.entry((input.session_id.clone(), input.agent_instance_id.clone()))
                    .or_insert_with(|| {
                        Arc::new(piko_orchd::events::SessionOutputHub::new(
                            input.session_id.clone(),
                            uuid::Uuid::new_v4().to_string(),
                            64,
                        ))
                    }),
            )
        };
        self.observation_router.register(
            &input.session_id,
            input_id,
            &input.agent_instance_id,
            input.agent_instance_id == format!("agent_{}_root", input.session_id),
            Arc::clone(&hub),
        );
        hub
    }

    /// Subscribe to the live observation stream for one admitted input. This
    /// resolves once `input_id` is the active root, has produced a report, or
    /// is no longer a pending follow-up.
    pub(super) async fn wait_started(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
        _disposition: piko_protocol::AgentInputDisposition,
    ) -> Result<piko_orchd_api::SessionSubscription, ProtocolError> {
        self.agent_runtime
            .wait_agent_input_started(
                session_id.to_string(),
                agent_instance_id.to_string(),
                input_id.to_string(),
            )
            .await
            .map_err(|error| ProtocolError::ObservationFailed(error.to_string()))?;
        let hub = self.hub_for_input(session_id, input_id).ok_or_else(|| {
            ProtocolError::ObservationFailed(format!(
                "no live observation hub for {session_id}/{agent_instance_id}/{input_id}"
            ))
        })?;
        let cursor = self
            .observation_cursor_for(session_id, input_id)
            .unwrap_or_else(|| hub.cursor());
        let subscription = hub
            .subscribe(&cursor)
            .await
            .map_err(|error| ProtocolError::ObservationFailed(error.to_string()))?;
        Ok(piko_orchd_api::SessionSubscription {
            session_id: session_id.to_string(),
            cursor: cursor.clone(),
            output: piko_orchd::events::merged_output_stream(subscription, cursor),
        })
    }

    /// Observe the durable terminal report for one root input.
    pub(super) async fn wait_completion(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
    ) -> Result<AgentRunCompletion, ProtocolError> {
        let result = self
            .agent_runtime
            .wait_agent_input_completion(
                session_id.to_string(),
                agent_instance_id.to_string(),
                input_id.to_string(),
            )
            .await
            .map_err(|error| crate::ports::AgentRunFailure {
                message: error.to_string(),
            });
        let barrier = self
            .hub_for_input(session_id, input_id)
            .map(|hub| hub.cursor())
            .unwrap_or_else(|| piko_protocol::agent_runtime::SessionCursor {
                epoch: String::new(),
                seq: 0,
            });
        Ok(AgentRunCompletion {
            input_id: input_id.to_string(),
            result,
            observation_barrier: barrier,
        })
    }

    pub(super) fn hub_for_input(
        &self,
        session_id: &str,
        input_id: &str,
    ) -> Option<Arc<piko_orchd::events::SessionOutputHub>> {
        self.active_agent_inputs
            .lock()
            .unwrap()
            .get(&(session_id.to_string(), input_id.to_string()))
            .map(|run| run.observation.clone())
    }

    /// Read the hub position captured immediately before processing starts,
    /// so a late subscriber still replays every reliable commit for the input.
    fn observation_cursor_for(
        &self,
        session_id: &str,
        input_id: &str,
    ) -> Option<piko_protocol::agent_runtime::SessionCursor> {
        self.active_agent_inputs
            .lock()
            .unwrap()
            .get(&(session_id.to_string(), input_id.to_string()))
            .map(|run| run.observation_cursor.clone())
    }

    pub(super) fn finish_agent_input(&self, session_id: &str, input_id: &str) {
        let removed = self
            .active_agent_inputs
            .lock()
            .unwrap()
            .remove(&(session_id.to_string(), input_id.to_string()));
        if removed.is_some() {
            self.observation_router.unregister(session_id, input_id);
            self.release_session_context_if_idle(session_id);
        }
    }
}
