#![allow(clippy::disallowed_methods)]
#![allow(dead_code)]

use std::sync::Arc;

pub mod mock_session;

pub use mock_session::MockSessionPublisher;

pub fn test_oneshot<T>() -> (
    tokio::sync::oneshot::Sender<T>,
    tokio::sync::oneshot::Receiver<T>,
) {
    tokio::sync::oneshot::channel()
}

type MockRunKey = (String, String);

/// One admitted AgentInput's mock observation state. The subscription is
/// handed out once by `wait_agent_input_started`; the completion receiver is
/// handed out once by `wait_agent_input_completion`.
pub struct MockRunSlot {
    subscription: Option<piko_orchd_api::SessionSubscription>,
    completion_rx: Option<tokio::sync::oneshot::Receiver<piko_hostd::ports::AgentRunCompletion>>,
    /// Publisher retained so the mock can build the observation barrier from
    /// the live cursor at completion time.
    publisher: Arc<MockSessionPublisher>,
    input_id: String,
}

/// Control handle a mock holds after allocating a run: it can publish events
/// on `publisher` and, when finished, send a completion on `completion_tx`.
pub struct MockRunControl {
    pub publisher: Arc<MockSessionPublisher>,
    pub completion_tx: tokio::sync::oneshot::Sender<piko_hostd::ports::AgentRunCompletion>,
}

impl MockRunSlot {
    pub fn publisher(&self) -> &Arc<MockSessionPublisher> {
        &self.publisher
    }

    pub fn input_id(&self) -> &str {
        &self.input_id
    }
}

/// Shared registry used by mock runners to implement the folded
/// `AgentRunRunner` surface (submit + wait keyed by `input_id`).
#[derive(Clone, Default)]
pub struct MockRunHarness {
    slots: Arc<std::sync::Mutex<std::collections::HashMap<MockRunKey, MockRunSlot>>>,
}

impl MockRunHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &self,
        session_id: &str,
        input_id: &str,
        subscription: piko_orchd_api::SessionSubscription,
        completion_rx: tokio::sync::oneshot::Receiver<piko_hostd::ports::AgentRunCompletion>,
        publisher: Arc<MockSessionPublisher>,
    ) {
        let key = (session_id.to_string(), input_id.to_string());
        self.slots.lock().unwrap().insert(
            key,
            MockRunSlot {
                subscription: Some(subscription),
                completion_rx: Some(completion_rx),
                publisher,
                input_id: input_id.to_string(),
            },
        );
    }

    pub fn take_subscription(
        &self,
        session_id: &str,
        input_id: &str,
    ) -> piko_orchd_api::SessionSubscription {
        let key = (session_id.to_string(), input_id.to_string());
        let mut slots = self.slots.lock().unwrap();
        let slot = slots.get_mut(&key).expect("mock subscription missing");
        slot.subscription
            .take()
            .expect("mock subscription already taken")
    }

    /// Barrier cursor for a still-registered slot (does not consume it), used
    /// to build a completion that matches the published observation epoch.
    pub fn barrier_for(
        &self,
        session_id: &str,
        input_id: &str,
    ) -> Option<piko_protocol::agent_runtime::SessionCursor> {
        let key = (session_id.to_string(), input_id.to_string());
        self.slots
            .lock()
            .unwrap()
            .get(&key)
            .map(|slot| slot.publisher.cursor())
    }

    pub fn input_id(&self, session_id: &str, input_id: &str) -> Option<String> {
        let key = (session_id.to_string(), input_id.to_string());
        self.slots
            .lock()
            .unwrap()
            .get(&key)
            .map(|slot| slot.input_id.clone())
    }

    pub async fn completion(
        &self,
        session_id: &str,
        input_id: &str,
    ) -> piko_hostd::ports::AgentRunCompletion {
        let key = (session_id.to_string(), input_id.to_string());
        let receiver = {
            let mut slots = self.slots.lock().unwrap();
            let slot = slots
                .get_mut(&key)
                .expect("mock completion receiver missing");
            slot.completion_rx
                .take()
                .expect("mock completion receiver already taken")
        };
        receiver.await.expect("mock completion signal")
    }

    pub fn finish(&self, session_id: &str, input_id: &str) {
        let key = (session_id.to_string(), input_id.to_string());
        self.slots.lock().unwrap().remove(&key);
    }

    pub fn session_has_active(&self, session_id: &str) -> bool {
        self.slots
            .lock()
            .unwrap()
            .keys()
            .any(|(sid, _)| sid == session_id)
    }

    /// Register a root-input mock run and spawn a task that publishes
    /// `events` then completes with `report`. The observation barrier is the
    /// publisher's cursor after the events publish.
    pub fn publish_root(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
        events: Vec<(u64, piko_protocol::agent_runtime::SessionEvent, String)>,
        report: piko_protocol::AgentWorkReport,
    ) -> piko_protocol::AgentInputReceipt {
        let (publisher, subscription) = MockSessionPublisher::new(session_id.to_string());
        let (completion_tx, completion_rx) = test_oneshot();
        let publisher_task = Arc::clone(&publisher);
        let publish_agent = agent_instance_id.to_string();
        let publish_input_id = input_id.to_string();
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            for (seq, event, agent_id) in events {
                let _ = seq;
                publisher_task.publish(publish_agent.clone(), agent_id, 0, event);
            }
            let barrier = publisher_task.cursor();
            let _ = completion_tx.send(piko_hostd::ports::AgentRunCompletion {
                input_id: publish_input_id,
                result: Ok(report),
                observation_barrier: barrier,
            });
        });
        self.register(session_id, input_id, subscription, completion_rx, publisher);
        piko_protocol::AgentInputReceipt {
            input_id: input_id.to_string(),
            request_id: input_id.to_string(),
            session_id: session_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
            disposition: piko_protocol::AgentInputDisposition::AppliedAsRoot,
            queued_position: None,
        }
    }

    /// Allocate a run slot (publisher + completion channels + receipt) so the
    /// mock can publish events and complete at its own pace.
    pub fn alloc_root(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
        disposition: piko_protocol::AgentInputDisposition,
    ) -> (piko_protocol::AgentInputReceipt, MockRunControl) {
        let (publisher, subscription) = MockSessionPublisher::new(session_id.to_string());
        let (completion_tx, completion_rx) = test_oneshot();
        self.register(
            session_id,
            input_id,
            subscription,
            completion_rx,
            publisher.clone(),
        );
        let receipt = piko_protocol::AgentInputReceipt {
            input_id: input_id.to_string(),
            request_id: input_id.to_string(),
            session_id: session_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
            disposition,
            queued_position: None,
        };
        (
            receipt,
            MockRunControl {
                publisher,
                completion_tx,
            },
        )
    }
}

