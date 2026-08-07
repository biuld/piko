//! Turn observation projection helpers.
//!
//! Bridges realtime deltas and durably committed messages coming off the
//! orchd observation stream into `HostState` / TUI-facing events. Reads
//! durable storage only through [`SessionStorePort`] so this module has no
//! `crate::infra` / `crate::adapters` dependency.

use piko_protocol::agent_runtime::RealtimeDeltaEnvelope;
use piko_protocol::{Message, SessionTreeEntry, TranscriptCommittedEvent};

use crate::api::{MessageEntry, ProtocolError, ServerMessage};
use crate::domain::sessions::{HostState, file_change_from_message};
use crate::ports::session_store::SessionStorePort;
use crate::ports::storage_types::SessionStorageError;

/// Observation path: project a committed message for TUI emission.
///
/// The Execution runtime commits durably to the AgentInstance shard *before*
/// publishing `MessageCommitted`, so prefer in-memory HostState (already
/// projected by [`record_committed_message`]) and fall back to durable
/// storage for the first observation of a shard or during session recovery.
pub fn project_committed_message(
    state: &HostState,
    store: Option<&dyn SessionStorePort>,
    session_id: &str,
    agent_instance_id: &str,
    message_id: &str,
) -> Option<TranscriptCommittedEvent> {
    if let Some(projected) =
        project_committed_message_from_state(state, session_id, agent_instance_id, message_id)
    {
        return Some(projected);
    }
    store.and_then(|store| {
        project_committed_message_from_store(store, session_id, agent_instance_id, message_id)
    })
}

/// Project a durably committed message and record it into HostState so a
/// subsequent `StateSnapshot` reflects it without a disk reload.
pub fn record_committed_message(
    state: &mut HostState,
    store: Option<&dyn SessionStorePort>,
    session_id: &str,
    agent_instance_id: &str,
    message_id: &str,
) -> Result<Option<TranscriptCommittedEvent>, ProtocolError> {
    let Some(projected) =
        project_committed_message(state, store, session_id, agent_instance_id, message_id)
    else {
        return Ok(None);
    };
    append_committed_message(
        state,
        session_id,
        agent_instance_id,
        &projected.agent_id,
        &projected.source_turn_id,
        &projected.message,
        &projected.message_id,
        projected.transcript_seq,
        None,
    )
}

