use std::sync::Arc;

use tokio::sync::mpsc::UnboundedSender;

use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::application::turns::projection::reconcile_committed_messages;
use crate::application::turns::projection::{
    realtime_message_from_delta, record_committed_message,
};
use crate::ports::{AgentOperationAddress, OperationRunCompletion, TurnRunner};
use crate::util::send_event;

impl HostApp {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn drive_operation_observation<C>(
        &self,
        runner: &Arc<dyn TurnRunner>,
        address: &AgentOperationAddress,
        session_dir: &std::path::Path,
        observation: orchd_api::SessionSubscription,
        mut completion_rx: tokio::sync::oneshot::Receiver<C>,
        mut ui_event_rx: tokio::sync::mpsc::UnboundedReceiver<ServerMessage>,
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<C, ProtocolError>
    where
        C: OperationRunCompletion + 'static,
    {
        use tokio_stream::StreamExt;

        let mut output = observation.output;
        let mut observed_cursor = observation.cursor;
        let mut completion = None;
        let mut ui_events_open = true;

        loop {
            if completion.as_ref().is_some_and(|completed: &C| {
                cursor_reached(&observed_cursor, completed.observation_barrier())
            }) {
                break;
            }
            tokio::select! {
                biased;
                result = &mut completion_rx, if completion.is_none() => {
                    let completed = result.map_err(|_| ProtocolError::ObservationFailed(
                        format!("operation completion channel closed for {}/{}/{}",
                            address.session_id,
                            address.agent_instance_id,
                            address.operation_id,
                        )
                    ))?;
                    if completed.operation_address() != *address {
                        return Err(ProtocolError::ObservationFailed(
                            "operation completion identity mismatch".into(),
                        ));
                    }
                    completion = Some(completed);
                }
                ui_event = ui_event_rx.recv(), if ui_events_open => {
                    if let Some(event) = ui_event {
                        self.forward_operation_ui_event(&address.session_id, event, tx).await;
                    } else {
                        ui_events_open = false;
                    }
                }
                item = output.next() => {
                    let Some(item) = item else {
                        let recovered = self.recover_operation_observation(
                            runner,
                            &address.session_id,
                            &address.operation_id,
                            &address.agent_instance_id,
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
                        Err(orchd_api::SessionStreamError::SnapshotRequired { .. }) => {
                            let recovered = self.recover_operation_observation(
                                runner,
                                &address.session_id,
                                &address.operation_id,
                                &address.agent_instance_id,
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
                                "session {}: {error}", address.session_id
                            )));
                        }
                    };
                    if let Some(cursor) = self
                        .project_operation_output(&address.session_id, session_dir, envelope, tx)
                        .await?
                    {
                        observed_cursor = cursor;
                    }
                }
            }
        }

        self.drain_operation_ui_events(&address.session_id, &mut ui_event_rx, tx)
            .await;
        Ok(completion.expect("completion checked before observation driver exits"))
    }

    pub(crate) async fn project_operation_output(
        &self,
        session_id: &str,
        session_dir: &std::path::Path,
        envelope: piko_protocol::agent_runtime::SessionOutputEnvelope,
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<Option<piko_protocol::agent_runtime::SessionCursor>, ProtocolError> {
        match envelope.output {
            piko_protocol::agent_runtime::SessionOutput::Delta(delta) => {
                if let Some(event) = realtime_message_from_delta(session_id, &delta) {
                    send_event(tx, ServerMessage::RealtimeMessage(event));
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
                        let committed = {
                            let mut state = self.state.lock().await;
                            let store = self.session_store_factory.open(session_dir);
                            record_committed_message(
                                &mut state,
                                Some(store.as_ref()),
                                session_id,
                                &event.agent_instance_id,
                                &message_id,
                            )?
                        };
                        let committed = committed.ok_or_else(|| {
                            ProtocolError::ObservationFailed(format!(
                                "committed projection {message_id} missing for agent {}",
                                event.agent_instance_id
                            ))
                        })?;
                        send_event(tx, ServerMessage::TranscriptCommitted(committed));
                    }
                    _ => {}
                }
                Ok(Some(cursor))
            }
        }
    }

    pub(crate) async fn forward_operation_ui_event(
        &self,
        session_id: &str,
        event: ServerMessage,
        tx: &UnboundedSender<ServerMessage>,
    ) {
        if let ServerMessage::AgentChanged(info) = &event
            && let Err(error) = self
                .state
                .lock()
                .await
                .upsert_live_agent(session_id, info.clone())
        {
            tracing::warn!(
                session_id,
                agent_instance_id = %info.agent_instance_id,
                %error,
                "failed to mirror AgentChanged into host state"
            );
        }
        send_event(tx, event);
    }

    pub(crate) async fn drain_operation_ui_events(
        &self,
        session_id: &str,
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<ServerMessage>,
        tx: &UnboundedSender<ServerMessage>,
    ) {
        while let Ok(event) = rx.try_recv() {
            self.forward_operation_ui_event(session_id, event, tx).await;
        }
    }

    /// Recover an operation observation and rebuild host projection from the
    /// durable Agent shards. Root Turns and direct Agent runs share this path.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn recover_operation_observation(
        &self,
        runner: &Arc<dyn TurnRunner>,
        session_id: &str,
        operation_id: &str,
        agent_instance_id: &str,
        session_dir: &std::path::Path,
        reason: piko_protocol::ReconcileReason,
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<orchd_api::SessionSubscription, ProtocolError> {
        let operation = AgentOperationAddress {
            session_id: session_id.to_string(),
            operation_id: operation_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
        };
        let (runtime_snapshot, recovered) = runner.recover_observation(&operation).await?;
        let (snapshot, agents) = {
            let mut state = self.state.lock().await;
            let store = self.session_store_factory.open(session_dir);
            reconcile_committed_messages(&mut state, store.as_ref(), session_id)?;
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
        );
        Ok(recovered)
    }
}

fn cursor_reached(
    observed: &piko_protocol::agent_runtime::SessionCursor,
    barrier: &piko_protocol::agent_runtime::SessionCursor,
) -> bool {
    observed.epoch == barrier.epoch && observed.seq >= barrier.seq
}