/// Build an `AgentWorkReport` for a completed mock run.
pub fn success_report(agent_instance_id: impl Into<String>) -> piko_protocol::AgentWorkReport {
    piko_protocol::AgentWorkReport {
        agent_instance_id: agent_instance_id.into(),
        root_input_id: "input-test".into(),
        report_id: format!("report_{}", uuid::Uuid::new_v4()),
        outcome: piko_protocol::ExecutionOutcome::Succeeded {
            usage: Default::default(),
        },
        summary: "done".into(),
        usage: Default::default(),
        artifacts: Vec::new(),
    }
}

pub fn running_agent_info(
    session_id: impl Into<String>,
    agent_instance_id: impl Into<String>,
) -> piko_protocol::AgentInfo {
    let session_id = session_id.into();
    let agent_instance_id = agent_instance_id.into();
    piko_protocol::AgentInfo {
        session_id,
        agent_instance_id,
        agent_id: "main".into(),
        parent_agent_instance_id: None,
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        activity: piko_protocol::AgentActivity::Running,
        unread_report_count: 0,
        name: "Main".into(),
        role: "assistant".into(),
        status: piko_protocol::AgentStatus::Running,
    }
}

use piko_protocol::agent_runtime::SessionEvent;
pub fn execution_running() -> SessionEvent {
    SessionEvent::InteractionResolved {
        interaction_id: "running".into(),
        status: piko_protocol::UserInteractionStatus::Submitted,
    }
}

pub fn execution_succeeded() -> SessionEvent {
    SessionEvent::InteractionResolved {
        interaction_id: "completed".into(),
        status: piko_protocol::UserInteractionStatus::Submitted,
    }
}

/// Plain-text projection of a `MessageContent` for mock commit mesages.
pub fn content_text(content: &piko_protocol::MessageContent) -> String {
    match content {
        piko_protocol::MessageContent::String(text) => text.clone(),
        piko_protocol::MessageContent::Blocks(blocks) => blocks
            .iter()
            .map(piko_protocol::ContentBlock::text_projection)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