/// Rebuild the in-memory committed projection from every durable Agent shard.
/// This is used when reliable observation cannot replay the full cursor range.
pub fn reconcile_committed_messages(
    state: &mut HostState,
    store: &dyn SessionStorePort,
    session_id: &str,
) -> Result<(), ProtocolError> {
    let agents = match store.agent_instances() {
        Ok(agents) => agents,
        Err(SessionStorageError::NotFound(_)) => return Ok(()),
        Err(error) => return Err(ProtocolError::ObservationFailed(error.to_string())),
    };
    for agent in agents {
        let agent_instance_id = agent.identity.agent_instance_id;
        let recovered = match store.load_agent(session_id, &agent_instance_id) {
            Ok(recovered) => recovered,
            Err(SessionStorageError::NotFound(_)) => continue,
            Err(SessionStorageError::Io { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                continue;
            }
            Err(error) => return Err(ProtocolError::ObservationFailed(error.to_string())),
        };
        for message in recovered.transcript {
            let _ = record_committed_message(
                state,
                Some(store),
                session_id,
                &agent_instance_id,
                &message.id,
            )?;
        }
    }
    Ok(())
}

fn project_committed_message_from_state(
    state: &HostState,
    session_id: &str,
    agent_instance_id: &str,
    message_id: &str,
) -> Option<TranscriptCommittedEvent> {
    let session = state.session(session_id).ok()?;
    let SessionTreeEntry::Message(message) = session
        .entries
        .iter()
        .find(|entry| entry.id() == message_id)?
    else {
        return None;
    };
    if message.agent_instance_id != agent_instance_id {
        return None;
    }
    Some(TranscriptCommittedEvent {
        session_id: session_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        agent_id: message.agent_id.clone(),
        source_turn_id: message.source_turn_id.clone(),
        message_id: message_id.to_string(),
        transcript_seq: message.transcript_seq,
        message: message.message.clone(),
    })
}

fn project_committed_message_from_store(
    store: &dyn SessionStorePort,
    session_id: &str,
    agent_instance_id: &str,
    message_id: &str,
) -> Option<TranscriptCommittedEvent> {
    let message = store
        .find_committed_message_lenient(agent_instance_id, message_id)
        .or_else(|| {
            store
                .find_committed_message(session_id, agent_instance_id, message_id)
                .ok()
                .flatten()
        })?;
    Some(TranscriptCommittedEvent {
        session_id: session_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        agent_id: message.agent_spec_id,
        source_turn_id: message.source_turn_id.unwrap_or_default(),
        message_id: message_id.to_string(),
        transcript_seq: message.transcript_seq,
        message: message.message,
    })
}

/// Convert orchd delta output to client `StreamItem` events (sole stream wire).
pub fn stream_items_from_delta(
    session_id: &str,
    envelope: &RealtimeDeltaEnvelope,
) -> Vec<ServerMessage> {
    let Some(message_id) = envelope.message_id.as_deref() else {
        return Vec::new();
    };
    piko_protocol::StreamItemPatch::from_realtime_delta(
        Some(session_id.to_string()),
        Some(envelope.agent_instance_id.clone()),
        message_id,
        Some(envelope.delta_seq),
        &envelope.delta,
    )
    .into_iter()
    .map(ServerMessage::StreamItem)
    .collect()
}

#[allow(clippy::too_many_arguments)]
fn append_committed_message(
    state: &mut HostState,
    session_id: &str,
    agent_instance_id: &str,
    agent_id: &str,
    source_turn_id: &str,
    message: &Message,
    message_id: &str,
    transcript_seq: u64,
    parent_id: Option<&str>,
) -> Result<Option<TranscriptCommittedEvent>, ProtocolError> {
    let is_new = state
        .session(session_id)?
        .entries
        .iter()
        .all(|entry| entry.id() != message_id);
    if is_new
        && let Message::Assistant {
            usage: Some(usage), ..
        } = message
        && let Ok(session) = state.session_mut(session_id)
    {
        session.account_step_usage(Some(source_turn_id), usage);
    }
    if is_new
        && let Some(change) = file_change_from_message(message)
        && !source_turn_id.is_empty()
    {
        let _ = state.track_turn_file_change(session_id, source_turn_id, change)?;
    }
    let parent_id = parent_id
        .map(str::to_string)
        .or_else(|| {
            state
                .session(session_id)
                .ok()?
                .task_heads
                .get(agent_instance_id)
                .cloned()
        })
        .or_else(|| {
            // Cross-execution Turns: first message on a new shard has no task head yet.
            state.session(session_id).ok()?.current_leaf_id.clone()
        });

    let timestamp = message_timestamp(message).to_string();
    let entry = SessionTreeEntry::Message(MessageEntry {
        id: message_id.to_string(),
        parent_id,
        timestamp,
        agent_id: agent_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        source_turn_id: source_turn_id.to_string(),
        transcript_seq,
        message: message.clone(),
    });

    // MessageCommitted notifications arrive after the AgentInstance shard write
    // is durable; only project into HostState here.
    state.append_task_entry(session_id, agent_instance_id, entry)?;
    let committed = TranscriptCommittedEvent {
        session_id: session_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        agent_id: agent_id.to_string(),
        source_turn_id: source_turn_id.to_string(),
        message_id: message_id.to_string(),
        transcript_seq,
        message: message.clone(),
    };
    let _ = state.append_agent_view_event(
        session_id,
        agent_instance_id,
        agent_id,
        ServerMessage::TranscriptCommitted(committed.clone()),
    );
    Ok(Some(committed))
}

fn message_timestamp(message: &Message) -> &i64 {
    const DEFAULT: i64 = 0;
    match message {
        Message::Context { timestamp, .. } => timestamp.as_ref().unwrap_or(&DEFAULT),
        Message::User { timestamp, .. } => timestamp.as_ref().unwrap_or(&DEFAULT),
        Message::Assistant { timestamp, .. } => timestamp.as_ref().unwrap_or(&DEFAULT),
        Message::ToolCall { timestamp, .. } => timestamp.as_ref().unwrap_or(&DEFAULT),
        Message::ToolResult { timestamp, .. } => timestamp.as_ref().unwrap_or(&DEFAULT),
    }
}

#[cfg(test)]
#[path = "projection/tests.rs"]
mod tests;
