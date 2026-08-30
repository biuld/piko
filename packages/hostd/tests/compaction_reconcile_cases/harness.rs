// ---- Compaction test harness: scripted gateways and turn runners ----

struct SummaryGateway;

#[async_trait]
impl InferenceGateway for SummaryGateway {
    async fn start(
        &self,
        _req: InferenceRequest,
        _cancel: CancellationToken,
    ) -> Result<InferenceExecution, InferenceError> {
        Ok(text_execution("## Goal\n- test compact\n"))
    }
}

/// Summarizer that fails the first call and records every call's model id.
struct FailingOnceGateway {
    calls: std::sync::Mutex<Vec<String>>,
    failed_once: std::sync::atomic::AtomicBool,
}

impl FailingOnceGateway {
    fn new() -> Self {
        Self {
            calls: std::sync::Mutex::new(Vec::new()),
            failed_once: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl InferenceGateway for FailingOnceGateway {
    async fn start(
        &self,
        req: InferenceRequest,
        _cancel: CancellationToken,
    ) -> Result<InferenceExecution, InferenceError> {
        self.calls.lock().unwrap().push(req.model.model.clone());
        if !self
            .failed_once
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            Err(InferenceError::new(
                piko_llmd::gateway::ErrorClass::Upstream,
                "summary",
                "start",
                "transient summarizer failure",
            ))
        } else {
            Ok(text_execution("## Goal\n- fallback summary\n"))
        }
    }
}

/// Summarizer that blocks until released, for the pending-guard race.
struct BlockingGateway {
    release: Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Receiver<()>>>>,
    started: Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl BlockingGateway {
    fn new(
        started: tokio::sync::oneshot::Sender<()>,
        release: tokio::sync::oneshot::Receiver<()>,
    ) -> Self {
        Self {
            release: Arc::new(tokio::sync::Mutex::new(Some(release))),
            started: Arc::new(tokio::sync::Mutex::new(Some(started))),
        }
    }
}

#[async_trait]
impl InferenceGateway for BlockingGateway {
    async fn start(
        &self,
        _req: InferenceRequest,
        _cancel: CancellationToken,
    ) -> Result<InferenceExecution, InferenceError> {
        if let Some(started) = self.started.lock().await.take() {
            let _ = started.send(());
        }
        let receiver = self
            .release
            .lock()
            .await
            .take()
            .expect("release receiver available once");
        let _ = receiver.await;
        Ok(text_execution("## Goal\n- released\n"))
    }
}

struct CompactAgentRunRunner {
    harness: crate::support::MockRunHarness,
    session_dir: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
}

impl CompactAgentRunRunner {
    fn new() -> Self {
        Self {
            harness: crate::support::MockRunHarness::new(),
            session_dir: Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

#[async_trait]
impl AgentRunRunner for CompactAgentRunRunner {
    async fn ensure_session_runtime(
        &self,
        _session_id: &str,
        _cwd: &str,
        session_dir: &std::path::Path,
        _resume_agent: Option<&piko_hostd::ports::ResumeAgent>,
    ) -> Result<(), piko_hostd::api::ProtocolError> {
        *self.session_dir.lock().unwrap() = Some(session_dir.to_path_buf());
        Ok(())
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        _runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_hostd::api::ProtocolError> {
        let session_dir = self
            .session_dir
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_default();
        let store = SessionStore::new(session_dir);
        let session_id = input.session_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        let turn_id = input.input_id.clone();
        let prompt = support::content_text(&input.content);

        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                root_input_id: "input-1".into(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: "user-1".into(),
                    parent_message_id: None,
                    tree_parent_entry_id: None,
                    message: Message::User {
                        content: MessageContent::String(prompt),
                        timestamp: Some(1),
                    },
                    committed_at: 1,
                },
                "main",
            )
            .unwrap();
        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                root_input_id: "input-1".into(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: "assistant-1".into(),
                    parent_message_id: Some("user-1".into()),
                    tree_parent_entry_id: None,
                    message: Message::Assistant {
                        content: vec![ContentBlock::Text {
                            text: "world".into(),
                        }],
                        checkpoint: None,
                        provider: "test-provider".into(),
                        model: "test-model".into(),
                        usage: None,
                        stop_reason: None,
                        error_message: None,
                        timestamp: Some(3),
                    },
                    committed_at: 3,
                },
                "main",
            )
            .unwrap();

        let events = vec![
            (
                0,
                execution_running(),
                "main".to_string(),
            ),
            (
                1,
                SessionEvent::MessageCommitted {
                    transcript_seq: 1,
                    message_id: "user-1".into(),
                    source_turn_id: turn_id.clone(),
                    role: MessageRole::User,
                },
                "main".to_string(),
            ),
            (
                2,
                SessionEvent::MessageCommitted {
                    transcript_seq: 2,
                    message_id: "assistant-1".into(),
                    source_turn_id: turn_id,
                    role: MessageRole::Assistant,
                },
                "main".to_string(),
            ),
            (
                3,
                execution_succeeded(),
                "main".to_string(),
            ),
        ];
        let _ = (&session_id, &input);
        Ok(self.harness.publish_root(
            &input.session_id,
            &agent_instance_id,
            &input.input_id,
            events,
            success_report(&agent_instance_id),
        ))
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


struct DistinctIdRunRunner {
    harness: crate::support::MockRunHarness,
    session_dir: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
}

impl DistinctIdRunRunner {
    fn new() -> Self {
        Self {
            harness: crate::support::MockRunHarness::new(),
            session_dir: Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

#[async_trait]
impl AgentRunRunner for DistinctIdRunRunner {
    async fn ensure_session_runtime(
        &self,
        _session_id: &str,
        _cwd: &str,
        session_dir: &std::path::Path,
        _resume_agent: Option<&piko_hostd::ports::ResumeAgent>,
    ) -> Result<(), piko_hostd::api::ProtocolError> {
        *self.session_dir.lock().unwrap() = Some(session_dir.to_path_buf());
        Ok(())
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        _runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_hostd::api::ProtocolError> {
        use piko_protocol::execution::MessageCommit;

        let session_dir = self
            .session_dir
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_default();
        let store = SessionStore::new(session_dir);
        let session_id = input.session_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        let turn_id = input.input_id.clone();
        let user_id = format!("user-{turn_id}");
        let assistant_id = format!("assistant-{turn_id}");
        let prompt = support::content_text(&input.content);
        // Chain onto the private transcript head so repeated turns stay linear.
        let user_parent = store
            .load_agent(&session_id, &agent_instance_id)
            .ok()
            .and_then(|recovered| recovered.head_message_id);

        store
            .commit_message(
                MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                root_input_id: "input-1".into(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: user_id.clone(),
                    parent_message_id: user_parent,
                    tree_parent_entry_id: None,
                    message: Message::User {
                        content: MessageContent::String(prompt),
                        timestamp: Some(1),
                    },
                    committed_at: 1,
                },
                "main",
            )
            .unwrap();
        store
            .commit_message(
                MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                root_input_id: "input-1".into(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: assistant_id.clone(),
                    parent_message_id: Some(user_id.clone()),
                    tree_parent_entry_id: None,
                    message: Message::Assistant {
                        content: vec![ContentBlock::Text {
                            text: "world".into(),
                        }],
                        checkpoint: None,
                        provider: "test-provider".into(),
                        model: "test-model".into(),
                        usage: None,
                        stop_reason: None,
                        error_message: None,
                        timestamp: Some(3),
                    },
                    committed_at: 3,
                },
                "main",
            )
            .unwrap();

        let events = vec![
            (
                0,
                execution_running(),
                "main".to_string(),
            ),
            (
                1,
                SessionEvent::MessageCommitted {
                    transcript_seq: 1,
                    message_id: user_id,
                    source_turn_id: turn_id.clone(),
                    role: MessageRole::User,
                },
                "main".to_string(),
            ),
            (
                2,
                SessionEvent::MessageCommitted {
                    transcript_seq: 2,
                    message_id: assistant_id,
                    source_turn_id: turn_id,
                    role: MessageRole::Assistant,
                },
                "main".to_string(),
            ),
            (
                3,
                execution_succeeded(),
                "main".to_string(),
            ),
        ];
        let _ = (&session_id, &input);
        Ok(self.harness.publish_root(
            &input.session_id,
            &agent_instance_id,
            &input.input_id,
            events,
            success_report(&agent_instance_id),
        ))
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
