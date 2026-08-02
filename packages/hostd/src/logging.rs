//! Global tracing initialization for hostd (and orchd via shared subscriber).
//!
//! Unified logging model: every `tracing` event is routed through the OTel
//! pipeline. With `[observability] enabled = true` the subscriber exports
//! spans to OTLP traces and all log records to OTLP logs — there is no
//! hand-rolled file logger anymore. When observability is disabled a plain
//! stderr console layer is installed so development still sees logs.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, MetricExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::logs::{SdkLogger, SdkLoggerProvider};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use thiserror::Error;
use tracing_subscriber::{
    EnvFilter, Registry, fmt,
    layer::{Layer, Layered, SubscriberExt},
    util::SubscriberInitExt,
};

use crate::domain::config::ObservabilitySettings;

pub const DEFAULT_FILTER: &str = "info,piko_hostd=info,piko_orchd=info";
pub const DEBUG_FILTER: &str = "debug,piko_hostd=debug,piko_orchd=debug";
pub const DEFAULT_OTEL_ENDPOINT: &str = "http://127.0.0.1:4318";
pub const DEFAULT_SERVICE_NAME: &str = "piko-hostd";

/// Resolved logging configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogConfig {
    pub filter: String,
    pub ansi: bool,
}

/// Holds the OTel providers; dropping flushes and shuts down exporters.
pub struct LogGuard {
    tracer_provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
    logger_provider: Option<SdkLoggerProvider>,
}

impl Drop for LogGuard {
    fn drop(&mut self) {
        if let Some(tracer_provider) = self.tracer_provider.take() {
            let _ = tracer_provider.shutdown();
        }
        if let Some(meter_provider) = self.meter_provider.take() {
            let _ = meter_provider.shutdown();
        }
        if let Some(logger_provider) = self.logger_provider.take() {
            let _ = logger_provider.shutdown();
        }
    }
}

#[derive(Debug, Error)]
pub enum LogError {
    #[error("logging already initialized")]
    AlreadyInitialized,
    #[error("invalid log filter: {0}")]
    InvalidFilter(String),
    #[error("failed to initialize tracing subscriber: {0}")]
    Init(String),
    #[error("failed to initialize OpenTelemetry exporter: {0}")]
    Otel(String),
}

/// CLI flags parsed from hostd binary arguments. Logging is unified on OTel,
/// so only the filter level is configurable from the CLI; a stderr console
/// fallback is used when observability is disabled.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HostdLogCli {
    pub log_level: Option<String>,
    pub log_stderr: bool,
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Parse hostd log-related CLI flags from an argument iterator.
pub fn parse_hostd_log_cli<I>(args: I) -> HostdLogCli
where
    I: IntoIterator<Item = String>,
{
    let mut cli = HostdLogCli::default();
    let mut args = args.into_iter().peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--log-level" => {
                if let Some(value) = args.next() {
                    cli.log_level = Some(value);
                }
            }
            "--log-stderr" => {
                cli.log_stderr = true;
            }
            _ => {}
        }
    }

    cli
}

/// Resolve [`LogConfig`] from CLI flags and environment variables.
pub fn resolve_config(cli: &HostdLogCli) -> Result<LogConfig, LogError> {
    let filter = cli
        .log_level
        .clone()
        .or_else(|| std::env::var("PIKO_LOG_LEVEL").ok())
        .or_else(|| std::env::var("RUST_LOG").ok())
        .unwrap_or_else(|| DEFAULT_FILTER.to_string());

    Ok(LogConfig { filter, ansi: true })
}

type OtelLayer =
    tracing_opentelemetry::OpenTelemetryLayer<Registry, opentelemetry_sdk::trace::Tracer>;
type OtelLogsBridge = OpenTelemetryTracingBridge<SdkLoggerProvider, SdkLogger>;

/// Providers and layers built when observability is enabled.
#[derive(Default)]
struct OtelRuntime {
    layer: Option<OtelLayer>,
    logs_bridge: Option<OtelLogsBridge>,
    tracer_provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
    logger_provider: Option<SdkLoggerProvider>,
}

