use super::*;

#[derive(Clone, Default)]
pub(crate) struct GatedAgentRunRunner {
    pub(crate) released: Arc<(std::sync::Mutex<bool>, tokio::sync::Notify)>,
    pub(crate) prompts: Arc<std::sync::Mutex<Vec<String>>>,
    pub(crate) submissions: Arc<std::sync::atomic::AtomicUsize>,
    harness: crate::support::MockRunHarness,
}

impl GatedAgentRunRunner {
    pub(crate) fn new() -> Self {
        Self {
            released: Arc::new((std::sync::Mutex::new(false), tokio::sync::Notify::new())),
            prompts: Arc::new(std::sync::Mutex::new(Vec::new())),
            submissions: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            harness: crate::support::MockRunHarness::new(),
        }
    }

    pub(crate) fn release(&self) {
        *self.released.0.lock().unwrap() = true;
        self.released.1.notify_waiters();
    }

    async fn wait_until_released(&self) {
        loop {
            if *self.released.0.lock().unwrap() {
                return;
            }
            self.released.1.notified().await;
        }
    }
}

#[async_trait]
impl AgentRunRunner for GatedAgentRunRunner {
    async fn ensure_session_runtime(
        &self,
        _session_id: &str,
        _cwd: &str,
        _session_dir: &std::path::Path,
        _resume_agent: Option<&piko_hostd::ports::ResumeAgent>,
    ) -> Result<(), piko_hostd::api::ProtocolError> {
        Ok(())
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        _runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_hostd::api::ProtocolError> {
        let disposition = if self
            .submissions
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            == 0
        {
            piko_protocol::AgentInputDisposition::AppliedAsRoot
        } else {
            piko_protocol::AgentInputDisposition::PendingFollowUp
        };
        let content = support::content_text(&input.content);
        if disposition == piko_protocol::AgentInputDisposition::AppliedAsRoot {
            self.prompts.lock().unwrap().push(content.clone());
        }
        let (receipt, control) = self.harness.alloc_root(
            &input.session_id,
            &input.agent_instance_id,
            &input.input_id,
            disposition,
        );
        let runner = self.clone();
        let session_id = input.session_id.clone();
        let input_id = input.input_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        tokio::spawn(async move {
            runner.wait_until_released().await;
            if disposition == piko_protocol::AgentInputDisposition::PendingFollowUp {
                runner.prompts.lock().unwrap().push(content);
            }
            control
                .publisher
                .publish(agent_instance_id.clone(), "main", 1, execution_running());
            control
                .publisher
                .publish(agent_instance_id.clone(), "main", 2, execution_succeeded());
            let barrier = control.publisher.cursor();
            let _ = control
                .completion_tx
                .send(piko_hostd::ports::AgentRunCompletion {
                    input_id,
                    result: Ok(success_report(&agent_instance_id)),
                    observation_barrier: barrier,
                });
            let _ = session_id;
        });
        Ok(receipt)
    }

    async fn wait_agent_input_started(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
        _disposition: piko_protocol::AgentInputDisposition,
    ) -> Result<piko_orchd_api::SessionSubscription, piko_hostd::api::ProtocolError> {
        Ok(self.harness.take_subscription(session_id, input_id))
    }

    async fn wait_agent_input_completion(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
    ) -> Result<piko_hostd::ports::AgentRunCompletion, piko_hostd::api::ProtocolError> {
        Ok(self.harness.completion(session_id, input_id).await)
    }

    async fn finish_agent_run(&self, session_id: &str, _agent_instance_id: &str, input_id: &str) {
        self.harness.finish(session_id, input_id);
    }
}
