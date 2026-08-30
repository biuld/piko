//! Agent snapshot and mailbox query helpers.

use super::*;

impl AgentRuntime {
    pub(super) async fn wait_agent_input_started_impl(
        &self,
        session_id: String,
        agent_instance_id: String,
        input_id: String,
    ) -> Result<(), AgentApiError> {
        let scope = self.scope(&session_id).await?;
        let handle = scope
            .agent(&agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let mut snapshots = handle.snapshot_rx.clone();
        let mut saw_pending = false;
        loop {
            {
                let snapshot = snapshots.borrow_and_update();
                if snapshot.active_root_input_id.as_deref() == Some(input_id.as_str())
                    || snapshot
                        .latest_report
                        .as_ref()
                        .is_some_and(|report| report.root_input_id == input_id)
                {
                    return Ok(());
                }
                if snapshot
                    .pending_follow_up_ids
                    .iter()
                    .any(|pending| pending == &input_id)
                {
                    saw_pending = true;
                } else if saw_pending {
                    return Err(AgentApiError::Cancelled);
                } else if matches!(
                    snapshot.lifecycle,
                    piko_protocol::AgentInstanceLifecycle::Terminated
                        | piko_protocol::AgentInstanceLifecycle::Unavailable
                ) {
                    return Err(AgentApiError::InvalidState);
                }
            }
            snapshots
                .changed()
                .await
                .map_err(|_| AgentApiError::RuntimeUnavailable)?;
        }
    }

    pub(super) async fn wait_agent_input_completion_impl(
        &self,
        session_id: String,
        agent_instance_id: String,
        input_id: String,
    ) -> Result<piko_protocol::AgentWorkReport, AgentApiError> {
        let scope = self.scope(&session_id).await?;
        let handle = scope
            .agent(&agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let mut snapshots = handle.snapshot_rx.clone();
        let mut saw_pending = false;
        loop {
            {
                let snapshot = snapshots.borrow_and_update();
                if let Some(report) = snapshot
                    .latest_report
                    .as_ref()
                    .filter(|report| report.root_input_id == input_id)
                {
                    return Ok(report.clone());
                }
                if snapshot.active_root_input_id.as_deref() == Some(input_id.as_str()) {
                    saw_pending = false;
                } else if snapshot
                    .pending_follow_up_ids
                    .iter()
                    .any(|pending| pending == &input_id)
                {
                    saw_pending = true;
                } else if saw_pending {
                    return Err(AgentApiError::Cancelled);
                } else if matches!(
                    snapshot.lifecycle,
                    piko_protocol::AgentInstanceLifecycle::Terminated
                        | piko_protocol::AgentInstanceLifecycle::Unavailable
                ) {
                    return Err(AgentApiError::InvalidState);
                }
            }
            snapshots
                .changed()
                .await
                .map_err(|_| AgentApiError::RuntimeUnavailable)?;
        }
    }

    pub(super) async fn agent_snapshot_impl(
        &self,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<Option<AgentSnapshot>, AgentApiError> {
        let scope = self.scope(&session_id).await?;
        Ok(scope
            .agent(&agent_instance_id)
            .await
            .map(|handle| handle.snapshot_rx.borrow().clone()))
    }

    pub(super) async fn list_agents_impl(
        &self,
        session_id: String,
    ) -> Result<Vec<AgentSnapshot>, AgentApiError> {
        let scope = self.scope(&session_id).await?;
        let mut snapshots = scope.snapshots().await;
        let parents = snapshots
            .iter()
            .map(|snapshot| {
                (
                    snapshot.identity.agent_instance_id.clone(),
                    snapshot.identity.parent_agent_instance_id.clone(),
                )
            })
            .collect::<HashMap<_, _>>();
        snapshots.sort_by(|left, right| {
            agent_depth(&parents, &left.identity.agent_instance_id)
                .cmp(&agent_depth(&parents, &right.identity.agent_instance_id))
                .then_with(|| {
                    left.identity
                        .agent_instance_id
                        .cmp(&right.identity.agent_instance_id)
                })
        });
        Ok(snapshots)
    }

    pub(super) async fn list_agent_specs_impl(
        &self,
    ) -> Result<Vec<piko_protocol::AgentSpec>, AgentApiError> {
        Ok(self.execution.services().list_agent_specs().await)
    }

    pub(super) async fn agent_inbox_impl(
        &self,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<AgentInboxSnapshot, AgentApiError> {
        let scope = self.scope(&session_id).await?;
        let handle = scope
            .agent(&agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let (reply, received) = piko_comms::reply::<AgentCommandReply, _>();
        handle
            .command_tx
            .send(AgentCommand::Inbox { reply })
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?;
        received
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)
    }

    pub(super) async fn consume_agent_inbox_item_impl(
        &self,
        request: piko_protocol::ConsumeAgentInboxRequest,
    ) -> Result<piko_protocol::ConsumeAgentInboxReceipt, AgentApiError> {
        let scope = self.scope(&request.session_id).await?;
        let handle = scope
            .agent(&request.agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let (reply, received) = piko_comms::reply::<AgentCommandReply, _>();
        handle
            .command_tx
            .send(AgentCommand::ConsumeInbox { request, reply })
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?;
        received
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?
    }

    pub(super) async fn wait_agent_mailbox_impl(
        &self,
        request: MailboxWaitRequest,
    ) -> Result<MailboxWaitSummary, AgentApiError> {
        let scope = self.scope(&request.session_id).await?;
        if let Some(caller) = &request.caller_agent_instance_id {
            scope
                .agent(caller)
                .await
                .ok_or(AgentApiError::AgentNotFound)?;
        }
        let mut receiver = scope.mailbox_events().subscribe();
        let timeout = tokio::time::Duration::from_millis(request.timeout_ms);
        let event = tokio::time::timeout(timeout, async {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        if request
                            .agent_instance_id
                            .as_deref()
                            .is_some_and(|filter| event.agent_instance_id() != filter)
                        {
                            continue;
                        }
                        return Some(event);
                    }
                    // Lagged events are skipped: waiting continues on the next
                    // update rather than failing or replaying.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                }
            }
        })
        .await
        .unwrap_or(None);

        let timed_out = event.is_none();
        let agents = self.list_agents_impl(request.session_id).await?;
        Ok(MailboxWaitSummary {
            timed_out,
            event,
            agents,
        })
    }
}
