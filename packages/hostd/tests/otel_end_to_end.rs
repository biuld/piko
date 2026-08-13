//! End-to-end observability verification: one turn produces a trace tree
//! turn.run → agent.run → model.step → tool.batch → tool.call with the
//! expected correlation attributes, and turn metrics are recorded.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::logs::{InMemoryLogExporterBuilder, SdkLoggerProvider};
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::testing::trace::new_tokio_test_exporter;
use opentelemetry_sdk::trace::SdkTracerProvider;
use piko_hostd::adapters::OrchAgentRunRunner;
use piko_hostd::api::{Command, CommandResult, ServerMessage};
use piko_hostd::infra::storage::{JsonlSessionRepository, SessionStore};
use piko_hostd::protocol::HostServer;
use piko_llmd::gateway::{
    InferenceError, InferenceEvent, InferenceExecution, InferenceGateway, InferenceRequest,
};
use tokio_stream::iter;
use tokio_util::sync::CancellationToken;

fn execution(events: Vec<InferenceEvent>) -> InferenceExecution {
    InferenceExecution {
        events: Box::pin(iter(events)),
        handle: None,
    }
}

/// Step 1 emits a tool call for the built-in `todo_write` tool; every later
/// step emits a plain text reply so the run terminates.
struct ScriptedGateway {
    step: AtomicUsize,
}

