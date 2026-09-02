use std::sync::Arc;

use crate::api::{ProtocolError, ServerMessage};
use crate::application::agent_work::projection::reconcile_committed_messages;
use crate::application::agent_work::projection::{
    record_committed_message, stream_items_from_delta,
};
use crate::application::host_app::HostApp;
use crate::ports::{AgentRunRunner, OperationRunCompletion};
use crate::util::{ClientEventSender, send_event};

impl HostApp {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn drive_operation_observation<C>(
        &self,
        runner: &Arc<dyn AgentRunRunner>,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
        session_dir: &std::path::Path,
        observation: piko_orchd_api::SessionSubscription,
        mut completion_future: std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<C, ProtocolError>> + Send>,
        >,
        tx: &ClientEventSender,
    ) -> Result<C, ProtocolError>
    where
        C: OperationRunCompletion + 'static,
    {
        use tokio_stream::StreamExt;

        let mut output = observation.output;
        let mut observed_cursor = observation.cursor;
        let mut completion = None;

        loop {
            if completion.as_ref().is_some_and(|completed: &C| {
                cursor_reached(&observed_cursor, completed.observation_barrier())
            }) {
                break;
            }
            tokio::select! {
                biased;
                result = &mut completion_future, if completion.is_none() => {
                    let completed = result?;
                    if completed.input_id() != input_id {
                        return Err(ProtocolError::ObservationFailed(
                            "operation completion identity mismatch".into(),
                        ));
                    }
                    completion = Some(completed);
                }
                item = output.next() => {
                    let Some(item) = item else {
                        let recovered = self.recover_operation_observation(
                            runner,
                            session_id,
                            agent_instance_id,
                            input_id,
                            session_dir,
                            piko_protocol::ReconcileReason::Reconnect,
                            tx,
                        ).await?;
                        observed_cursor = recovered.cursor.clone();
                        output = recovered.output;
                        continue;
                    };
                    let envelope = match item {
                        Ok(envelope) => envelope,
                        Err(piko_orchd_api::SessionStreamError::SnapshotRequired { .. }) => {
                            let recovered = self.recover_operation_observation(
                                runner,
                                session_id,
                                agent_instance_id,
                                input_id,
                                session_dir,
                                piko_protocol::ReconcileReason::RetentionExhausted,
                                tx,
                            ).await?;
                            observed_cursor = recovered.cursor.clone();
                            output = recovered.output;
                            continue;
                        }
                        Err(error) => {
                            return Err(ProtocolError::ObservationFailed(format!(
                                "session {}: {error}", session_id
                            )));
                        }
                    };
                    if let Some(cursor) = self
                        .project_operation_output(session_id, session_dir, envelope, tx)
                        .await?
                    {
                        observed_cursor = cursor;
                    }
                }
            }
        }

        Ok(completion.expect("completion checked before observation driver exits"))
    }

