use super::*;

impl AgentRuntime {
    /// Tear down a half-initialized attach without removing a newer scope for
    /// the same session.
    pub(super) async fn cleanup_session(
        runtime: &AgentRuntime,
        session_id: &str,
        scope: &Arc<SessionAgentScope>,
    ) {
        Self::remove_session_if_scope(runtime, session_id, scope).await;
        scope.shutdown().await;
        let _ = runtime
            .execution
            .detach_session(session_id.to_string())
            .await;
    }

    pub(super) async fn remove_session_if_scope(
        runtime: &AgentRuntime,
        session_id: &str,
        scope: &Arc<SessionAgentScope>,
    ) {
        let mut sessions = runtime.sessions.write().await;
        if sessions
            .get(session_id)
            .is_some_and(|slot| Arc::ptr_eq(slot.scope(), scope))
        {
            sessions.remove(session_id);
        }
    }

    /// Durably (re-)admit the root identity. The journal Create command is
    /// idempotent, so recovery replays converge on the same identity.
    pub(super) async fn commit_agent_root(
        &self,
        scope: &Arc<SessionAgentScope>,
        root: &AgentInstanceIdentity,
        root_spec: piko_protocol::AgentSpec,
        root_recovery: Option<AgentRecoveryState>,
    ) -> Result<(), AgentApiError> {
        scope
            .commit()
            .commit_agent_command(
                &root.session_id,
                AgentDurableCommand::Create {
                    identity: root.clone(),
                    spec: root_spec,
                    origin_root_input_id: None,
                    origin_tool_call_id: None,
                },
            )
            .await
            .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        self.spawn_agent_actor(scope, root.clone(), None, root_recovery)
            .await
    }

    /// Validate one connected recovered tree before any durable side effect.
    pub(super) fn validate_recovered_tree(
        root: &AgentInstanceIdentity,
        recovered_agents: &[AgentRecoveryState],
    ) -> Result<(), AgentApiError> {
        let mut identities: HashMap<&str, &AgentInstanceIdentity> = HashMap::new();
        identities.insert(root.agent_instance_id.as_str(), root);
        let mut root_recovery_seen = false;
        for state in recovered_agents {
            let identity = &state.identity;
            if identity.session_id != root.session_id {
                return Err(AgentApiError::AgentParentMismatch);
            }
            if identity.agent_instance_id == root.agent_instance_id {
                if root_recovery_seen || identity != root {
                    return Err(if root_recovery_seen {
                        AgentApiError::AgentAlreadyExists
                    } else {
                        AgentApiError::AgentParentMismatch
                    });
                }
                root_recovery_seen = true;
                continue;
            }
            if identity.parent_agent_instance_id.is_none() {
                return Err(AgentApiError::AgentParentMismatch);
            }
            if identities
                .insert(identity.agent_instance_id.as_str(), identity)
                .is_some()
            {
                return Err(AgentApiError::AgentAlreadyExists);
            }
        }
        let parents: HashMap<&str, Option<&str>> = identities
            .iter()
            .map(|(id, identity)| (*id, identity.parent_agent_instance_id.as_deref()))
            .collect();
        for (id, parent) in &parents {
            if let Some(parent_id) = parent
                && (!parents.contains_key(parent_id) || parent_id == id)
            {
                return Err(AgentApiError::AgentParentMismatch);
            }
        }
        for id in parents.keys() {
            if *id == root.agent_instance_id {
                continue;
            }
            let mut current = Some(*id);
            let mut steps = 0_usize;
            while let Some(node) = current {
                if node == root.agent_instance_id {
                    break;
                }
                steps += 1;
                if steps > parents.len() {
                    return Err(AgentApiError::AgentParentMismatch);
                }
                current = parents
                    .get(node)
                    .and_then(|parent| parent.as_ref())
                    .copied();
            }
            if current != Some(root.agent_instance_id.as_str()) {
                return Err(AgentApiError::AgentParentMismatch);
            }
        }
        Ok(())
    }
}
