use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::{StreamExt, StreamMap};

use llmd::gateway::LlmGateway;
use orchd::Supervisor;
use orchd::AgentReport;
use orchd::application::PendingStream;
use orchd::protocol::agents::{AgentSpec, HostTaskContext};
use orchd::protocol::runtime::{OrchRunCommandOptions, OrchRunOptions};

use crate::api::{Event, ProtocolError};
use crate::domain::config::{McpServerConfig, SandboxSettings};
use crate::domain::turns::runner::{TurnRunInput, TurnRunOutput, TurnRunner};

#[derive(Clone)]
pub struct OrchTurnRunner {
    supervisor: Arc<Supervisor>,
}

impl OrchTurnRunner {
    pub async fn new(
        model_executor: Arc<dyn LlmGateway>,
        provider: &str,
        api_key: &str,
        model_id: &str,
    ) -> Self {
        Self::new_with_mcp(
            model_executor, provider, api_key, model_id, None, None, &[], None,
        )
        .await
    }

    pub async fn new_with_mcp(
        model_executor: Arc<dyn LlmGateway>,
        provider: &str,
        api_key: &str,
        model_id: &str,
        thinking_level: Option<piko_protocol::model::ThinkingLevel>,
        thinking_level_map: piko_protocol::model::ThinkingLevelMap,
        mcp_configs: &[McpServerConfig],
        sandbox_settings: Option<&SandboxSettings>,
    ) -> Self {
        use orchd::protocol::config::{ModelRef, OrchdConfig, ProviderConfig, SandboxConfig};
        use orchd::protocol::model::ModelRunSettings;

        let mut providers = std::collections::HashMap::new();
        providers.insert(
            provider.to_string(),
            ProviderConfig {
                kind: provider.to_string(),
                api_key: api_key.to_string(),
                base_url: None,
                headers: None,
            },
        );

        let default_settings = ModelRunSettings {
            thinking_level,
            allow_tool_calls: true,
            ..Default::default()
        };

        let sandbox = sandbox_settings
            .map(|s| SandboxConfig {
                enabled: s.enabled.unwrap_or(false),
                policy_path: s.policy_path.clone(),
                shell_path: s.shell_path.clone(),
            })
            .unwrap_or_default();

        let config = OrchdConfig {
            providers,
            agents: Default::default(),
            default_model: ModelRef {
                provider: provider.to_string(),
                model_id: model_id.to_string(),
            },
            default_settings,
            runtime: Default::default(),
            thinking_level_map,
            sandbox,
        };
        let supervisor = Supervisor::from_config(model_executor, config).await;

        // Initialize MCP tools
        let registry = supervisor.tool_registry().clone();
        let registered = crate::infra::mcp::initialize_mcp_tools(mcp_configs, registry).await;
        if !registered.is_empty() {
            tracing::info!("MCP tools registered: {:?}", registered);
        }

        Self { supervisor }
    }
}

#[async_trait]
impl TurnRunner for OrchTurnRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
        event_tx: Option<UnboundedSender<Event>>,
    ) -> Result<TurnRunOutput, ProtocolError> {
        let mut events = Vec::new();
        let session_id = input.session_id.clone();
        let turn_id = input.turn_id.clone();
        let agent_id = "main".to_string();

        // Register agent
        let agent_spec = AgentSpec {
            id: agent_id.clone(),
            name: agent_id.clone(),
            role: "assistant".into(),
            description: Some("hostd-managed agent".into()),
            system_prompt: input.system_prompt.clone(),
            model: None,
            tool_set_ids: vec!["builtin".into(), "workspace".into()],
            active_tool_names: input.active_tool_names.clone(),
            thinking_level: None,
        };
        self.supervisor.register_agent(agent_spec.clone()).await;

        // Start root agent stream
        let root_stream = self
            .supervisor
            .run_streaming(
                &input.prompt,
                Some(OrchRunOptions {
                    command: OrchRunCommandOptions {
                        target_agent_id: Some(agent_id.clone()),
                    },
                    history: None,
                    host_context: Some(HostTaskContext {
                        session_id: session_id.clone(),
                        turn_id: turn_id.clone(),
                    }),
                }),
            )
            .await;

        // StreamMap with the root stream as initial entry
        let mut map: StreamMap<String, Pin<Box<dyn Stream<Item = Event> + Send>>> = StreamMap::new();
        map.insert("root".into(), root_stream);

        let mut total_task_count: u32 = 0;
        let mut next_sub_idx: u32 = 0;

        // Consume from StreamMap — polls all streams concurrently
        while let Some((_stream_key, event)) = map.next().await {
            if matches!(&event, Event::TaskCreated { .. }) {
                total_task_count += 1;
            }

            // Record sub-agent results for spawn / await_task
            match &event {
                Event::TaskCompleted { task_id, summary, final_status, total_steps, .. } => {
                    self.supervisor.record_task_result(task_id, AgentReport {
                        text: summary.clone(),
                        status: final_status.clone(),
                        total_steps: *total_steps,
                        task_id: None,
                    }).await;
                }
                Event::TaskFailed { task_id, error, .. } => {
                    self.supervisor.record_task_result(task_id, AgentReport {
                        text: error.clone(),
                        status: "error".into(),
                        total_steps: 0,
                        task_id: None,
                    }).await;
                }
                Event::TaskCancelled { task_id, .. } => {
                    self.supervisor.record_task_result(task_id, AgentReport {
                        text: "cancelled".into(),
                        status: "cancelled".into(),
                        total_steps: 0,
                        task_id: None,
                    }).await;
                }
                _ => {}
            }

            emit_or_collect(&mut events, event, &event_tx);

            // After each event, poll for newly spawned sub-agent streams
            let new_streams: Vec<PendingStream> = self.supervisor.take_pending_streams().await;
            for ps in new_streams {
                let key = format!("sub_{}", next_sub_idx);
                next_sub_idx += 1;
                map.insert(key, ps.stream);
            }
        }

        self.supervisor.unregister_agent(&agent_id).await;

        Ok(TurnRunOutput {
            events,
            total_tasks: total_task_count.max(1),
        })
    }

    async fn steer_task(
        &self,
        task_id: &str,
        _source_task_id: &str,
        _source_agent_id: &str,
        message: &str,
    ) -> bool {
        self.supervisor.to_spawner().steer_task(task_id, message).await
    }
}

fn emit_or_collect(
    events: &mut Vec<Event>,
    event: Event,
    event_tx: &Option<UnboundedSender<Event>>,
) {
    if let Some(tx) = event_tx {
        let _ = tx.send(event);
    } else {
        events.push(event);
    }
}