    pub(crate) async fn project_operation_output(
        &self,
        session_id: &str,
        session_dir: &std::path::Path,
        envelope: piko_protocol::agent_runtime::SessionOutputEnvelope,
        tx: &ClientEventSender,
    ) -> Result<Option<piko_protocol::agent_runtime::SessionCursor>, ProtocolError> {
        match envelope.output {
            piko_protocol::agent_runtime::SessionOutput::Delta(delta) => {
                for stream in stream_items_from_delta(session_id, &delta) {
                    send_event(tx, stream).await;
                }
                Ok(None)
            }
            piko_protocol::agent_runtime::SessionOutput::Event(event) => {
                let cursor = event.cursor.clone();
                match event.event {
                    piko_protocol::agent_runtime::SessionEvent::MessageCommitted {
                        message_id,
                        ..
                    }
                    | piko_protocol::agent_runtime::SessionEvent::ToolCommitted {
                        message_id,
                        ..
                    } => {
                        self.emit_committed_message(
                            session_id,
                            session_dir,
                            &event.agent_instance_id,
                            &message_id,
                            tx,
                        )
                        .await?;
                    }
                    piko_protocol::agent_runtime::SessionEvent::ModelStepCommitted { boundary } => {
                        if boundary.session_id != session_id
                            || boundary.agent_instance_id != event.agent_instance_id
                        {
                            return Err(ProtocolError::ObservationFailed(
                                "model-step boundary identity mismatch".into(),
                            ));
                        }
                        let mut message_ids = vec![boundary.assistant_message_id.clone()];
                        message_ids.extend(boundary.tool_call_message_ids.iter().cloned());
                        for message_id in message_ids {
                            self.emit_committed_message(
                                session_id,
                                session_dir,
                                &event.agent_instance_id,
                                &message_id,
                                tx,
                            )
                            .await?;
                        }
                        send_event(tx, ServerMessage::ModelStepCommitted(boundary)).await;
                    }
                    piko_protocol::agent_runtime::SessionEvent::AgentChanged { agent } => {
                        self.state
                            .lock()
                            .await
                            .upsert_live_agent(session_id, agent.clone())?;
                        send_event(tx, ServerMessage::AgentChanged(agent)).await;
                    }
                    piko_protocol::agent_runtime::SessionEvent::ApprovalRequested { approval } => {
                        send_event(
                            tx,
                            ServerMessage::Approval(crate::api::ApprovalEvent::Requested {
                                session_id: session_id.to_string(),
                                agent_instance_id: approval.agent_instance_id,
                                agent_id: event.agent_id,
                                approval_id: approval.approval_id,
                                tool_name: approval.tool_name,
                                tool_args: approval.request,
                                prompt: approval.prompt,
                            }),
                        )
                        .await;
                        self.publish_work_snapshot(session_id, tx).await?;
                    }
                    piko_protocol::agent_runtime::SessionEvent::ApprovalResolved {
                        approval_id,
                        status,
                    } => {
                        let decision = match status {
                            crate::api::ApprovalStatus::Approved => {
                                crate::api::ApprovalDecision::Accept
                            }
                            crate::api::ApprovalStatus::Pending
                            | crate::api::ApprovalStatus::Rejected
                            | crate::api::ApprovalStatus::Expired => {
                                crate::api::ApprovalDecision::Decline
                            }
                        };
                        send_event(
                            tx,
                            ServerMessage::Approval(crate::api::ApprovalEvent::Resolved {
                                session_id: session_id.to_string(),
                                approval_id,
                                decision,
                            }),
                        )
                        .await;
                        self.publish_work_snapshot(session_id, tx).await?;
                    }
                    piko_protocol::agent_runtime::SessionEvent::InteractionRequested {
                        interaction,
                    } => {
                        send_event(
                            tx,
                            ServerMessage::Interaction(crate::api::InteractionEvent::Requested {
                                session_id: session_id.to_string(),
                                agent_instance_id: interaction.agent_instance_id,
                                agent_id: interaction.agent_id,
                                interaction_id: interaction.interaction_id,
                                tool_call_id: interaction.tool_call_id,
                                title: interaction.title,
                                questions: interaction.questions,
                                require_confirm: interaction.require_confirm,
                                auto_resolution_ms: interaction.auto_resolution_ms,
                            }),
                        )
                        .await;
                        self.publish_work_snapshot(session_id, tx).await?;
                    }
                    piko_protocol::agent_runtime::SessionEvent::InteractionResolved {
                        interaction_id,
                        status,
                    } => {
                        send_event(
                            tx,
                            ServerMessage::Interaction(crate::api::InteractionEvent::Resolved {
                                session_id: session_id.to_string(),
                                interaction_id,
                                status,
                            }),
                        )
                        .await;
                        self.publish_work_snapshot(session_id, tx).await?;
                    }
                }
                Ok(Some(cursor))
            }
        }
    }

