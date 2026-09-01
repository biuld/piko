use super::*;

#[derive(Debug, Clone)]
pub(crate) struct ExecutionTerminal {
    pub outcome: piko_protocol::agent_work::AgentWorkOutcome,
    pub transcript: Vec<piko_protocol::Message>,
    pub head_message_id: Option<String>,
}

pub(super) async fn supervise_execution(
    scope: Arc<SessionExecutionScope>,
    actor: ExecutionActor,
    generation: u64,
    terminal_tx: piko_comms::ReplySender<ExecutionTerminalContract, ExecutionTerminal>,
) -> ExecutionExit {
    let identity = actor.identity().clone();
    let result = std::panic::AssertUnwindSafe(actor.run())
        .catch_unwind()
        .await;
    let (outcome, transcript, head_message_id) = match result {
        Ok(result) => (result.outcome, result.transcript, result.head_message_id),
        Err(_) => (
            piko_protocol::AgentWorkOutcome::failed("ExecutionActor panicked"),
            Vec::new(),
            None,
        ),
    };
    match &outcome {
        piko_protocol::agent_work::AgentWorkOutcome::Succeeded { .. } => {
            tracing::info!(
                target: "agent.run_completed",
                session_id = %identity.session_id,
                root_input_id = %identity.root_input_id,
                agent_instance_id = %identity.agent_instance_id,
                "Agent run completed"
            );
        }
        piko_protocol::agent_work::AgentWorkOutcome::Cancelled { reason } => {
            tracing::Span::current().record("otel.status_code", "ERROR");
            tracing::info!(
                target: "agent.run_cancelled",
                session_id = %identity.session_id,
                root_input_id = %identity.root_input_id,
                agent_instance_id = %identity.agent_instance_id,
                reason = ?reason,
                "Agent run cancelled"
            );
        }
        piko_protocol::agent_work::AgentWorkOutcome::Failed { error } => {
            tracing::Span::current().record("otel.status_code", "ERROR");
            tracing::error!(
                target: "agent.run_failed",
                session_id = %identity.session_id,
                root_input_id = %identity.root_input_id,
                agent_instance_id = %identity.agent_instance_id,
                error = %truncate(error, 512),
                "Agent run failed"
            );
        }
    }
    let candidate = ExecutionTerminal {
        outcome: outcome.clone(),
        transcript,
        head_message_id,
    };
    let mut selector = TerminalSelector::new();
    let _ = selector.choose(candidate);
    let terminal = selector
        .into_selected()
        .expect("Execution supervisor must select one terminal candidate");
    scope
        .publish_terminal(&identity.root_input_id, terminal.clone())
        .await;
    let _ = terminal_tx.send(terminal.clone());
    scope
        .remove_if_generation(&identity.root_input_id, generation)
        .await;
    ExecutionExit {
        identity,
        terminal: outcome,
    }
}
