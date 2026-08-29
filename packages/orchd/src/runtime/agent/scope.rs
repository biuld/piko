use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use piko_comms::BroadcastSender;
use piko_comms::contracts::AgentMailboxEvent as AgentMailboxEventContract;
use piko_orchd_api::{AgentApiError, AgentCommitPort};
use piko_protocol::AgentMailboxEvent;
use tokio::sync::Mutex;

use super::mailbox::{AgentCommand, AgentHandle};
use crate::runtime::tasks::TaskRegistry;
use piko_protocol::{CreateAgentReceipt, CreateAgentRequest};

/// Limits applied when creating new children in one session agent tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentTreeLimits {
    max_agents: usize,
    max_depth: usize,
}

impl Default for AgentTreeLimits {
    fn default() -> Self {
        Self {
            max_agents: piko_protocol::runtime::DEFAULT_MAX_AGENTS as usize,
            max_depth: piko_protocol::runtime::DEFAULT_MAX_DEPTH as usize,
        }
    }
}

impl AgentTreeLimits {
    pub(crate) fn from_runtime_config(
        config: &piko_protocol::runtime::OrchestratorRuntimeConfig,
    ) -> Self {
        Self {
            max_agents: config
                .max_agents
                .unwrap_or(piko_protocol::runtime::DEFAULT_MAX_AGENTS)
                .max(1) as usize,
            max_depth: config
                .max_depth
                .unwrap_or(piko_protocol::runtime::DEFAULT_MAX_DEPTH)
                .max(1) as usize,
        }
    }
}

pub struct SessionAgentScope {
    session_id: String,
    root_agent_instance_id: String,
    commit: std::sync::Arc<dyn AgentCommitPort>,
    agents: Mutex<HashMap<String, AgentHandle>>,
    limits: AgentTreeLimits,
    create_requests: Mutex<HashMap<String, (CreateAgentRequest, CreateAgentReceipt)>>,
    create_lock: Mutex<()>,
    generation: AtomicU64,
    tasks: Arc<TaskRegistry>,
    mailbox_events: BroadcastSender<AgentMailboxEventContract, AgentMailboxEvent>,
}

