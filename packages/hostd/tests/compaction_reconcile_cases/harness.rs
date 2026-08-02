// ---- Compaction test harness: scripted gateways and turn runners ----

struct SummaryGateway;

#[async_trait]
impl LlmGateway for SummaryGateway {
    async fn chat_stream(
        &self,
        _req: GatewayRequest,
        _cancel: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>>, String> {
        Err("not used".into())
    }

    async fn llm_call(
        &self,
        _model: Model,
        _system_prompt: Option<String>,
        _messages: Vec<Message>,
        _settings: ModelRunSettings,
    ) -> Result<String, String> {
        Ok("## Goal\n- test compact\n".into())
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
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
impl LlmGateway for FailingOnceGateway {
    async fn chat_stream(
        &self,
        _req: GatewayRequest,
        _cancel: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>>, String> {
        Err("not used".into())
    }

    async fn llm_call(
        &self,
        model: Model,
        _system_prompt: Option<String>,
        _messages: Vec<Message>,
        _settings: ModelRunSettings,
    ) -> Result<String, String> {
        self.calls.lock().unwrap().push(model.id.clone());
        if !self
            .failed_once
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            Err("transient summarizer failure".into())
        } else {
            Ok("## Goal\n- fallback summary\n".into())
        }
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
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
impl LlmGateway for BlockingGateway {
    async fn chat_stream(
        &self,
        _req: GatewayRequest,
        _cancel: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>>, String> {
        Err("not used".into())
    }

    async fn llm_call(
        &self,
        _model: Model,
        _system_prompt: Option<String>,
        _messages: Vec<Message>,
        _settings: ModelRunSettings,
    ) -> Result<String, String> {
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
        Ok("## Goal\n- released\n".into())
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
}

struct CompactAgentRunRunner;

#[async_trait]
impl AgentRunRunner for CompactAgentRunRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, piko_hostd::api::ProtocolError> {
        let store = SessionStore::new(&input.session_dir);
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let session_id = input.session_id.clone();
        let agent_instance_id = input.operation_id.clone();
        let turn_id = input.operation_id.clone();
        let prompt = input.prompt.clone();

        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                    execution_id: agent_instance_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: "user-1".into(),
                    parent_message_id: None,
                    message: Message::User {
                        content: MessageContent::String(prompt),
                        timestamp: Some(1),
                    },
                    committed_at: 1,
                },
                "agent-1",
            )
            .unwrap();
        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                    execution_id: agent_instance_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: "assistant-1".into(),
                    parent_message_id: Some("user-1".into()),
                    message: Message::Assistant {
                        content: vec![ContentBlock::Text {
                            text: "world".into(),
                        }],
                        api: "test".into(),
                        provider: "test-provider".into(),
                        model: "test-model".into(),
                        usage: None,
                        stop_reason: None,
                        error_message: None,
                        timestamp: Some(3),
                    },
                    committed_at: 3,
                },
                "agent-1",
            )
            .unwrap();

        let publisher_task = Arc::clone(&publisher);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            publisher_task.publish(agent_instance_id.clone(), "agent-1", 0, execution_running());
            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                1,
                SessionEvent::MessageCommitted {
                    transcript_seq: 1,
                    message_id: "user-1".into(),
                    source_turn_id: turn_id.clone(),
                    role: MessageRole::User,
                },
            );
            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                2,
                SessionEvent::MessageCommitted {
                    transcript_seq: 2,
                    message_id: "assistant-1".into(),
                    source_turn_id: turn_id.clone(),
                    role: MessageRole::Assistant,
                },
            );
            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                3,
                execution_succeeded(),
            );
        });

        Ok(successful_turn_run(
            subscription,
            input.session_id,
            input.operation_id,
            input.agent_instance_id,
            3,
            std::time::Duration::ZERO,
        ))
    }
}


struct DistinctIdRunRunner;

#[async_trait]
impl AgentRunRunner for DistinctIdRunRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, piko_hostd::api::ProtocolError> {
        use piko_protocol::execution::MessageCommit;

        let store = SessionStore::new(&input.session_dir);
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let session_id = input.session_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        let turn_id = input.operation_id.clone();
        let user_id = format!("user-{turn_id}");
        let assistant_id = format!("assistant-{turn_id}");
        let prompt = input.prompt.clone();
        // Chain onto the shard head so repeated turns stay linear.
        let user_parent = store
            .load_agent(&session_id, &agent_instance_id)
            .ok()
            .and_then(|recovered| recovered.head_message_id);

        store
            .commit_message(
                MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                    execution_id: agent_instance_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: user_id.clone(),
                    parent_message_id: user_parent,
                    message: Message::User {
                        content: MessageContent::String(prompt),
                        timestamp: Some(1),
                    },
                    committed_at: 1,
                },
                "agent-1",
            )
            .unwrap();
        store
            .commit_message(
                MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                    execution_id: agent_instance_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: assistant_id.clone(),
                    parent_message_id: Some(user_id.clone()),
                    message: Message::Assistant {
                        content: vec![ContentBlock::Text {
                            text: "world".into(),
                        }],
                        api: "test".into(),
                        provider: "test-provider".into(),
                        model: "test-model".into(),
                        usage: None,
                        stop_reason: None,
                        error_message: None,
                        timestamp: Some(3),
                    },
                    committed_at: 3,
                },
                "agent-1",
            )
            .unwrap();

        let publisher_task = Arc::clone(&publisher);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            publisher_task.publish(agent_instance_id.clone(), "agent-1", 0, execution_running());
            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                1,
                SessionEvent::MessageCommitted {
                    transcript_seq: 1,
                    message_id: user_id,
                    source_turn_id: turn_id.clone(),
                    role: MessageRole::User,
                },
            );
            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                2,
                SessionEvent::MessageCommitted {
                    transcript_seq: 2,
                    message_id: assistant_id,
                    source_turn_id: turn_id,
                    role: MessageRole::Assistant,
                },
            );
            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                3,
                execution_succeeded(),
            );
        });

        Ok(successful_turn_run(
            subscription,
            input.session_id,
            input.operation_id,
            input.agent_instance_id,
            3,
            std::time::Duration::ZERO,
        ))
    }
}

