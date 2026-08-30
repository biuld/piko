use crate::api::{ProtocolError, ServerMessage};
use crate::application::HostApp;
use crate::util::{ClientEventSender, now_ms, storage_error};

/// The application boundary for user-visible Agent work control.
pub(crate) struct AgentWorkControl<'a> {
    app: &'a HostApp,
}

impl<'a> AgentWorkControl<'a> {
    pub(crate) fn new(app: &'a HostApp) -> Self {
        Self { app }
    }

    pub(crate) async fn submit(
        &self,
        command_id: String,
        input: piko_protocol::AgentInput,
        tx: &ClientEventSender,
    ) -> Result<(), ProtocolError> {
        self.submit_with_response(command_id, input, tx).await
    }

    pub(crate) async fn cancel_input(
        &self,
        command_id: String,
        session_id: String,
        agent_instance_id: String,
        input_id: String,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        self.app.state.lock().await.session(&session_id)?;
        let runner = self.app.turn_runner.lock().await.clone();
        let receipt = runner
            .cancel_agent_input(&session_id, &agent_instance_id, &input_id)
            .await?;
        let mut messages = vec![ServerMessage::CommandResponse {
            command_id,
            result: Ok(crate::api::CommandResult::AgentInputCancelled {
                receipt,
                timestamp: now_ms(),
            }),
        }];
        let (snapshot, agents) = self.app.session_view(&session_id).await?;
        messages.push(super::sessions::helpers::session_reconciled_message(
            session_id,
            piko_protocol::ReconcileReason::ExplicitRefresh,
            snapshot,
            agents,
        ));
        Ok(messages)
    }

    pub(crate) async fn interrupt_current(
        &self,
        command_id: String,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        self.app.state.lock().await.session(&session_id)?;
        let accepted = self
            .app
            .turn_runner
            .lock()
            .await
            .clone()
            .interrupt_agent(&session_id, &agent_instance_id)
            .await;
        Ok(vec![ServerMessage::CommandResponse {
            command_id,
            result: Ok(crate::api::CommandResult::AgentInterrupted {
                session_id,
                agent_instance_id,
                accepted,
                timestamp: now_ms(),
            }),
        }])
    }

    async fn submit_with_response(
        &self,
        command_id: String,
        input: piko_protocol::AgentInput,
        tx: &ClientEventSender,
    ) -> Result<(), ProtocolError> {
        self.validate_user_input(&input).await?;
        match input.delivery {
            piko_protocol::AgentInputDelivery::SteerActive => {
                self.require_running(&input.session_id, &input.agent_instance_id)
                    .await?;
                let session_id = input.session_id.clone();
                let agent_instance_id = input.agent_instance_id.clone();
                let receipt = self
                    .app
                    .turn_runner
                    .lock()
                    .await
                    .clone()
                    .submit_agent_input(input, piko_orchd_api::AgentInputRuntime::default())
                    .await
                    .map_err(|_| {
                        ProtocolError::InvalidCommand(format!(
                            "steer rejected for agent {agent_instance_id}"
                        ))
                    })?;
                crate::util::send_event(
                    tx,
                    ServerMessage::CommandResponse {
                        command_id,
                        result: Ok(crate::api::CommandResult::AgentInputSubmitted {
                            receipt,
                            timestamp: now_ms(),
                        }),
                    },
                )
                .await;
                self.publish_work_snapshot(&session_id, tx).await?;
                Ok(())
            }
            piko_protocol::AgentInputDelivery::Auto
            | piko_protocol::AgentInputDelivery::StartWhenIdle
            | piko_protocol::AgentInputDelivery::FollowUp => {
                self.app
                    .submit_input_with_id(
                        command_id,
                        input.session_id,
                        input.agent_instance_id,
                        input.input_id,
                        input.content,
                        tx,
                    )
                    .await
            }
        }
    }

