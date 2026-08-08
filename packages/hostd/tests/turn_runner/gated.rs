use super::*;

#[derive(Clone)]
pub(crate) struct GatedAgentRunRunner {
    pub(crate) released: Arc<(std::sync::Mutex<bool>, tokio::sync::Notify)>,
    pub(crate) prompts: Arc<std::sync::Mutex<Vec<String>>>,
    pub(crate) submissions: Arc<std::sync::atomic::AtomicUsize>,
}

impl GatedAgentRunRunner {
    pub(crate) fn new() -> Self {
        Self {
            released: Arc::new((std::sync::Mutex::new(false), tokio::sync::Notify::new())),
            prompts: Arc::new(std::sync::Mutex::new(Vec::new())),
            submissions: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
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
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, piko_hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let disposition = if self
            .submissions
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            == 0
        {
            piko_protocol::InputDisposition::Accepted
        } else {
            piko_protocol::InputDisposition::Queued
        };
        if disposition == piko_protocol::InputDisposition::Accepted {
            self.prompts.lock().unwrap().push(input.prompt.clone());
        }
        let (started_tx, started) = support::test_oneshot();
        let epoch = subscription.cursor.epoch.clone();
        let queued_start = if disposition == piko_protocol::InputDisposition::Accepted {
            let _ = started_tx.send(subscription);
            None
        } else {
            Some((started_tx, subscription))
        };
        let (completion_tx, completion) = support::test_oneshot();
        let runner = self.clone();
        let session_id = input.session_id.clone();
        let operation_id = input.operation_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        let prompt = input.prompt.clone();
        let address = piko_hostd::ports::AgentOperationAddress {
            session_id: session_id.clone(),
            operation_id: operation_id.clone(),
            agent_instance_id: agent_instance_id.clone(),
        };
        let completion_address = address.clone();
        tokio::spawn(async move {
            runner.wait_until_released().await;
            if let Some((started_tx, subscription)) = queued_start {
                runner.prompts.lock().unwrap().push(prompt);
                let _ = started_tx.send(subscription);
            }
            publisher.publish(agent_instance_id.clone(), "main", 1, execution_running());
            publisher.publish(agent_instance_id.clone(), "main", 2, execution_succeeded());
            let _ = completion_tx.send(piko_hostd::ports::AgentRunCompletion {
                address: completion_address,
                result: Ok(success_report(agent_instance_id)),
                observation_barrier: piko_protocol::agent_runtime::SessionCursor { epoch, seq: 2 },
            });
        });
        Ok(AgentRunHandle {
            address,
            receipt: piko_protocol::AgentInputReceipt {
                request_id: operation_id,
                session_id: input.session_id,
                agent_instance_id: input.agent_instance_id,
                disposition,
            },
            process: test_agent_run_process(started, completion),
        })
    }
}
