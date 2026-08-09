use std::sync::Arc;

use crate::adapters::OrchAgentRunRunner;
use crate::domain::config::{HostSettings, ModelRegistry};
use crate::domain::sessions::SessionModelRef;
use crate::ports::AgentRunRunner;
use piko_llmd::auth::AuthStorage;
use piko_llmd::gateway::InferenceGateway;

/// Build an OrchAgentRunRunner and return both the runner and the model executor (if available).
pub(crate) async fn build_orch_turn_runner(
    settings: &HostSettings,
) -> Result<
    (
        Arc<dyn AgentRunRunner>,
        Option<Arc<dyn InferenceGateway>>,
        Option<SessionModelRef>,
    ),
    String,
> {
    let auth = AuthStorage::create(None).map_err(|error| error.to_string())?;
    let registry = ModelRegistry::new(auth.clone(), vec![]);
    let resolved = registry
        .resolve(
            settings.default_model.as_deref(),
            settings.default_provider.as_deref(),
        )
        .ok_or_else(|| "no model available for hostd".to_string())?;

    let provider = &resolved.provider;
    let oauth_flow = registry.get_oauth(provider);
    if !auth.has_auth(provider) {
        return Err(format!("no auth configured for provider {provider}"));
    }
    let target = resolved
        .target
        .as_ref()
        .ok_or_else(|| format!("no compatible target configured for provider {provider}"))?;
    let lookup_id =
        piko_llmd::modeling::ModelKey::new(&resolved.provider, &resolved.model.id).lookup_id();
    let mut targets = std::collections::HashMap::new();
    targets.insert(
        lookup_id,
        piko_llmd::target::ModelTargetConfig {
            target_id: target.id.clone(),
            api_surface: target.api_surface.clone(),
            auth_method: target.auth_method,
            protocol: target.protocol,
            capabilities: Some(piko_llmd::target::ModelCapabilities {
                text: resolved
                    .model
                    .input
                    .contains(&piko_protocol::model::InputModality::Text),
                images: resolved
                    .model
                    .input
                    .contains(&piko_protocol::model::InputModality::Image),
                tools: resolved
                    .model
                    .tool_execution_loci
                    .contains(&piko_protocol::model::ToolExecutionLocus::Caller),
                reasoning: resolved.model.reasoning,
                reasoning_efforts: resolved.model.reasoning_efforts.iter().cloned().collect(),
                refusals: true,
                upstream_tools: Default::default(),
                hybrid_tools: resolved
                    .model
                    .tool_execution_loci
                    .contains(&piko_protocol::model::ToolExecutionLocus::Hybrid),
                parallel_tools: resolved.model.parallel_tool_calls,
                structured_json_schema: resolved.model.structured_output,
                streaming_delivery: resolved
                    .model
                    .delivery_modes
                    .contains(&piko_protocol::model::InferenceDeliveryMode::Streaming),
                assembled_delivery: resolved
                    .model
                    .delivery_modes
                    .contains(&piko_protocol::model::InferenceDeliveryMode::Assembled),
                max_output_tokens: Some(resolved.model.max_tokens.min(u32::MAX as u64) as u32),
                ..Default::default()
            }),
            base_url: Some(target.base_url.clone()),
            endpoint: None,
            headers: None,
            streaming_fallback: true,
            reasoning_effort_map: target.reasoning_effort_map.clone(),
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
    let mut oauth_flows = std::collections::HashMap::new();
    if target.auth_method == piko_protocol::model::ProviderAuthMethod::OAuth {
        let flow = oauth_flow
            .ok_or_else(|| format!("no OAuth implementation registered for provider {provider}"))?;
        oauth_flows.insert(provider.clone(), flow);
    }
    let auth_resolver = std::sync::Arc::new(piko_llmd::providers::StoredAuthResolver::new(
        auth,
        oauth_flows,
    ));
    let executor = piko_llmd::build_gateway_with_auth(
        targets,
        retry_config,
        crate::telemetry::handle(),
        auth_resolver,
    );
    let thinking = settings.default_thinking_level.clone();
    let runner = Arc::new(
        OrchAgentRunRunner::new_with_mcp(
            executor.clone(),
            &resolved.provider,
            &resolved.model.id,
            thinking,
            resolved.model.context_window,
            resolved.model.max_tokens,
            &settings.mcp_servers,
            settings.mcp.as_ref(),
            settings.execution.as_ref(),
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
