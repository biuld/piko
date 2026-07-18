//! Thin LLM tool adapter for the mandatory AgentRuntime control surface.

use std::sync::Arc;

use async_trait::async_trait;
use piko_orchd_api::{
    AgentApiError, AgentRuntimeApi, ToolDiscoveryContext, ToolExecError, ToolExecResult,
    ToolExecutionContext, ToolProvider,
};
use piko_protocol::messages::ToolCall;
use piko_protocol::tools::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutionMode, ToolExecutorRef,
    ToolProviderSource,
};
use piko_protocol::{
    AgentInputDelivery, AgentLifecycleRequest, CreateAgentRequest, MessageContent,
    SendAgentInputRequest,
};

#[derive(Clone)]
pub struct MultiAgentToolProvider {
    runtime: Arc<dyn AgentRuntimeApi>,
}

impl MultiAgentToolProvider {
    pub fn new(runtime: Arc<dyn AgentRuntimeApi>) -> Self {
        Self { runtime }
    }

    fn tools() -> Vec<ToolDef> {
        vec![
            tool(
                "spawn_agent",
                "Create a child AgentInstance and wait for its first execution report.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_spec_id": { "type": "string" },
                        "prompt": { "type": "string" }
                    },
                    "required": ["agent_spec_id", "prompt"]
                }),
            ),
            tool(
                "spawn_agent_detached",
                "Create a child AgentInstance that continues independently and reports to the caller inbox.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_spec_id": { "type": "string" },
                        "prompt": { "type": "string" }
                    },
                    "required": ["agent_spec_id", "prompt"]
                }),
            ),
            tool(
                "send_agent_message",
                "Send input to an existing AgentInstance, reusing its private transcript.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_instance_id": { "type": "string" },
                        "message": { "type": "string" },
                        "delivery": {
                            "type": "string",
                            "enum": ["auto", "steer", "follow_up"],
                            "default": "auto"
                        }
                    },
                    "required": ["agent_instance_id", "message"]
                }),
            ),
            tool(
                "get_agent_status",
                "Read the lifecycle and current activity of an AgentInstance.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_instance_id": { "type": "string" }
                    },
                    "required": ["agent_instance_id"]
                }),
            ),
            tool(
                "collect_agent_reports",
                "Collect and durably consume unread detached reports for the calling AgentInstance.",
                serde_json::json!({ "type": "object", "properties": {} }),
            ),
            tool(
                "close_agent",
                "Close an existing direct child AgentInstance to new input.",
                agent_target_schema(),
            ),
            tool(
                "reopen_agent",
                "Reopen an existing direct child AgentInstance.",
                agent_target_schema(),
            ),
        ]
    }

    async fn spawn(
        &self,
        call: &ToolCall,
        context: &ToolExecutionContext,
        detached: bool,
    ) -> Result<serde_json::Value, AgentApiError> {
        let agent_spec_id = required_string(&call.arguments, "agent_spec_id")?;
        let prompt = required_string(&call.arguments, "prompt")?;
        let spawn_id = stable_runtime_id(&context.execution_id, &call.id);
        let child_id = format!("agent_{spawn_id}");
        let child = self
            .runtime
            .create_agent(CreateAgentRequest {
                request_id: format!("create:{}:{}", context.execution_id, call.id),
                session_id: context.session_id.clone(),
                parent_agent_instance_id: context.agent_instance_id.clone(),
                agent_spec_id,
                requested_agent_instance_id: Some(child_id),
                origin_tool_call_id: Some(call.id.clone()),
            })
            .await?;
        let input = SendAgentInputRequest {
            request_id: format!("input:{}:{}", context.execution_id, call.id),
            session_id: context.session_id.clone(),
            agent_instance_id: child.identity.agent_instance_id.clone(),
            caller_agent_instance_id: Some(context.agent_instance_id.clone()),
            // Child agent runs have no Interaction Turn binding.
            source_turn_id: None,
            message_id: format!("message:{}:{}", context.execution_id, call.id),
            content: MessageContent::String(prompt),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
        };

        if detached {
            self.runtime
                .send_agent_input_detached(input, context.agent_instance_id.clone())
                .await?;
            Ok(serde_json::json!({
                "agent_instance_id": child.identity.agent_instance_id,
                "status": "accepted"
            }))
        } else {
            let acceptance = if let Some(cancellation) = &context.cancellation {
                tokio::select! {
                    acceptance = self.runtime.run_agent(input) => acceptance?,
                    _ = cancellation.cancelled() => {
                        let _ = self.runtime.cancel_agent_run(
                            context.session_id.clone(),
                            child.identity.agent_instance_id.clone(),
                        ).await;
                        return Err(AgentApiError::Cancelled);
                    }
                }
            } else {
                self.runtime.run_agent(input).await?
            };
            let report = if let Some(cancellation) = &context.cancellation {
                tokio::select! {
                    report = acceptance.wait() => report?,
                    _ = cancellation.cancelled() => {
                        let _ = self.runtime.cancel_agent_run(
                            context.session_id.clone(),
                            child.identity.agent_instance_id.clone(),
                        ).await;
                        return Err(AgentApiError::Cancelled);
                    }
                }
            } else {
                acceptance.wait().await?
            };
            Ok(report_value(&report))
        }
    }
}

