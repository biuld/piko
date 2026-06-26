// ---- RPC: handlers — method-to-Orchestrator mappings ----

use std::io::Write;
use std::sync::Arc;

use serde_json::Value;
use tracing::info;

use crate::orchestrator::core::OrchCore;
use crate::protocol::agents::{AgentSpec, AgentTask};
use crate::protocol::config::OrchdConfig;
use crate::protocol::runtime::OrchRunOptions;
use crate::protocol::tools::ToolSet;
use crate::rpc::approval::RpcApprovalGateway;
use crate::rpc::peer::RpcPeer;
use crate::tools::rpc_host_provider::RpcHostToolProvider;

/// Result type for handler dispatch.
type HandlerResult = Result<Value, String>;

/// Dispatch an RPC method call to the orchestrator.
pub async fn dispatch(
    orch: &Arc<OrchCore>,
    peer: Option<Arc<RpcPeer>>,
    method: &str,
    params: &Value,
) -> HandlerResult {
    match method {
        "configure" | "orch.configure" => handle_configure(orch, params).await,
        "register_agent" | "orch.register_agent" => handle_register_agent(orch, params).await,
        "unregister_agent" | "orch.unregister_agent" => handle_unregister_agent(orch, params).await,
        "register_tool_provider" | "orch.register_tool_provider" => {
            handle_register_tool_provider(orch, peer, params).await
        }
        "unregister_tool_provider" | "orch.unregister_tool_provider" => {
            handle_unregister_tool_provider(orch, params).await
        }
        "register_tool_set" | "orch.register_tool_set" => {
            handle_register_tool_set(orch, params).await
        }
        "unregister_tool_set" | "orch.unregister_tool_set" => {
            handle_unregister_tool_set(orch, params).await
        }
        "set_model_config" | "orch.set_model_config" => handle_set_model_config(orch, params).await,
        "spawn" | "spawn_detached" | "orch.start_task" => handle_spawn_detached(orch, params).await,
        "await_task" | "orch.await_task" => handle_await_task(orch, params).await,
        "run" => handle_run(orch, params).await,
        "cancel_task" | "orch.cancel_task" => handle_cancel_task(orch, params).await,
        "snapshot" | "orch.snapshot" => handle_snapshot(orch).await,
        "update_plan" | "orch.update_plan" => handle_update_plan(orch, params).await,
        "get_graph" | "orch.get_graph" => handle_get_graph(orch).await,
        "subscribe" | "orch.subscribe_events" => handle_subscribe(orch).await,
        "llm_call" | "orch.llm_call" => handle_llm_call(orch, params).await,
        _ => Err(format!("Method not found: {method}")),
    }
}

// ---- configure — receives OrchdConfig from Host ----

async fn handle_configure(orch: &Arc<OrchCore>, params: &Value) -> HandlerResult {
    let config: OrchdConfig =
        serde_json::from_value(params.clone()).map_err(|e| format!("Invalid params: {e}"))?;

    info!(
        providers = config.providers.len(),
        agents = config.agents.len(),
        "configure"
    );

    // Register agents from config
    for spec in config.agents.values() {
        orch.register_agent(spec.clone()).await;
    }

    // Build model config from default
    let model = crate::protocol::messages::Model {
        id: config.default_model.model_id.clone(),
        name: config.default_model.model_id.clone(),
        provider: config.default_model.provider.clone(),
        base_url: None,
    };
    let provider_config = config
        .providers
        .get(&config.default_model.provider)
        .map(|p| crate::protocol::model::ModelProviderConfig {
            api_key: Some(p.api_key.clone()),
            base_url: p.base_url.clone(),
            headers: Some(p.headers.clone().unwrap_or_default()),
            reasoning: None,
            session_id: None,
            extra: None,
        })
        .unwrap_or_default();

    let model_config = crate::protocol::runtime::OrchModelConfig {
        model,
        provider: provider_config,
        settings: config.default_settings.clone(),
    };
    orch.set_model_config(model_config).await;

    Ok(Value::Null)
}

// ---- Individual handlers ----

async fn handle_register_agent(orch: &Arc<OrchCore>, params: &Value) -> HandlerResult {
    let spec: AgentSpec =
        serde_json::from_value(params.clone()).map_err(|e| format!("Invalid params: {e}"))?;
    info!(agent_id = %spec.id, "register_agent");
    orch.register_agent(spec).await;
    Ok(Value::Null)
}

async fn handle_unregister_agent(orch: &Arc<OrchCore>, params: &Value) -> HandlerResult {
    let agent_id = params
        .get("agentId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Invalid params: missing agentId".to_string())?;
    info!(agent_id, "unregister_agent");
    orch.unregister_agent(agent_id).await;
    Ok(Value::Null)
}

async fn handle_register_tool_provider(
    orch: &Arc<OrchCore>,
    peer: Option<Arc<RpcPeer>>,
    params: &Value,
) -> HandlerResult {
    let provider_id = params
        .get("providerId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Invalid params: missing providerId".to_string())?;
    let source = params
        .get("source")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or(crate::protocol::tools::ToolProviderSource::Host);
    let peer = peer.ok_or_else(|| "Invalid params: RPC peer unavailable".to_string())?;
    info!(provider_id, "register_tool_provider");
    orch.register_provider(Box::new(RpcHostToolProvider::new(
        provider_id.to_string(),
        source,
        peer.clone(),
    )))
    .await;
    orch.tool_registry
        .set_approval_gateway(Some(Box::new(RpcApprovalGateway::new(peer))))
        .await;
    Ok(Value::Null)
}

