use std::sync::Arc;

use crate::adapters::OrchAgentRunRunner;
use crate::domain::config::{HostSettings, ModelRegistry};
use crate::domain::sessions::SessionModelRef;
use crate::ports::AgentRunRunner;
use piko_llmd::auth::AuthStorage;
use piko_llmd::gateway::LlmGateway;

/// Build an OrchAgentRunRunner and return both the runner and the model executor (if available).
pub(crate) async fn build_orch_turn_runner(
    settings: &HostSettings,
) -> Result<
    (
        Arc<dyn AgentRunRunner>,
        Option<Arc<dyn LlmGateway>>,
        Option<SessionModelRef>,
    ),
    String,
> {
    let mut auth = AuthStorage::create(None).map_err(|error| error.to_string())?;
    let registry = ModelRegistry::new(auth.clone(), vec![]);
    let resolved = registry
        .resolve(
            settings.default_model.as_deref(),
            settings.default_provider.as_deref(),
        )
        .ok_or_else(|| "no model available for hostd".to_string())?;

    let provider = &resolved.provider;
    let api_key = auth
        .resolve_oauth_api_key(provider)
        .await
        .map_err(|e| format!("failed to resolve auth for provider {provider}: {e}"))?
        .or_else(|| auth.get_api_key(provider))
        .ok_or_else(|| format!("no auth configured for provider {provider}"))?;

    let api_key_for_runner = api_key.clone();
    let mut providers = std::collections::HashMap::new();
    providers.insert(
        resolved.provider.clone(),
        piko_protocol::config::ProviderConfig {
            kind: resolved.provider.clone(),
            api_key,
            base_url: resolved.provider_config.base_url.clone(),
            headers: resolved.provider_config.headers.clone(),
            streaming_fallback: None,
        },
    );
    let retry_config = piko_protocol::config::RetryConfig {
        enabled: settings
            .retry
            .as_ref()
            .and_then(|r| r.enabled)
            .unwrap_or(true),
        max_retries: settings
            .retry
            .as_ref()
            .and_then(|r| r.max_retries)
            .unwrap_or(3),
        base_delay_ms: settings
            .retry
            .as_ref()
            .and_then(|r| r.base_delay_ms)
            .unwrap_or(2000),
        max_delay_ms: settings
            .retry
            .as_ref()
            .and_then(|r| r.max_delay_ms)
            .unwrap_or(30_000),
        budget_ms: settings
            .retry
            .as_ref()
            .and_then(|r| r.budget_ms)
            .unwrap_or(60_000),
    };
    let executor = piko_llmd::build_gateway_with_telemetry(
        providers,
        retry_config,
        crate::telemetry::handle(),
    );
    let thinking = settings.default_thinking_level.clone();
    let thinking_map = resolved.model.thinking_level_map.clone();
    let runner = Arc::new(
        OrchAgentRunRunner::new_with_mcp(
            executor.clone(),
            &resolved.provider,
            &api_key_for_runner,
            &resolved.model.id,
            thinking,
            thinking_map,
            resolved.model.context_window,
            resolved.model.max_tokens,
            &settings.mcp_servers,
            settings.mcp.as_ref(),
            settings.sandbox.as_ref(),
            settings.approvals.as_ref(),
            settings.guardian.as_ref(),
            settings.safety.as_ref(),
            settings.permissions.as_ref(),
            settings.features.as_ref(),
            settings.transcript.as_ref(),
            crate::telemetry::handle(),
        )
        .await,
    );
    let active_model = Some(SessionModelRef::new(
        resolved.provider.clone(),
        resolved.model.id.clone(),
    ));
    Ok((runner, Some(executor), active_model))
}