impl SessionAgentScope {
    pub fn new(
        session_id: String,
        root_agent_instance_id: String,
        commit: std::sync::Arc<dyn AgentCommitPort>,
        limits: AgentTreeLimits,
    ) -> Self {
        Self {
            tasks: TaskRegistry::new(),
            session_id,
            root_agent_instance_id,
            commit,
            agents: Mutex::new(HashMap::new()),
            limits,
            create_requests: Mutex::new(HashMap::new()),
            create_lock: Mutex::new(()),
            generation: AtomicU64::new(0),
            mailbox_events: piko_comms::broadcast::<AgentMailboxEventContract, _>().0,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn root_agent_instance_id(&self) -> &str {
        &self.root_agent_instance_id
    }

    pub fn commit(&self) -> &std::sync::Arc<dyn AgentCommitPort> {
        &self.commit
    }

    /// Session-scoped typed background tasks (F-01 / D-01).
    pub fn tasks(&self) -> &Arc<TaskRegistry> {
        &self.tasks
    }

    /// Best-effort session notification lane for mailbox updates (F-10).
    pub fn mailbox_events(&self) -> &BroadcastSender<AgentMailboxEventContract, AgentMailboxEvent> {
        &self.mailbox_events
    }

    pub fn next_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub async fn insert_agent(
        &self,
        agent_instance_id: String,
        handle: AgentHandle,
    ) -> Result<(), AgentApiError> {
        let mut agents = self.agents.lock().await;
        if agents.contains_key(&agent_instance_id) {
            return Err(AgentApiError::AgentAlreadyExists);
        }
        agents.insert(agent_instance_id, handle);
        Ok(())
    }

    pub async fn agent(&self, agent_instance_id: &str) -> Option<AgentHandle> {
        self.agents.lock().await.get(agent_instance_id).cloned()
    }

    pub async fn authorize_child_creation(&self, parent_id: &str) -> Result<(), AgentApiError> {
        let agents = self.agents.lock().await;
        let parent = agents.get(parent_id).ok_or(AgentApiError::AgentNotFound)?;
        if !parent.agent_kind.can_spawn_subagents() {
            return Err(AgentApiError::AgentCannotSpawnChildren);
        }
        Ok(())
    }

    pub async fn lock_create(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.create_lock.lock().await
    }

    pub async fn create_receipt(
        &self,
        request: &CreateAgentRequest,
    ) -> Result<Option<CreateAgentReceipt>, AgentApiError> {
        match self.create_requests.lock().await.get(&request.request_id) {
            Some((existing, receipt)) if existing == request => Ok(Some(receipt.clone())),
            Some(_) => Err(AgentApiError::IdempotencyConflict),
            None => Ok(None),
        }
    }

    pub async fn record_create(&self, request: CreateAgentRequest, receipt: CreateAgentReceipt) {
        self.create_requests
            .lock()
            .await
            .insert(request.request_id.clone(), (request, receipt));
    }

    pub async fn snapshots(&self) -> Vec<piko_protocol::AgentSnapshot> {
        self.agents
            .lock()
            .await
            .values()
            .map(|handle| handle.snapshot_rx.borrow().clone())
            .collect()
    }

    pub async fn validate_new_child(&self, parent_id: &str) -> Result<(), AgentApiError> {
        let agents = self.agents.lock().await;
        if agents.len() >= self.limits.max_agents {
            return Err(AgentApiError::AgentCountLimitExceeded);
        }
        let mut current = Some(parent_id.to_string());
        let mut depth = 0;
        while let Some(id) = current {
            depth += 1;
            if depth >= self.limits.max_depth {
                return Err(AgentApiError::AgentDepthLimitExceeded);
            }
            current = agents.get(&id).and_then(|handle| {
                handle
                    .snapshot_rx
                    .borrow()
                    .identity
                    .parent_agent_instance_id
                    .clone()
            });
        }
        Ok(())
    }

    pub async fn authorize_input(
        &self,
        caller_id: Option<&str>,
        target_id: &str,
    ) -> Result<(), AgentApiError> {
        let Some(caller_id) = caller_id else {
            return Ok(());
        };
        if caller_id == target_id {
            return Ok(());
        }
        let agents = self.agents.lock().await;
        let caller = agents
            .get(caller_id)
            .ok_or(AgentApiError::AgentUnauthorized)?;
        let target = agents.get(target_id).ok_or(AgentApiError::AgentNotFound)?;
        let caller_parent = caller
            .snapshot_rx
            .borrow()
            .identity
            .parent_agent_instance_id
            .clone();
        let target_parent = target
            .snapshot_rx
            .borrow()
            .identity
            .parent_agent_instance_id
            .clone();
        if caller_parent.as_deref() == Some(target_id)
            || target_parent.as_deref() == Some(caller_id)
        {
            Ok(())
        } else {
            Err(AgentApiError::AgentUnauthorized)
        }
    }

    pub async fn remove_if_generation(&self, agent_instance_id: &str, generation: u64) {
        let mut agents = self.agents.lock().await;
        if agents
            .get(agent_instance_id)
            .is_some_and(|handle| handle.generation == generation)
        {
            agents.remove(agent_instance_id);
        }
    }

    pub async fn shutdown(&self) {
        self.tasks.cancel_all().await;
        let handles = self
            .agents
            .lock()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for handle in handles {
            let (reply, received) =
                piko_comms::reply::<piko_comms::contracts::AgentCommandReply, _>();
            if handle
                .command_tx
                .send(AgentCommand::Shutdown { reply })
                .await
                .is_ok()
            {
                let _ = received.await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_limits_default_and_normalize_zero() {
        let defaults = AgentTreeLimits::from_runtime_config(
            &piko_protocol::runtime::OrchestratorRuntimeConfig::default(),
        );
        assert_eq!(defaults.max_agents, 32);
        assert_eq!(defaults.max_depth, 8);

        let zero = AgentTreeLimits::from_runtime_config(
            &piko_protocol::runtime::OrchestratorRuntimeConfig {
                max_concurrent_agents: None,
                max_agents: Some(0),
                max_depth: Some(0),
            },
        );
        assert_eq!(zero.max_agents, 1);
        assert_eq!(zero.max_depth, 1);
    }
}