impl ScriptedGateway {
    fn new() -> Self {
        Self {
            step: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl InferenceGateway for ScriptedGateway {
    async fn start(
        &self,
        _request: InferenceRequest,
        _cancel: CancellationToken,
    ) -> Result<InferenceExecution, InferenceError> {
        let step = self.step.fetch_add(1, Ordering::SeqCst);
        if step == 0 {
            Ok(execution(vec![
                InferenceEvent::function_call(
                    "call-todo",
                    "todo_write",
                    r#"{"todos":[{"id":1,"status":"pending","content":"plan"}]}"#,
                ),
                InferenceEvent::Usage(piko_protocol::Usage::empty()),
                InferenceEvent::completed("tool_use"),
            ]))
        } else {
            Ok(execution(vec![
                InferenceEvent::text("done"),
                InferenceEvent::Usage(piko_protocol::Usage::empty()),
                InferenceEvent::completed("stop"),
            ]))
        }
    }
}

#[tokio::test]
async fn turn_produces_end_to_end_trace_tree_and_turn_metrics() {
    // ---- test OTel backend (in-memory spans + manual metric reader) ----
    let (span_exporter, mut span_rx, _shutdown_rx) = new_tokio_test_exporter();
    let tracer_provider = SdkTracerProvider::builder()
        .with_simple_exporter(span_exporter)
        .build();
    let metric_exporter = InMemoryMetricExporter::default();
    let meter_provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(metric_exporter.clone()).build())
        .build();
    let log_exporter = InMemoryLogExporterBuilder::default().build();
    let logger_provider = SdkLoggerProvider::builder()
        .with_simple_exporter(log_exporter.clone())
        .build();
    opentelemetry::global::set_tracer_provider(tracer_provider.clone());
    opentelemetry::global::set_meter_provider(meter_provider.clone());
    let _meter_provider_keepalive = meter_provider.clone();
    let otel_layer = tracing_opentelemetry::layer()
        .with_tracer(opentelemetry::global::tracer("otel_end_to_end_test"));
    let logs_bridge = OpenTelemetryTracingBridge::new(&logger_provider);
    let subscriber = tracing_subscriber::registry()
        .with(otel_layer)
        .with(logs_bridge)
        .with(tracing_subscriber::fmt::layer().with_ansi(false));
    use tracing_subscriber::layer::SubscriberExt;
    tracing::subscriber::set_global_default(subscriber).unwrap();
    piko_hostd::telemetry::init(true, false);

    // ---- run one turn through the real hostd path ----
    let temp = tempfile::tempdir().unwrap();
    let initial = HostServer::with_storage(JsonlSessionRepository::new(temp.path()));
    let created = initial
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/project".into(),
        })
        .await;
    let session_id = created
        .iter()
        .find_map(|event| match event {
            ServerMessage::CommandResponse {
                result: Ok(CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .unwrap();
    let listed = initial
        .handle_command(Command::SessionList {
            command_id: "list".into(),
            scope: piko_protocol::SessionListScope::All,
            cwd: None,
        })
        .await;
    let session_path = listed
        .iter()
        .find_map(|event| match event {
            ServerMessage::CommandResponse {
                result: Ok(CommandResult::SessionListed { sessions, .. }),
                ..
            } => sessions
                .iter()
                .find(|session| session.session_id == session_id)
                .and_then(|session| session.session_path.clone()),
            _ => None,
        })
        .unwrap();
    let store = SessionStore::new(&session_path);
    let root = store.ensure_root_agent("main").unwrap();
    let root_agent_instance_id = root.agent_instance_id.clone();

    let runner = Arc::new(
        OrchAgentRunRunner::new(Arc::new(ScriptedGateway::new()), "test", "test-model").await,
    );
    let server =
        HostServer::with_storage_and_runner(JsonlSessionRepository::new(temp.path()), runner);
    server
        .handle_command(Command::SessionOpen {
            command_id: "open".into(),
            session_id: session_id.clone(),
            session_path: Some(session_path.clone()),
        })
        .await;
    server
        .handle_command(Command::AgentSubscribe {
            command_id: "subscribe".into(),
            session_id: session_id.clone(),
            agent_instance_id: root_agent_instance_id.clone(),
            after_seq: None,
        })
        .await;
    let events = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        server.handle_command(Command::ChatSubmit {
            command_id: "otel-turn".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: root_agent_instance_id.clone(),
            text: "plan this".into(),
        }),
    )
    .await
    .expect("turn should complete");
    assert!(events.iter().any(|event| matches!(
        event,
        ServerMessage::TurnLifecycle(piko_protocol::TurnEvent::Completed {
            agent_instance_id,
            ..
        }) if agent_instance_id == &root_agent_instance_id
    )));

    // ---- flush and collect spans ----
    tracer_provider.force_flush().unwrap();
    let mut spans = Vec::new();
    while let Ok(span) = span_rx.try_recv() {
        spans.push(span);
    }

    let names: std::collections::HashSet<&str> =
        spans.iter().map(|span| span.name.as_ref()).collect();
    for expected in [
        "turn.run",
        "agent.run",
        "piko.prompt.assemble",
        "model.step",
        "tool.batch",
        "tool.call",
    ] {
        assert!(
            names.contains(expected),
            "expected span {expected:?} in exported spans: {spans:?}"
        );
    }

    // ---- hierarchy: turn → agent → step → batch → call ----
    let by_name = |name: &str| {
        spans
            .iter()
            .filter(|span| span.name.as_ref() == name)
            .collect::<Vec<_>>()
    };
    let turn_spans = by_name("turn.run");
    let turn = turn_spans.first().expect("turn.run span");
    let agent_spans = by_name("agent.run");
    let agent = agent_spans.first().expect("agent.run span");
    let prompt_spans = by_name("piko.prompt.assemble");
    let prompt = prompt_spans.first().expect("piko.prompt.assemble span");
    let step_spans = by_name("model.step");
    let step = step_spans.first().expect("model.step span");
    let batch_spans = by_name("tool.batch");
    let batch = batch_spans.first().expect("tool.batch span");
    let call_spans = by_name("tool.call");
    let call = call_spans.first().expect("tool.call span");
    assert_eq!(agent.parent_span_id, turn.span_context.span_id());
    assert_eq!(prompt.parent_span_id, agent.span_context.span_id());
    assert_eq!(step.parent_span_id, agent.span_context.span_id());
    assert_eq!(batch.parent_span_id, step.span_context.span_id());
    assert_eq!(call.parent_span_id, batch.span_context.span_id());

    // ---- correlation attributes ----
    let attr = |span: &opentelemetry_sdk::trace::SpanData, key: &str| -> Option<String> {
        span.attributes
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| kv.value.to_string())
    };
    let turn_session = attr(turn, "session_id");
    assert_eq!(turn_session.as_deref(), Some(session_id.as_str()));
    let step_model = attr(step, "model");
    assert_eq!(step_model.as_deref(), Some("test-model"));
    let step_provider = attr(step, "provider");
    assert_eq!(step_provider.as_deref(), Some("test"));
    assert_eq!(attr(prompt, "assembly_version").as_deref(), Some("5"));
    let blocks = attr(prompt, "piko.prompt.blocks").expect("ordered prompt block metadata");
    assert!(blocks.contains("platform.policy"));
    assert!(!blocks.contains("coding agent operating inside piko"));
    for sensitive in [
        "gen_ai.system_instructions",
        "gen_ai.input.messages",
        "gen_ai.tool.definitions",
    ] {
        assert!(
            attr(prompt, sensitive).is_none(),
            "prompt assembly metadata span must not contain {sensitive}"
        );
    }

    // ---- turn metrics ----
    meter_provider.force_flush().unwrap();
    let finished = metric_exporter
        .get_finished_metrics()
        .expect("finished metrics");
    let metric_names: Vec<&str> = finished
        .iter()
        .flat_map(|resource_metrics| resource_metrics.scope_metrics())
        .flat_map(|scope| scope.metrics())
        .map(|metric| metric.name())
        .collect();
    assert!(
        metric_names.contains(&"piko.turn.duration_ms"),
        "expected piko.turn.duration_ms in metrics: {metric_names:?}"
    );
    assert!(
        metric_names.contains(&"piko.model.step.duration_ms"),
        "expected piko.model.step.duration_ms in metrics: {metric_names:?}"
    );

    // ---- unified OTel logs: tracing events export as LogRecords ----
    logger_provider.force_flush().unwrap();
    let logs = log_exporter.get_emitted_logs().expect("emitted logs");
    assert!(
        !logs.is_empty(),
        "expected at least one OTel LogRecord from the turn"
    );
    assert!(
        logs.iter().any(|log| log.record.trace_context().is_some()),
        "expected a LogRecord correlated with a span (trace_context set)"
    );
    assert!(
        logs.iter().any(|log| {
            log.record
                .attributes_iter()
                .any(|(key, _)| key.as_str() == "run_id")
        }),
        "expected a LogRecord carrying run_id"
    );
}
