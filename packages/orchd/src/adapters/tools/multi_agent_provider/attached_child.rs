use std::sync::Arc;

use piko_orchd_api::AgentRuntimeApi;

/// Cancels only the root input created by an attached spawn. A delayed cleanup
/// therefore cannot interrupt successor work on the same AgentInstance.
pub(super) struct AttachedChildCancellation {
    runtime: Arc<dyn AgentRuntimeApi>,
    session_id: String,
    agent_instance_id: String,
    root_input_id: String,
    armed: bool,
}

impl AttachedChildCancellation {
    pub(super) fn new(
        runtime: Arc<dyn AgentRuntimeApi>,
        session_id: String,
        agent_instance_id: String,
        root_input_id: String,
    ) -> Self {
        Self {
            runtime,
            session_id,
            agent_instance_id,
            root_input_id,
            armed: true,
        }
    }

    pub(super) async fn cancel(&mut self) {
        if !self.armed {
            return;
        }
        self.armed = false;
        let _ = self
            .runtime
            .interrupt_agent_if_active(
                self.session_id.clone(),
                self.agent_instance_id.clone(),
                self.root_input_id.clone(),
            )
            .await;
    }

    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for AttachedChildCancellation {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let runtime = Arc::clone(&self.runtime);
        let session_id = self.session_id.clone();
        let agent_instance_id = self.agent_instance_id.clone();
        let root_input_id = self.root_input_id.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = runtime
                    .interrupt_agent_if_active(session_id, agent_instance_id, root_input_id)
                    .await;
            });
        }
    }
}