async fn handle_unregister_tool_provider(orch: &Arc<OrchCore>, params: &Value) -> HandlerResult {
    let provider_id = params
        .get("providerId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Invalid params: missing providerId".to_string())?;
    info!(provider_id, "unregister_tool_provider");
    orch.unregister_provider(provider_id).await;
    Ok(Value::Null)
}

async fn handle_register_tool_set(orch: &Arc<OrchCore>, params: &Value) -> HandlerResult {
    let tool_set: ToolSet =
        serde_json::from_value(params.clone()).map_err(|e| format!("Invalid params: {e}"))?;
    info!(tool_set_id = %tool_set.id, "register_tool_set");
    orch.register_tool_set(tool_set).await;
    Ok(Value::Null)
}

async fn handle_unregister_tool_set(orch: &Arc<OrchCore>, params: &Value) -> HandlerResult {
    let tool_set_id = params
        .get("toolSetId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Invalid params: missing toolSetId".to_string())?;
    info!(tool_set_id, "unregister_tool_set");
    orch.unregister_tool_set(tool_set_id).await;
    Ok(Value::Null)
}

async fn handle_set_model_config(orch: &Arc<OrchCore>, params: &Value) -> HandlerResult {
    let config =
        serde_json::from_value(params.clone()).map_err(|e| format!("Invalid params: {e}"))?;
    info!("set_model_config");
    orch.set_model_config(config).await;
    Ok(Value::Null)
}


async fn handle_spawn_detached(orch: &Arc<OrchCore>, params: &Value) -> HandlerResult {
    let task: AgentTask =
        serde_json::from_value(params.clone()).map_err(|e| format!("Invalid params: {e}"))?;
    let target = task.target_agent_id.clone();
    info!(task_target = %target, "spawn_detached");
    let task_id = orch.spawn_detached(task).await;
    Ok(serde_json::json!({"taskId": task_id}))
}

async fn handle_await_task(orch: &Arc<OrchCore>, params: &Value) -> HandlerResult {
    let task_id = params
        .get("taskId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Invalid params: missing taskId".to_string())?;
    info!(task_id, "await_task");
    let result = orch.await_task(task_id).await;
    Ok(result.unwrap_or(Value::Null))
}

async fn handle_run(orch: &Arc<OrchCore>, params: &Value) -> HandlerResult {
    let prompt = params.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
    let opts: Option<OrchRunOptions> = params
        .get("options")
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    info!(prompt_len = prompt.len(), "run");
    let result = orch.run(prompt, opts).await;
    serde_json::to_value(&result).map_err(|e| format!("Failed to serialize result: {e}"))
}

async fn handle_cancel_task(orch: &Arc<OrchCore>, params: &Value) -> HandlerResult {
    let task_id = params
        .get("taskId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Invalid params: missing taskId".to_string())?;
    let reason = params.get("reason").and_then(|v| v.as_str());
    info!(task_id, reason, "cancel_task");
    orch.cancel_task(task_id, reason).await;
    Ok(Value::Null)
}

async fn handle_snapshot(orch: &Arc<OrchCore>) -> HandlerResult {
    info!("snapshot");
    let state = orch.snapshot().await;
    serde_json::to_value(&state).map_err(|e| format!("Failed to serialize state: {e}"))
}

async fn handle_update_plan(orch: &Arc<OrchCore>, params: &Value) -> HandlerResult {
    let agent_id = params
        .get("agentId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Invalid params: missing agentId".to_string())?;
    let task_id = params
        .get("taskId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Invalid params: missing taskId".to_string())?;
    let plan: Vec<Value> = params
        .get("plan")
        .and_then(|v| v.as_array())
        .map(|a| a.to_vec())
        .unwrap_or_default();
    info!(agent_id, task_id, "update_plan");
    orch.update_plan(agent_id, task_id, plan).await;
    Ok(Value::Null)
}

async fn handle_get_graph(orch: &Arc<OrchCore>) -> HandlerResult {
    info!("get_graph");
    let graph = orch.get_graph().await;
    serde_json::to_value(&graph).map_err(|e| format!("Failed to serialize graph: {e}"))
}

async fn handle_subscribe(orch: &Arc<OrchCore>) -> HandlerResult {
    use crate::protocol::events::HostEvent;

    // Create a listener that writes events to stdout as JSON-RPC notifications
    let listener: Box<dyn Fn(HostEvent) + Send + Sync> = Box::new(move |event| {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "host_event",
            "params": event,
        });
        let mut out = std::io::stdout().lock();
        let json = serde_json::to_string(&notification).unwrap_or_default();
        let _ = writeln!(out, "{json}");
        let _ = out.flush();
    });

    let _cleanup = orch.subscribe(listener).await;
    // Note: cleanup function is never called for stdio subscriptions.
    // It would be called when the orchestrator shuts down or on explicit unsubscribe.

    Ok(serde_json::json!({"subscribed": true}))
}

async fn handle_llm_call(orch: &Arc<OrchCore>, params: &Value) -> HandlerResult {
    let model: crate::protocol::messages::Model = params
        .get("model")
        .ok_or_else(|| "Invalid params: missing model".to_string())
        .and_then(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))?;

    let system_prompt: Option<String> = params
        .get("systemPrompt")
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    let messages: Vec<crate::protocol::messages::Message> = params
        .get("messages")
        .ok_or_else(|| "Invalid params: missing messages".to_string())
        .and_then(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))?;

    let settings: crate::protocol::model::ModelRunSettings = params
        .get("settings")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    info!(model_id = %model.id, provider = %model.provider, "llm_call");
    let result = orch
        .model_executor
        .llm_call(model, system_prompt, messages, settings)
        .await;
    match result {
        Ok(text) => Ok(serde_json::json!({ "text": text })),
        Err(e) => Err(e),
    }
}
