use std::sync::Arc;

use orchd_api::AgentRuntimeApi;
use piko_protocol::{AgentInputDelivery, MessageContent, SendAgentInputRequest};

use crate::api::ProtocolError;
use crate::ports::{
    AgentOperationAddress, AgentRunCompletion, AgentRunFailure, AgentRunHandle, AgentRunInput,
};

use super::OrchAgentRunRunner;
use super::run::root_agent_spec;

impl OrchAgentRunRunner {
    pub(super) async fn run_agent_subscription(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, ProtocolError> {
        let root_spec = root_agent_spec(&input.cwd);
        let hub = Arc::new(orchd::events::SessionOutputHub::new(
            input.session_id.clone(),
            uuid::Uuid::new_v4().to_string(),
            64,
        ));
        let key = (input.session_id.clone(), input.agent_instance_id.clone());
        {
            let mut active = self.active_agent_runs.lock().unwrap();
            if active.contains_key(&key) {
                return Err(ProtocolError::InvalidCommand(format!(
                    "agent already has an active input: {}",
                    input.agent_instance_id
                )));
            }
            active.insert(
                key.clone(),
                super::ActiveAgentRunRuntime {
                    run_id: input.operation_id.clone(),
                    agent_instance_id: input.agent_instance_id.clone(),
                    observation: Arc::clone(&hub),
                    commit_router: None,
                    realtime_router: None,
                },
            );
        }
        let prepared = match self
            .prepare_session_runtime(
                &input.session_id,
                &input.cwd,
                &input.session_dir,
                &input.operation_id,
                &input.agent_instance_id,
                input.agent_instance_id == format!("agent_{}_root", input.session_id),
                &root_spec,
                input.resume_agent.as_ref(),
                Arc::clone(&hub),
            )
            .await
        {
            Ok(prepared) => prepared,
            Err(error) => {
                self.abort_agent_input(
                    &input.session_id,
                    &input.agent_instance_id,
                    &input.operation_id,
                );
                return Err(error);
            }
        };
        if let Some(active) = self.active_agent_runs.lock().unwrap().get_mut(&key)
            && active.run_id == input.operation_id
        {
            active.commit_router = Some(Arc::clone(&prepared.commit_router));
            active.realtime_router = Some(Arc::clone(&prepared.realtime_router));
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

        let cursor = hub.cursor();
        let subscription = hub
            .subscribe(&piko_protocol::agent_runtime::SessionCursor {
                epoch: cursor.epoch.clone(),
                seq: 0,
            })
            .await
            .map_err(|error| ProtocolError::ObservationFailed(error.to_string()))?;
        let request = SendAgentInputRequest {
            request_id: format!("req_{}", uuid::Uuid::new_v4()),
            session_id: input.session_id.clone(),
            agent_instance_id: input.agent_instance_id.clone(),
            caller_agent_instance_id: None,
            source_turn_id: input.source_turn_id,
            message_id: format!("msg_user_{}", uuid::Uuid::new_v4()),
            content: MessageContent::String(input.prompt),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: input.prompt_resources,
            active_tool_names: input.active_tool_names,
        };
        let (completion_tx, completion) = tokio::sync::oneshot::channel();
        let runtime = Arc::clone(&self.agent_runtime);
        let session_id = input.session_id.clone();
        let operation_id = input.operation_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        let address = AgentOperationAddress {
            session_id: session_id.clone(),
            operation_id: operation_id.clone(),
            agent_instance_id: agent_instance_id.clone(),
        };
        let completion_hub = Arc::clone(&hub);
        tokio::spawn(async move {
            let result = runtime
                .run_agent(request)
                .await
                .map_err(|error| AgentRunFailure {
                    message: error.to_string(),
                });
            let _ = completion_tx.send(AgentRunCompletion {
                address,
                result,
                observation_barrier: completion_hub.cursor(),
            });
        });

        Ok(AgentRunHandle {
            address: AgentOperationAddress {
                session_id: input.session_id.clone(),
                operation_id: input.operation_id,
                agent_instance_id: input.agent_instance_id,
            },
            observation: orchd_api::SessionSubscription {
                session_id: input.session_id,
                cursor: cursor.clone(),
                output: orchd::events::merged_output_stream(subscription, cursor),
            },
            completion,
        })
    }

    pub(super) fn finish_agent_input(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        run_id: &str,
    ) {
        let removed = {
            let mut active = self.active_agent_runs.lock().unwrap();
            let key = (session_id.to_string(), agent_instance_id.to_string());
            if active.get(&key).is_some_and(|run| run.run_id == run_id) {
                active.remove(&key)
            } else {
                None
            }
        };
        let Some(run) = removed else {
            return;
        };
        if let Some(router) = run.commit_router {
            router.unregister(&run.agent_instance_id, run_id);
        }
        if let Some(router) = run.realtime_router {
            router.unregister(&run.agent_instance_id, run_id);
        }
        self.release_session_context_if_idle(session_id);
    }

    fn abort_agent_input(&self, session_id: &str, agent_instance_id: &str, run_id: &str) {
        self.finish_agent_input(session_id, agent_instance_id, run_id);
        if let Some(router) = self.commit_routers.lock().unwrap().get(session_id).cloned() {
            router.unregister(agent_instance_id, run_id);
        }
        if let Some(router) = self
            .realtime_routers
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
        {
            router.unregister(agent_instance_id, run_id);
        }
        self.ui_router.unregister(session_id, run_id);
    }
}