#[async_trait]
impl ToolProvider for MultiAgentToolProvider {
    fn id(&self) -> &str {
        "multi_agent"
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Orch
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        Self::tools()
    }

    async fn execute(&self, call: ToolCall, context: ToolExecutionContext) -> ToolExecResult {
        let result = match call.name.as_str() {
            "spawn_agent" => self.spawn(&call, &context, false).await,
            "spawn_agent_detached" => self.spawn(&call, &context, true).await,
            "send_agent_message" => {
                let target = required_string(&call.arguments, "agent_instance_id");
                let message = required_string(&call.arguments, "message");
                match (target, message) {
                    (Ok(target), Ok(message)) => {
                        let delivery = match call
                            .arguments
                            .get("delivery")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("auto")
                        {
                            "steer" => AgentInputDelivery::SteerActive,
                            "follow_up" => AgentInputDelivery::FollowUp,
                            _ => AgentInputDelivery::Auto,
                        };
                        self.runtime
                            .send_agent_input(SendAgentInputRequest {
                                request_id: format!("message:{}:{}", context.execution_id, call.id),
                                session_id: context.session_id.clone(),
                                agent_instance_id: target,
                                caller_agent_instance_id: Some(context.agent_instance_id.clone()),
                                // Steered/follow-up input to an existing agent has no
                                // Interaction Turn binding of its own.
                                source_turn_id: None,
                                message_id: format!("message:{}:{}", context.execution_id, call.id),
                                content: MessageContent::String(message),
                                delivery,
                                prompt_resources: None,
                                active_tool_names: None,
                            })
                            .await
                            .map(|receipt| serde_json::json!({
                                "agent_instance_id": receipt.agent_instance_id,
                                "disposition": receipt.disposition,
                            }))
                    }
                    (Err(error), _) | (_, Err(error)) => Err(error),
                }
            }
            "get_agent_status" => match required_string(&call.arguments, "agent_instance_id") {
                Ok(target) => self
                    .runtime
                    .agent_snapshot(context.session_id.clone(), target)
                    .await
                    .and_then(|snapshot| snapshot.ok_or(AgentApiError::AgentNotFound))
                    .map(|snapshot| serde_json::json!({
                        "agent_instance_id": snapshot.identity.agent_instance_id,
                        "agent_spec_id": snapshot.identity.agent_spec_id,
                        "parent_agent_instance_id": snapshot.identity.parent_agent_instance_id,
                        "lifecycle": snapshot.lifecycle,
                        "activity": match snapshot.activity {
                            piko_protocol::AgentActivity::Idle => "idle",
                            piko_protocol::AgentActivity::Running => "running",
                            piko_protocol::AgentActivity::WaitingForApproval => "waiting_for_approval",
                            piko_protocol::AgentActivity::Cancelling => "cancelling",
                        },
                        "unread_report_count": snapshot.unread_report_count,
                    })),
                Err(error) => Err(error),
            },
            "collect_agent_reports" => {
                match self
                    .runtime
                    .agent_inbox(
                        context.session_id.clone(),
                        context.agent_instance_id.clone(),
                    )
                    .await
                {
                    Ok(inbox) => {
                        let unread = inbox
                            .items
                            .into_iter()
                            .filter(|item| item.consumed_at.is_none())
                            .collect::<Vec<_>>();
                        let mut consumed = Vec::with_capacity(unread.len());
                        let mut failure = None;
                        for item in &unread {
                            match self
                                .runtime
                                .consume_agent_inbox_item(piko_protocol::ConsumeAgentInboxRequest {
                                    request_id: format!("consume:{}:{}", call.id, item.report_id),
                                    session_id: context.session_id.clone(),
                                    agent_instance_id: context.agent_instance_id.clone(),
                                    report_id: item.report_id.clone(),
                                    consumed_at: chrono::Utc::now().timestamp_millis(),
                                })
                                .await
                            {
                                Ok(_) => consumed.push(item.clone()),
                                Err(error) => {
                                    failure = Some(error);
                                    break;
                                }
                            }
                        }
                        if let Some(error) = failure {
                            Err(error)
                        } else {
                            Ok(serde_json::json!({
                                "reports": consumed
                                    .iter()
                                    .map(|item| serde_json::json!({
                                        "report_id": item.report_id,
                                        "source_agent_instance_id": item.source_agent_instance_id,
                                        "report": report_value(&item.report),
                                    }))
                                    .collect::<Vec<_>>()
                            }))
                        }
                    }
                    Err(error) => Err(error),
                }
            }
            "close_agent" | "reopen_agent" => {
                match required_string(&call.arguments, "agent_instance_id") {
                    Ok(target) => {
                        let request = AgentLifecycleRequest {
                            request_id: format!("lifecycle:{}:{}", context.execution_id, call.id),
                            session_id: context.session_id.clone(),
                            agent_instance_id: target,
                            caller_agent_instance_id: Some(context.agent_instance_id.clone()),
                        };
                        let receipt = if call.name == "close_agent" {
                            self.runtime.close_agent(request).await
                        } else {
                            self.runtime.reopen_agent(request).await
                        };
                        receipt.and_then(|receipt| {
                            serde_json::to_value(receipt).map_err(|error| {
                                AgentApiError::PersistenceFailed(error.to_string())
                            })
                        })
                    }
                    Err(error) => Err(error),
                }
            }
            _ => Err(AgentApiError::InputRejected),
        };

        match result {
            Ok(value) => ToolExecResult {
                ok: true,
                value: Some(value),
                error: None,
            },
            Err(error) => ToolExecResult {
                ok: false,
                value: None,
                error: Some(ToolExecError {
                    code: "agent_runtime_error".into(),
                    message: error.to_string(),
                    retryable: Some(matches!(
                        error,
                        AgentApiError::Overload | AgentApiError::RuntimeUnavailable
                    )),
                }),
            },
        }
    }
}

