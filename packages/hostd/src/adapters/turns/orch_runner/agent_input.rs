use std::sync::Arc;

use async_trait::async_trait;
use piko_orchd_api::{AgentInputRuntime, AgentRuntimeApi};
use piko_protocol::{AgentInput, AgentInputDelivery, SendAgentInputRequest};

use crate::api::ProtocolError;
use crate::ports::{
    AgentOperationAddress, AgentRunCompletion, AgentRunFailure, AgentRunHandle, AgentRunInput,
    AgentRunProcess,
};
use crate::util::now_ms;

use super::OrchAgentRunRunner;
use super::run::root_agent_spec;

struct OrchAgentRunProcess {
    runtime: Arc<piko_orchd::AgentRuntime>,
    hub: Arc<piko_orchd::events::SessionOutputHub>,
    acceptance_cursor: piko_protocol::agent_runtime::SessionCursor,
    disposition: piko_protocol::AgentInputDisposition,
    address: AgentOperationAddress,
    input_id: String,
}

#[async_trait]
impl AgentRunProcess for OrchAgentRunProcess {
    async fn wait_started(&mut self) -> Result<piko_orchd_api::SessionSubscription, ProtocolError> {
        self.runtime
            .wait_agent_input_started(
                self.address.session_id.clone(),
                self.address.agent_instance_id.clone(),
                self.input_id.clone(),
            )
            .await
            .map_err(|error| ProtocolError::ObservationFailed(error.to_string()))?;
        let cursor = if self.disposition == piko_protocol::AgentInputDisposition::PendingFollowUp {
            self.hub.cursor()
        } else {
            self.acceptance_cursor.clone()
        };
        let subscription = self
            .hub
            .subscribe(&cursor)
            .await
            .map_err(|error| ProtocolError::ObservationFailed(error.to_string()))?;
        Ok(piko_orchd_api::SessionSubscription {
            session_id: self.address.session_id.clone(),
            cursor: cursor.clone(),
            output: piko_orchd::events::merged_output_stream(subscription, cursor),
        })
    }

    async fn wait_completion(self: Box<Self>) -> Result<AgentRunCompletion, ProtocolError> {
        let Self {
            runtime,
            hub,
            address,
            input_id,
            ..
        } = *self;
        let result = runtime
            .wait_agent_input_completion(
                address.session_id.clone(),
                address.agent_instance_id.clone(),
                input_id,
            )
            .await
            .map_err(|error| AgentRunFailure {
                message: error.to_string(),
            });
        Ok(AgentRunCompletion {
            address,
            result,
            observation_barrier: hub.cursor(),
        })
    }
}

impl OrchAgentRunRunner {
    pub(super) async fn run_agent_subscription(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, ProtocolError> {
        let root_spec = root_agent_spec(&input.cwd);
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
        let key = (input.session_id.clone(), input.operation_id.clone());
        {
            let mut active = self.active_agent_runs.lock().unwrap();
            if active.contains_key(&key) {
                return Err(ProtocolError::InvalidCommand(format!(
                    "Agent operation already exists: {}",
                    input.operation_id
                )));
            }
            active.insert(
                key.clone(),
                super::ActiveAgentRunRuntime {
                    run_id: input.operation_id.clone(),
                    observation: Arc::clone(&hub),
                },
            );
        }
        self.observation_router.register(
            &input.session_id,
            &input.operation_id,
            &input.agent_instance_id,
            input.agent_instance_id == format!("agent_{}_root", input.session_id),
            Arc::clone(&hub),
        );
        match self
            .prepare_session_runtime(
                &input.session_id,
                &input.cwd,
                &input.session_dir,
                &root_spec,
                input.resume_agent.as_ref(),
            )
            .await
        {
            Ok(()) => {}
            Err(error) => {
                self.abort_agent_input(
                    &input.session_id,
                    &input.agent_instance_id,
                    &input.operation_id,
                );
                return Err(error);
            }
        }
        if let Err(error) = self
            .agent_runtime
            .agent_snapshot(input.session_id.clone(), input.agent_instance_id.clone())
            .await
        {
            self.abort_agent_input(
                &input.session_id,
                &input.agent_instance_id,
                &input.operation_id,
            );
            return Err(ProtocolError::InvalidCommand(error.to_string()));
        }

        let acceptance_cursor = hub.cursor();
        let request = SendAgentInputRequest {
            request_id: input.operation_id.clone(),
            session_id: input.session_id.clone(),
            agent_instance_id: input.agent_instance_id.clone(),
            caller_agent_instance_id: None,
            source_turn_id: input.source_turn_id.clone(),
            message_id: format!("msg_user_{}", uuid::Uuid::new_v4()),
            content: input.content,
            delivery: AgentInputDelivery::FollowUp,
            prompt_resources: input.prompt_resources,
            active_tool_names: input.active_tool_names,
        };
        let canonical = AgentInput::from_request(&request, now_ms());
        let receipt = match self
            .agent_runtime
            .submit_runtime_agent_input(
                canonical,
                AgentInputRuntime {
                    prompt_resources: request.prompt_resources,
                    active_tool_names: request.active_tool_names,
                    source_turn_id: request.source_turn_id,
                    message_id: Some(request.message_id),
                },
            )
            .await
        {
            Ok(receipt) => receipt,
            Err(error) => {
                self.abort_agent_input(
                    &input.session_id,
                    &input.agent_instance_id,
                    &input.operation_id,
                );
                return Err(ProtocolError::InvalidCommand(error.to_string()));
            }
        };
        let disposition = receipt.disposition;
        let address = AgentOperationAddress {
            session_id: input.session_id.clone(),
            operation_id: input.operation_id.clone(),
            agent_instance_id: input.agent_instance_id.clone(),
        };

        Ok(AgentRunHandle {
            address: address.clone(),
            receipt,
            process: Box::new(OrchAgentRunProcess {
                runtime: Arc::clone(&self.agent_runtime),
                hub,
                acceptance_cursor,
                disposition,
                address,
                input_id: input.operation_id,
            }),
        })
    }

    pub(super) fn finish_agent_input(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        run_id: &str,
    ) {
        let removed = {
            let mut active = self.active_agent_runs.lock().unwrap();
            let key = (session_id.to_string(), run_id.to_string());
            if active.get(&key).is_some_and(|run| run.run_id == run_id) {
                active.remove(&key)
            } else {
                None
            }
        };
        if removed.is_none() {
            return;
        }
        self.release_session_context_if_idle(session_id);
    }

    fn abort_agent_input(&self, session_id: &str, agent_instance_id: &str, run_id: &str) {
        self.finish_agent_input(session_id, agent_instance_id, run_id);
        self.observation_router.unregister(session_id, run_id);
    }
}