    async fn emit_committed_message(
        &self,
        session_id: &str,
        session_dir: &std::path::Path,
        agent_instance_id: &str,
        message_id: &str,
        tx: &ClientEventSender,
    ) -> Result<(), ProtocolError> {
        let (committed, tree_entry, agent_work_diff) = {
            let mut state = self.state.lock().await;
            let store = self.session_store_factory.open(session_dir);
            let committed = record_committed_message(
                &mut state,
                Some(store.as_ref()),
                session_id,
                agent_instance_id,
                message_id,
            )
            .await?;
            let tree_entry = committed.as_ref().and_then(|committed| {
                state.session(session_id).ok().and_then(|session| {
                    session
                        .entries
                        .iter()
                        .find(|entry| entry.id() == committed.message_id)
                        .cloned()
                })
            });
            let agent_work_diff = committed.as_ref().and_then(|committed| {
                crate::domain::sessions::file_change_from_message(&committed.message)?;
                Some(
                    state
                        .agent_work_diff(session_id, &committed.root_input_id)
                        .unwrap_or(piko_protocol::AgentWorkDiffEvent {
                            session_id: session_id.to_string(),
                            root_input_id: committed.root_input_id.clone(),
                            files: Vec::new(),
                            unified_diff: String::new(),
                        }),
                )
            });
            (committed, tree_entry, agent_work_diff)
        };
        let committed = committed.ok_or_else(|| {
            ProtocolError::ObservationFailed(format!(
                "committed projection {message_id} missing for agent {agent_instance_id}"
            ))
        })?;
        // F-27: message commit atomically persisted the todo replacement;
        // publish the matching live projection.
        let todo_list = {
            let mut state = self.state.lock().await;
            state
                .session_mut(session_id)
                .ok()
                .and_then(|s| s.take_pending_todo_projection())
        };
        send_event(tx, ServerMessage::TranscriptCommitted(committed)).await;
        let tree_entry = tree_entry.ok_or_else(|| {
            ProtocolError::ObservationFailed(format!(
                "committed tree entry {message_id} missing for agent {agent_instance_id}"
            ))
        })?;
        send_event(
            tx,
            ServerMessage::SessionEntryCommitted(piko_protocol::SessionEntryCommittedEvent {
                session_id: session_id.to_string(),
                entry: tree_entry,
            }),
        )
        .await;
        if let Some(list) = todo_list {
            send_event(
                tx,
                ServerMessage::TodoListUpdated(crate::domain::todos::todo_list_updated_event(list)),
            )
            .await;
        }
        if let Some(agent_work_diff) = agent_work_diff {
            send_event(tx, ServerMessage::AgentWorkDiff(agent_work_diff)).await;
        }
        Ok(())
    }

    /// Recover an operation observation and rebuild host projection from the
    /// durable journal. Root turns and direct Agent inputs share this path.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn recover_operation_observation(
        &self,
        runner: &Arc<dyn AgentRunRunner>,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
        session_dir: &std::path::Path,
        reason: piko_protocol::ReconcileReason,
        tx: &ClientEventSender,
    ) -> Result<piko_orchd_api::SessionSubscription, ProtocolError> {
        let (runtime_snapshot, recovered) = runner
            .recover_observation(session_id, agent_instance_id, input_id)
            .await?;
        let (snapshot, agents) = {
            let mut state = self.state.lock().await;
            let store = self.session_store_factory.open(session_dir);
            reconcile_committed_messages(&mut state, store.as_ref(), session_id).await?;
            (
                state.snapshot(session_id)?,
                state.get_agent_list(session_id),
            )
        };
        let (snapshot, agents) = self.enrich_session_view(session_id, snapshot, agents).await;
        send_event(
            tx,
            ServerMessage::SessionReconciled(piko_protocol::SessionReconciledEvent {
                session_id: session_id.to_string(),
                reason,
                cursor: runtime_snapshot.cursor,
                snapshot,
                agents,
            }),
        )
        .await;
        Ok(recovered)
    }
}

fn cursor_reached(
    observed: &piko_protocol::agent_runtime::SessionCursor,
    barrier: &piko_protocol::agent_runtime::SessionCursor,
) -> bool {
    observed.epoch == barrier.epoch && observed.seq >= barrier.seq
}