fn required_string(value: &serde_json::Value, name: &str) -> Result<String, AgentApiError> {
    value
        .get(name)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or(AgentApiError::InputRejected)
}

fn tool(name: &str, description: &str, input_schema: serde_json::Value) -> ToolDef {
    ToolDef {
        name: name.into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("built-in-tool", name),
        description: description.into(),
        input_schema,
        executor: ToolExecutorRef {
            kind: "orchestrator".into(),
            target: name.into(),
            extra: None,
        },
        execution_mode: Some(ToolExecutionMode::Sequential),
        exposure: None,
        capabilities: Some(vec![ToolCapability::Delegation]),
        approval: Some(ToolApprovalRequirement::Never),
        metadata: None,
    }
}

fn agent_target_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "agent_instance_id": { "type": "string" }
        },
        "required": ["agent_instance_id"]
    })
}

fn stable_runtime_id(execution_id: &str, tool_call_id: &str) -> String {
    piko_orchd_api::stable_internal_id("spawn", &[execution_id, tool_call_id])
}

fn report_value(report: &piko_protocol::AgentRunReport) -> serde_json::Value {
    serde_json::json!({
        "agent_instance_id": report.agent_instance_id,
        "outcome": report.outcome,
        "summary": report.summary,
        "usage": report.usage,
        "artifacts": report.artifacts,
    })
}
