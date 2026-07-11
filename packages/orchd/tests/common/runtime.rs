#![allow(dead_code)]

use std::sync::Arc;

use orchd::AgentRuntimeService;
use orchd::host::Supervisor;
use orchd::integration::PersistSink;
use orchd::testing::CollectingPersistSink;
use piko_protocol::ServerMessage as Event;
use piko_protocol::agents::{AgentSpec, HostTaskContext};
use piko_protocol::config::OrchdConfig;
use piko_protocol::runtime::OrchRunOptions;

use crate::session_output::subscription_event_stream;

pub const TEST_STREAM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub async fn test_supervisor(
    gateway: Arc<dyn llmd::gateway::LlmGateway>,
    config: OrchdConfig,
) -> Arc<Supervisor> {
    let supervisor = Supervisor::from_config(gateway, config).await;
    supervisor
        .set_persist_sink(Arc::new(CollectingPersistSink::new()) as Arc<dyn PersistSink>)
        .await;
    supervisor
}

pub fn resolve_run_opts(opts: Option<OrchRunOptions>) -> OrchRunOptions {
    let mut opts = opts.unwrap_or_default();
    if opts.host_context.is_none() {
        let id = uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .take(12)
            .collect::<String>();
        opts.host_context = Some(HostTaskContext::new(format!("run_local_{id}")));
        opts.source_turn_id
            .get_or_insert_with(|| format!("turn_local_{id}"));
        opts.work_id
            .get_or_insert_with(|| format!("work_local_{id}"));
    }
    opts
}

pub async fn run_test_stream(
    supervisor: &Supervisor,
    prompt: &str,
    opts: Option<OrchRunOptions>,
) -> std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Event> + Send>> {
    let opts = resolve_run_opts(opts);
    let host_context = opts.host_context.expect("host_context resolved");
    let agent_id = opts
        .command
        .target_agent_id
        .unwrap_or_else(|| "main".to_string());
    let session_id = host_context.session_id.clone();
    let source_turn_id = opts
        .source_turn_id
        .unwrap_or_else(|| "turn_test".to_string());
    let work_id = opts.work_id.unwrap_or_else(|| "work_test".to_string());
    let runtime = AgentRuntimeService::runtime_for(supervisor);
    let subscription = runtime
        .start_root_turn(
            &session_id,
            &source_turn_id,
            &work_id,
            &agent_id,
            prompt,
            None,
            None,
        )
        .await
        .expect("start_root_turn");
    subscription_event_stream(subscription)
}

pub fn test_config() -> OrchdConfig {
    let mut config = OrchdConfig::single_provider("faux", "test-key", "faux-1");
    config.agents.clear();
    config
}

pub fn test_agent_spec(id: &str) -> AgentSpec {
    AgentSpec {
        id: id.to_string(),
        name: id.to_string(),
        role: "test".to_string(),
        description: None,
        system_prompt: "You are a test agent.".to_string(),
        model: None,
        tool_set_ids: vec![],
        active_tool_names: None,
        thinking_level: None,
    }
}

pub async fn wait_for_task_status(
    supervisor: &Supervisor,
    task_id: &str,
    expected: piko_protocol::agents::AgentTaskStatus,
) {
    let reached = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let snapshot = supervisor.snapshot().await;
            if snapshot
                .tasks
                .get(task_id)
                .is_some_and(|task| task.status == expected)
            {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    if reached.is_err() {
        let actual = supervisor
            .snapshot()
            .await
            .tasks
            .get(task_id)
            .map(|task| task.status.clone());
        panic!("task {task_id} did not reach {expected:?}; actual status: {actual:?}");
    }
}

pub async fn wait_for_task_report(
    supervisor: &Supervisor,
    task_id: &str,
) -> orchd::testing::TaskReport {
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(report) = supervisor.poll_task(task_id).await {
                return report;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await;
    match result {
        Ok(report) => report,
        Err(_) => panic!(
            "task {task_id} did not produce a report; snapshot: {:?}",
            supervisor.snapshot().await
        ),
    }
}