/// Initialize the global tracing subscriber once.
pub fn init(
    config: LogConfig,
    observability: Option<&ObservabilitySettings>,
) -> Result<LogGuard, LogError> {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return Err(LogError::AlreadyInitialized);
    }

    let env_filter = EnvFilter::try_new(&config.filter)
        .map_err(|err| LogError::InvalidFilter(err.to_string()))?;

    let observability_enabled = observability
        .map(|obs| obs.enabled.unwrap_or(false))
        .unwrap_or(false);
    crate::telemetry::init(observability_enabled);
    let otel = init_otel(observability)?;

    let console = fmt::layer().with_writer(io::stderr).with_ansi(config.ansi);
    if observability_enabled {
        install_layers(otel.layer, otel.logs_bridge, env_filter, console)?;
    } else {
        install_layers(None, None, env_filter, console)?;
    }

    Ok(LogGuard {
        tracer_provider: otel.tracer_provider,
        meter_provider: otel.meter_provider,
        logger_provider: otel.logger_provider,
    })
}

/// Compose the tracing subscriber. The OTel layers attach first (the trace
/// layer's span lookup target is `Registry`); the generic bridge/filter/console
/// layers adapt to whatever wraps them.
fn install_layers<F>(
    otel_layer: Option<OtelLayer>,
    logs_bridge: Option<OtelLogsBridge>,
    env_filter: EnvFilter,
    console: F,
) -> Result<(), LogError>
where
    F: Layer<
            Layered<
                EnvFilter,
                Layered<Option<OtelLogsBridge>, Layered<Option<OtelLayer>, Registry>>,
            >,
        > + Send
        + Sync
        + 'static,
{
    Registry::default()
        .with(otel_layer)
        .with(logs_bridge)
        .with(env_filter)
        .with(console)
        .try_init()
        .map_err(|err| LogError::Init(err.to_string()))
}

/// Build the `tracing-opentelemetry` layers and install the global OTel
/// providers when observability is enabled.
fn init_otel(observability: Option<&ObservabilitySettings>) -> Result<OtelRuntime, LogError> {
    let Some(obs) = observability else {
        return Ok(OtelRuntime::default());
    };
    if !obs.enabled.unwrap_or(false) {
        return Ok(OtelRuntime::default());
    }

    let endpoint = obs
        .otel_endpoint
        .clone()
        .unwrap_or_else(|| DEFAULT_OTEL_ENDPOINT.to_string());
    let service_name = obs
        .service_name
        .clone()
        .unwrap_or_else(|| DEFAULT_SERVICE_NAME.to_string());
    let resource = opentelemetry_sdk::Resource::builder()
        .with_service_name(service_name.clone())
        .build();

    let span_exporter = SpanExporter::builder()
        .with_http()
        .with_endpoint(&endpoint)
        .build()
        .map_err(|err| LogError::Otel(err.to_string()))?;
    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter)
        .with_resource(resource.clone())
        .build();
    let tracer = tracer_provider.tracer(service_name.clone());
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let metric_exporter = MetricExporter::builder()
        .with_http()
        .with_endpoint(&endpoint)
        .build()
        .map_err(|err| LogError::Otel(err.to_string()))?;
    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(metric_exporter)
        .with_resource(resource.clone())
        .build();

    let log_exporter = LogExporter::builder()
        .with_http()
        .with_endpoint(&endpoint)
        .build()
        .map_err(|err| LogError::Otel(err.to_string()))?;
    let logger_provider = SdkLoggerProvider::builder()
        .with_batch_exporter(log_exporter)
        .with_resource(resource)
        .build();
    let logs_bridge = OpenTelemetryTracingBridge::new(&logger_provider);

    global::set_tracer_provider(tracer_provider.clone());
    global::set_meter_provider(meter_provider.clone());

    Ok(OtelRuntime {
        layer: Some(layer),
        logs_bridge: Some(logs_bridge),
        tracer_provider: Some(tracer_provider),
        meter_provider: Some(meter_provider),
        logger_provider: Some(logger_provider),
    })
}
