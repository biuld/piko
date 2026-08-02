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
    AgentActivity, AgentInputDelivery, AgentLifecycleRequest, CreateAgentRequest,
    MailboxWaitRequest, MessageContent, SendAgentInputRequest,
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
                "Send input to an existing AgentInstance, reusing its private transcript; starts a new turn when idle and steers the active turn while running.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_instance_id": { "type": "string" },
                        "message": { "type": "string" }
                    },
                    "required": ["agent_instance_id", "message"]
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
            tool(
                "followup_task",
                "Send a follow-up task to an existing agent; starts a new turn when idle and durably queues it while running.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_instance_id": { "type": "string" },
                        "message": { "type": "string" }
                    },
                    "required": ["agent_instance_id", "message"]
                }),
            ),
            tool(
                "interrupt_agent",
                "Interrupt an agent's current turn and report its previous activity; the agent stays available for follow-up.",
                agent_target_schema(),
            ),
            tool(
                "list_agents",
                "List live agents in the session, parents before children.",
                serde_json::json!({ "type": "object", "properties": {} }),
            ),
            tool(
                "wait_agent",
                "Wait (bounded by timeout_ms) for the next mailbox update from any live agent, optionally filtered to one agent.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "timeout_ms": { "type": "integer", "minimum": 1 },
                        "agent_instance_id": { "type": "string" }
                    },
                    "required": ["timeout_ms"]
                }),
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

    async fn interrupt_agent(
        &self,
        call: &ToolCall,
        context: &ToolExecutionContext,
    ) -> Result<serde_json::Value, AgentApiError> {
        let target = required_string(&call.arguments, "agent_instance_id")?;
        let snapshot = self
            .runtime
            .agent_snapshot(context.session_id.clone(), target.clone())
            .await
            .and_then(|snapshot| snapshot.ok_or(AgentApiError::AgentNotFound))?;
        let previous_activity = activity_str(&snapshot.activity);
        match self
            .runtime
            .cancel_agent_run(context.session_id.clone(), target.clone())
            .await
        {
            Ok(receipt) => Ok(serde_json::json!({
                "agent_instance_id": receipt.agent_instance_id,
                "previous_activity": previous_activity,
                "accepted": receipt.accepted,
            })),
            // An idle target has no run to cancel; that is a benign no-op,
            // not an LLM-visible failure.
            Err(AgentApiError::InvalidState) => Ok(serde_json::json!({
                "agent_instance_id": target,
                "previous_activity": previous_activity,
                "accepted": false,
            })),
            Err(error) => Err(error),
        }
    }

    async fn wait_agent(
        &self,
        call: &ToolCall,
        context: &ToolExecutionContext,
    ) -> Result<serde_json::Value, AgentApiError> {
        let timeout_ms = call
            .arguments
            .get("timeout_ms")
            .and_then(serde_json::Value::as_u64)
            .filter(|value| *value > 0)
            .ok_or(AgentApiError::InputRejected)?;
        let agent_instance_id = call
            .arguments
            .get("agent_instance_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let wait = self.runtime.wait_agent_mailbox(MailboxWaitRequest {
            session_id: context.session_id.clone(),
            caller_agent_instance_id: Some(context.agent_instance_id.clone()),
            timeout_ms,
            agent_instance_id,
        });
        let summary = if let Some(cancellation) = &context.cancellation {
            tokio::select! {
                summary = wait => summary?,
                _ = cancellation.cancelled() => return Err(AgentApiError::Cancelled),
            }
        } else {
            wait.await?
        };
        serde_json::to_value(summary)
            .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))
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
                        self.runtime
                            .send_agent_input(SendAgentInputRequest {
                                request_id: format!("message:{}:{}", context.execution_id, call.id),
                                session_id: context.session_id.clone(),
                                agent_instance_id: target,
                                caller_agent_instance_id: Some(context.agent_instance_id.clone()),
                                // Message input to an existing agent has no
                                // Interaction Turn binding of its own; follow-up
                                // semantics live on `followup_task`.
                                source_turn_id: None,
                                message_id: format!("message:{}:{}", context.execution_id, call.id),
                                content: MessageContent::String(message),
                                delivery: AgentInputDelivery::Auto,
                                prompt_resources: None,
                                active_tool_names: None,
                            })
                            .await
                            .map(|receipt| {
                                serde_json::json!({
                                    "agent_instance_id": receipt.agent_instance_id,
                                    "disposition": receipt.disposition,
                                })
                            })
                    }
                    (Err(error), _) | (_, Err(error)) => Err(error),
                }
            }
            "followup_task" => {
                let target = required_string(&call.arguments, "agent_instance_id");
                let message = required_string(&call.arguments, "message");
                match (target, message) {
                    (Ok(target), Ok(message)) => self
                        .runtime
                        .send_agent_input(SendAgentInputRequest {
                            request_id: format!("followup:{}:{}", context.execution_id, call.id),
                            session_id: context.session_id.clone(),
                            agent_instance_id: target,
                            caller_agent_instance_id: Some(context.agent_instance_id.clone()),
                            // A follow-up task is not an Interaction Turn of its
                            // own; it starts a run when idle and queues while busy.
                            source_turn_id: None,
                            message_id: format!("followup:{}:{}", context.execution_id, call.id),
                            content: MessageContent::String(message),
                            delivery: AgentInputDelivery::FollowUp,
                            prompt_resources: None,
                            active_tool_names: None,
                        })
                        .await
                        .map(|receipt| {
                            serde_json::json!({
                                "agent_instance_id": receipt.agent_instance_id,
                                "disposition": receipt.disposition,
                            })
                        }),
                    (Err(error), _) | (_, Err(error)) => Err(error),
                }
            }
            "interrupt_agent" => self.interrupt_agent(&call, &context).await,
            "list_agents" => self
                .runtime
                .list_agents(context.session_id.clone())
                .await
                .map(|snapshots| {
                    serde_json::json!({
                        "agents": snapshots.iter().map(|snapshot| serde_json::json!({
                            "agent_instance_id": snapshot.identity.agent_instance_id,
                            "agent_spec_id": snapshot.identity.agent_spec_id,
                            "parent_agent_instance_id": snapshot.identity.parent_agent_instance_id,
                            "lifecycle": snapshot.lifecycle,
                            "activity": activity_str(&snapshot.activity),
                            "unread_report_count": snapshot.unread_report_count,
                            "latest_report_summary": snapshot
                                .latest_report
                                .as_ref()
                                .map(|report| report.summary.clone()),
                        })).collect::<Vec<_>>()
                    })
                }),
            "wait_agent" => self.wait_agent(&call, &context).await,
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

fn activity_str(activity: &AgentActivity) -> &'static str {
    match activity {
        AgentActivity::Idle => "idle",
        AgentActivity::Running => "running",
        AgentActivity::WaitingForApproval => "waiting_for_approval",
        AgentActivity::Cancelling => "cancelling",
    }
}