    async fn require_running(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<(), ProtocolError> {
        self.app.state.lock().await.session(session_id)?;
        if self.agent_is_steerable(session_id, agent_instance_id).await {
            return Ok(());
        }
        Err(ProtocolError::InvalidCommand(format!(
            "agent {agent_instance_id} is not running; cannot steer"
        )))
    }

    async fn agent_is_steerable(&self, session_id: &str, agent_instance_id: &str) -> bool {
        let runner = self.app.turn_runner.lock().await.clone();
        if let Some(agents) = runner.list_agent_instances(session_id).await
            && agents.iter().any(|agent| {
                agent.agent_instance_id == agent_instance_id
                    && agent_activity_is_live(&agent.activity)
            })
        {
            return true;
        }
        let (approvals, interactions) = runner.pending_prompts_for_session(session_id).await;
        if approvals
            .iter()
            .any(|approval| approval.agent_instance_id == agent_instance_id)
            || interactions
                .iter()
                .any(|interaction| interaction.agent_instance_id == agent_instance_id)
        {
            return true;
        }
        let Some(session_dir) = self.app.session_paths.lock().await.get(session_id).cloned() else {
            return false;
        };
        let Ok(projection) = self
            .app
            .session_store_factory
            .open(&session_dir)
            .load_projection()
            .await
        else {
            return false;
        };
        projection
            .agent_work
            .get(agent_instance_id)
            .is_some_and(work_is_steerable)
    }

    async fn validate_user_input(
        &self,
        input: &piko_protocol::AgentInput,
    ) -> Result<(), ProtocolError> {
        if input.input_id.is_empty() || input.request_id.is_empty() {
            return Err(ProtocolError::InvalidCommand(
                "Agent input identity must not be empty".into(),
            ));
        }
        if input.origin != piko_protocol::AgentInputOrigin::User
            || input.caller_agent_instance_id.is_some()
            || input.detached_recipient_agent_instance_id.is_some()
        {
            return Err(ProtocolError::InvalidCommand(
                "client AgentInput must have user origin and no caller".into(),
            ));
        }
        super::turns::content::validate_user_content(&input.content)?;
        let cwd = self.app.state.lock().await.session_cwd(&input.session_id)?;
        let session_dir = self
            .app
            .ensure_turn_session_dir(&input.session_id, &cwd)
            .await?;
        let projection = self
            .app
            .session_store_factory
            .open(&session_dir)
            .load_projection()
            .await
            .map_err(storage_error)?;
        let target = projection
            .agents
            .get(&input.agent_instance_id)
            .ok_or_else(|| {
                ProtocolError::InvalidCommand(format!(
                    "agent instance not found: {}",
                    input.agent_instance_id
                ))
            })?;
        if target.lifecycle != piko_protocol::AgentInstanceLifecycle::Open {
            return Err(ProtocolError::InvalidCommand(format!(
                "agent instance is not open: {}",
                input.agent_instance_id
            )));
        }
        Ok(())
    }

    async fn publish_work_snapshot(
        &self,
        session_id: &str,
        tx: &ClientEventSender,
    ) -> Result<(), ProtocolError> {
        let (snapshot, agents) = self.app.session_view(session_id).await?;
        crate::util::send_event(
            tx,
            super::sessions::helpers::session_reconciled_message(
                session_id.to_string(),
                piko_protocol::ReconcileReason::ExplicitRefresh,
                snapshot,
                agents,
            ),
        )
        .await;
        Ok(())
    }
}

fn agent_activity_is_live(activity: &piko_protocol::AgentActivity) -> bool {
    matches!(
        activity,
        piko_protocol::AgentActivity::Running
            | piko_protocol::AgentActivity::WaitingForApproval
            | piko_protocol::AgentActivity::Cancelling
    )
}

fn work_is_steerable(work: &piko_protocol::AgentWorkSnapshot) -> bool {
    work.active_work.as_ref().is_some_and(|active| {
        matches!(
            active.state,
            piko_protocol::AgentWorkViewState::Starting
                | piko_protocol::AgentWorkViewState::Running
                | piko_protocol::AgentWorkViewState::RequiresAction
                | piko_protocol::AgentWorkViewState::Cancelling
        )
    }) || matches!(
        work.foreground,
        piko_protocol::AgentForeground::Running
            | piko_protocol::AgentForeground::RequiresAction
            | piko_protocol::AgentForeground::Cancelling
    )
}
