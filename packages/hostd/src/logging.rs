//! Global tracing initialization for hostd (and orchd via shared subscriber).
//!
//! Unified logging model: every `tracing` event is routed through the OTel
//! pipeline. With `[observability] enabled = true` the subscriber exports
//! metrics and all log records to OTLP — there is no hand-rolled file logger
//! anymore. Spans are never exported (F-36: the durable trajectory is the
//! causal record); internal `tracing` spans remain for console correlation.
//! When observability is disabled a plain stderr console layer is installed
//! so development still sees logs.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

use opentelemetry::global;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, MetricExporter, WithExportConfig};
use opentelemetry_sdk::logs::{SdkLogger, SdkLoggerProvider};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use thiserror::Error;
use tracing_subscriber::{
    EnvFilter, Registry, fmt,
    layer::{Layer, Layered, SubscriberExt},
    util::SubscriberInitExt,
};

use crate::domain::config::ObservabilitySettings;

pub const DEFAULT_FILTER: &str = "info,piko_hostd=info,piko_orchd=info";
pub const DEBUG_FILTER: &str = "debug,piko_hostd=debug,piko_orchd=debug";
pub const DEFAULT_OTEL_ENDPOINT: &str = "http://localhost:4318";
pub const DEFAULT_SERVICE_NAME: &str = "piko-hostd";

/// Join an OTLP HTTP base URL with a signal path (`/v1/traces` etc.).
///
/// `opentelemetry-otlp` 0.31 treats a programmatically set endpoint as a final
/// URL and does **not** append signal paths. Posting to the base alone hits
/// Aspire's root (302) and redirects to invalid routes such as
/// `/structuredlogs` (404). Always provide per-signal absolute paths.
fn otlp_signal_endpoint(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = if path.starts_with('/') {
        path
    } else {
        return format!("{base}/{path}");
    };
    // If the operator already gave a full signal URL, keep it.
    if base.ends_with("/v1/traces") || base.ends_with("/v1/metrics") || base.ends_with("/v1/logs") {
        return base.to_string();
    }
    format!("{base}{path}")
}

/// Merge loopback hosts into `NO_PROXY` so local collectors skip Shadowrocket-style system proxies.
fn ensure_otlp_no_proxy() {
    const BYPASS: &str = "localhost,127.0.0.1,::1";
    let merged = match std::env::var("NO_PROXY").or_else(|_| std::env::var("no_proxy")) {
        Ok(existing) if !existing.is_empty() => {
            let lower = existing.to_ascii_lowercase();
            let mut parts: Vec<String> = existing
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            for host in BYPASS.split(',') {
                if !lower.split(',').any(|h| h.trim() == host) {
                    parts.push(host.to_string());
                }
            }
            parts.join(",")
        }
        _ => BYPASS.to_string(),
    };
    // SAFETY: process-local, before OTLP clients are constructed.
    unsafe {
        std::env::set_var("NO_PROXY", &merged);
        std::env::set_var("no_proxy", &merged);
    }
}

/// Resolved logging configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogConfig {
    pub filter: String,
    pub ansi: bool,
}

/// Holds the OTel providers (metrics + logs); dropping flushes and shuts down
/// exporters. Traces are intentionally not exported (F-36).
pub struct LogGuard {
    meter_provider: Option<SdkMeterProvider>,
    logger_provider: Option<SdkLoggerProvider>,
}

impl Drop for LogGuard {
    fn drop(&mut self) {
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

type OtelLogsBridge = OpenTelemetryTracingBridge<SdkLoggerProvider, SdkLogger>;

/// Providers and layers built when observability is enabled.
#[derive(Default)]
struct OtelRuntime {
    logs_bridge: Option<OtelLogsBridge>,
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

    // Build OTel providers first so the global meter is installed before
    // `telemetry::init` binds instruments (and so batch exporters start).
    let otel = init_otel(observability)?;
    crate::telemetry::init(observability_enabled);

    let console = fmt::layer().with_writer(io::stderr).with_ansi(config.ansi);
    if observability_enabled {
        install_layers(otel.logs_bridge, env_filter, console)?;
    } else {
        install_layers(None, env_filter, console)?;
    }

    Ok(LogGuard {
        meter_provider: otel.meter_provider,
        logger_provider: otel.logger_provider,
    })
}

/// Compose the tracing subscriber. The OTel logs bridge attaches first; the
/// generic filter/console layers adapt to whatever wraps them.
fn install_layers<F>(
    logs_bridge: Option<OtelLogsBridge>,
    env_filter: EnvFilter,
    console: F,
) -> Result<(), LogError>
where
    F: Layer<Layered<EnvFilter, Layered<Option<OtelLogsBridge>, Registry>>> + Send + Sync + 'static,
{
    Registry::default()
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

    ensure_otlp_no_proxy();

    let metrics_endpoint = otlp_signal_endpoint(&endpoint, "/v1/metrics");
    let logs_endpoint = otlp_signal_endpoint(&endpoint, "/v1/logs");

    let metric_exporter = MetricExporter::builder()
        .with_http()
        .with_endpoint(&metrics_endpoint)
        .build()
        .map_err(|err| LogError::Otel(err.to_string()))?;
    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(metric_exporter)
        .with_resource(resource.clone())
        .build();

    let log_exporter = LogExporter::builder()
        .with_http()
        .with_endpoint(&logs_endpoint)
        .build()
        .map_err(|err| LogError::Otel(err.to_string()))?;
    let logger_provider = SdkLoggerProvider::builder()
        .with_batch_exporter(log_exporter)
        .with_resource(resource)
        .build();
    let logs_bridge = OpenTelemetryTracingBridge::new(&logger_provider);

    global::set_meter_provider(meter_provider.clone());

    // stderr: export errors are mostly silent; surface enablement for ops.
    eprintln!(
        "[piko-hostd] OTLP export enabled service={service_name} base={endpoint} \
         metrics={metrics_endpoint} logs={logs_endpoint} (traces disabled)"
    );

    Ok(OtelRuntime {
        logs_bridge: Some(logs_bridge),
        meter_provider: Some(meter_provider),
        logger_provider: Some(logger_provider),
    })
}

#[cfg(test)]
mod tests {
    use super::otlp_signal_endpoint;

    #[test]
    fn appends_signal_paths_to_base() {
        assert_eq!(
            otlp_signal_endpoint("http://localhost:4318", "/v1/traces"),
            "http://localhost:4318/v1/traces"
        );
        assert_eq!(
            otlp_signal_endpoint("http://localhost:4318/", "/v1/logs"),
            "http://localhost:4318/v1/logs"
        );
    }

    #[test]
    fn keeps_full_signal_urls() {
        assert_eq!(
            otlp_signal_endpoint("http://localhost:4318/v1/traces", "/v1/logs"),
            "http://localhost:4318/v1/traces"
        );
    }
}
