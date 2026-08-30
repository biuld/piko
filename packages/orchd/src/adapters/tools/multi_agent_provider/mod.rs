//! Thin LLM tool adapter for the multi-agent control surface (F-10 + F-21).

mod resolve;

use std::sync::Arc;

use async_trait::async_trait;
use piko_orchd_api::{
    AgentRuntimeApi, ToolDiscoveryContext, ToolExecError, ToolExecResult, ToolExecutionContext,
    ToolProvider,
};
use piko_protocol::messages::ToolCall;
use piko_protocol::tools::{ToolDef, ToolProviderSource};
use piko_protocol::{
    AgentActivity, AgentInputDelivery, AgentLifecycleRequest, CreateAgentRequest,
    MailboxWaitRequest, MessageContent, SendAgentInputRequest,
};

use resolve::{
    MessageWhen, ToolFail, activity_str, catalog_value, map_spawn_agent_error, multi_agent_tools,
    report_value, required_string, resolve_spawn_spec_id, resolve_when, stable_runtime_id,
    tool_disposition,
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
        multi_agent_tools()
    }

    async fn list_agent_specs_value(&self) -> Result<serde_json::Value, ToolFail> {
        let specs = self
            .runtime
            .list_agent_specs()
            .await
            .map_err(ToolFail::from_agent)?;
        Ok(catalog_value(&specs))
    }

    async fn spawn(
        &self,
        call: &ToolCall,
        context: &ToolExecutionContext,
        detached: bool,
    ) -> Result<serde_json::Value, ToolFail> {
        let prompt = required_string(&call.arguments, "prompt").map_err(ToolFail::from_agent)?;
        let specs = self
            .runtime
            .list_agent_specs()
            .await
            .map_err(ToolFail::from_agent)?;
        let agent_spec_id = resolve_spawn_spec_id(&call.arguments, &specs)?;
        let spawn_id = stable_runtime_id(&context.root_input_id, &call.id);
        let child_id = format!("agent_{spawn_id}");
        let child = self
            .runtime
            .create_agent(CreateAgentRequest {
                request_id: format!("create:{}:{}", context.root_input_id, call.id),
                session_id: context.session_id.clone(),
                parent_agent_instance_id: context.agent_instance_id.clone(),
                agent_spec_id: agent_spec_id.clone(),
                requested_agent_instance_id: Some(child_id),
                origin_tool_call_id: Some(call.id.clone()),
            })
            .await
            .map_err(|error| map_spawn_agent_error(error, &specs))?;
        let input = SendAgentInputRequest {
            request_id: format!("input:{}:{}", context.root_input_id, call.id),
            session_id: context.session_id.clone(),
            agent_instance_id: child.identity.agent_instance_id.clone(),
            caller_agent_instance_id: Some(context.agent_instance_id.clone()),
            root_input_id: None,
            message_id: format!("message:{}:{}", context.root_input_id, call.id),
            content: MessageContent::String(prompt),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
        };

        if detached {
            let canonical =
                piko_protocol::AgentInput::from_request(&input, crate::runtime::utils::now_ms());
            self.runtime
                .submit_agent_input_detached(canonical, context.agent_instance_id.clone())
                .await
                .map_err(ToolFail::from_agent)?;
            Ok(serde_json::json!({
                "agent_instance_id": child.identity.agent_instance_id,
                "agent_spec_id": agent_spec_id,
                "attached": false,
                "status": "accepted"
            }))
        } else {
            let canonical =
                piko_protocol::AgentInput::from_request(&input, crate::runtime::utils::now_ms());
            let input_id = canonical.input_id.clone();
            if let Some(cancellation) = &context.cancellation {
                tokio::select! {
                    receipt = self.runtime.submit_agent_input(canonical) => {
                        receipt.map_err(ToolFail::from_agent)?;
                    },
                    _ = cancellation.cancelled() => {
                        let _ = self.runtime.cancel_agent_run(
                            context.session_id.clone(),
                            child.identity.agent_instance_id.clone(),
                        ).await;
                        return Err(ToolFail::from_agent(piko_orchd_api::AgentApiError::Cancelled));
                    }
                }
            } else {
                self.runtime
                    .submit_agent_input(canonical)
                    .await
                    .map_err(ToolFail::from_agent)?;
            }
            let completion = self.runtime.wait_agent_input_completion(
                context.session_id.clone(),
                child.identity.agent_instance_id.clone(),
                input_id,
            );
            let report = if let Some(cancellation) = &context.cancellation {
                tokio::select! {
                    report = completion => report.map_err(ToolFail::from_agent)?,
                    _ = cancellation.cancelled() => {
                        let _ = self.runtime.cancel_agent_run(
                            context.session_id.clone(),
                            child.identity.agent_instance_id.clone(),
                        ).await;
                        return Err(ToolFail::from_agent(piko_orchd_api::AgentApiError::Cancelled));
                    }
                }
            } else {
                completion.await.map_err(ToolFail::from_agent)?
            };
            let mut value = report_value(&report);
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "agent_spec_id".into(),
                    serde_json::Value::String(agent_spec_id),
                );
                object.insert("attached".into(), serde_json::Value::Bool(true));
            }
            Ok(value)
        }
    }

    async fn message_agent(
        &self,
        call: &ToolCall,
        context: &ToolExecutionContext,
    ) -> Result<serde_json::Value, ToolFail> {
        let target =
            required_string(&call.arguments, "agent_instance_id").map_err(ToolFail::from_agent)?;
        let message = required_string(&call.arguments, "message").map_err(ToolFail::from_agent)?;
        let when = resolve_when(&call.arguments)?;

        if when == MessageWhen::Steer {
            let snapshot = self
                .runtime
                .agent_snapshot(context.session_id.clone(), target.clone())
                .await
                .map_err(ToolFail::from_agent)?
                .ok_or_else(|| {
                    ToolFail::from_agent(piko_orchd_api::AgentApiError::AgentNotFound)
                })?;
            if !matches!(snapshot.activity, AgentActivity::Running) {
                return Err(ToolFail {
                    code: "agent_not_running".into(),
                    message: format!(
                        "Agent instance \"{target}\" is not running (activity={}). Use when=queue to start or queue a task.",
                        activity_str(&snapshot.activity)
                    ),
                    retryable: false,
                });
            }
        }

        let delivery = match when {
            MessageWhen::Queue => AgentInputDelivery::FollowUp,
            MessageWhen::Steer => AgentInputDelivery::SteerActive,
        };
        let request_id = format!("message:{}:{}", context.root_input_id, call.id);
        let receipt = self
            .runtime
            .submit_agent_input(piko_protocol::AgentInput {
                input_id: stable_runtime_id(&context.root_input_id, &call.id),
                request_id,
                session_id: context.session_id.clone(),
                agent_instance_id: target,
                origin: piko_protocol::AgentInputOrigin::Agent,
                caller_agent_instance_id: Some(context.agent_instance_id.clone()),
                content: MessageContent::String(message),
                delivery,
                submitted_at: crate::runtime::utils::now_ms(),
                detached_recipient_agent_instance_id: None,
            })
            .await
            .map_err(ToolFail::from_agent)?;
        Ok(serde_json::json!({
            "agent_instance_id": receipt.agent_instance_id,
            "when": when.as_str(),
            "disposition": tool_disposition(receipt.disposition),
        }))
    }

    async fn interrupt_agent(
        &self,
        call: &ToolCall,
        context: &ToolExecutionContext,
    ) -> Result<serde_json::Value, ToolFail> {
        let target =
            required_string(&call.arguments, "agent_instance_id").map_err(ToolFail::from_agent)?;
        let snapshot = self
            .runtime
            .agent_snapshot(context.session_id.clone(), target.clone())
            .await
            .map_err(ToolFail::from_agent)?
            .ok_or_else(|| ToolFail::from_agent(piko_orchd_api::AgentApiError::AgentNotFound))?;
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
            Err(piko_orchd_api::AgentApiError::InvalidState) => Ok(serde_json::json!({
                "agent_instance_id": target,
                "previous_activity": previous_activity,
                "accepted": false,
            })),
            Err(error) => Err(ToolFail::from_agent(error)),
        }
    }

    async fn wait_agent(
        &self,
        call: &ToolCall,
        context: &ToolExecutionContext,
    ) -> Result<serde_json::Value, ToolFail> {
        let timeout_ms = call
            .arguments
            .get("timeout_ms")
            .and_then(serde_json::Value::as_u64)
            .filter(|value| *value > 0)
            .ok_or_else(|| ToolFail::from_agent(piko_orchd_api::AgentApiError::InputRejected))?;
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
                summary = wait => summary.map_err(ToolFail::from_agent)?,
                _ = cancellation.cancelled() => return Err(ToolFail::from_agent(piko_orchd_api::AgentApiError::Cancelled)),
            }
        } else {
            wait.await.map_err(ToolFail::from_agent)?
        };
        serde_json::to_value(summary).map_err(|error| {
            ToolFail::from_agent(piko_orchd_api::AgentApiError::PersistenceFailed(
                error.to_string(),
            ))
        })
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
        let result = if context.agent_kind.can_spawn_subagents() {
            match call.name.as_str() {
            "list_agent_specs" => self.list_agent_specs_value().await,
            "spawn_agent" => self.spawn(&call, &context, false).await,
            "spawn_agent_detached" => self.spawn(&call, &context, true).await,
            "message_agent" => self.message_agent(&call, &context).await,
            "interrupt_agent" => self.interrupt_agent(&call, &context).await,
            "list_agents" => self
                .runtime
                .list_agents(context.session_id.clone())
                .await
                .map_err(ToolFail::from_agent)
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
                            Err(ToolFail::from_agent(error))
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
                    Err(error) => Err(ToolFail::from_agent(error)),
                }
            }
            "close_agent" | "reopen_agent" => {
                match required_string(&call.arguments, "agent_instance_id") {
                    Ok(target) => {
                        let request = AgentLifecycleRequest {
                            request_id: format!("lifecycle:{}:{}", context.root_input_id, call.id),
                            session_id: context.session_id.clone(),
                            agent_instance_id: target,
                            caller_agent_instance_id: Some(context.agent_instance_id.clone()),
                        };
                        let receipt = if call.name == "close_agent" {
                            self.runtime.close_agent(request).await
                        } else {
                            self.runtime.reopen_agent(request).await
                        };
                        receipt
                            .and_then(|receipt| {
                                serde_json::to_value(receipt).map_err(|error| {
                                    piko_orchd_api::AgentApiError::PersistenceFailed(
                                        error.to_string(),
                                    )
                                })
                            })
                            .map_err(ToolFail::from_agent)
                    }
                    Err(error) => Err(ToolFail::from_agent(error)),
                }
            }
            _ => Err(ToolFail::from_agent(
                piko_orchd_api::AgentApiError::InputRejected,
            )),
            }
        } else {
            Err(ToolFail::from_agent(
                piko_orchd_api::AgentApiError::AgentCannotSpawnChildren,
            ))
        };

        match result {
            Ok(value) => ToolExecResult {
                ok: true,
                value: Some(value),
                error: None,
            },
            Err(fail) => ToolExecResult {
                ok: false,
                value: None,
                error: Some(ToolExecError {
                    code: fail.code,
                    message: fail.message,
                    retryable: Some(fail.retryable),
                }),
            },
        }
    }
}

#[cfg(test)]
mod surface_tests {
    use super::*;

    #[test]
    fn discover_exposes_f21_surface() {
        let tools = MultiAgentToolProvider::tools();
        let names: std::collections::BTreeSet<_> =
            tools.iter().map(|tool| tool.name.as_str()).collect();
        assert!(names.contains("list_agent_specs"));
        assert!(names.contains("message_agent"));
        assert!(!names.contains("followup_task"));
        assert!(!names.contains("send_agent_message"));
        let spawn = tools
            .iter()
            .find(|tool| tool.name == "spawn_agent")
            .unwrap();
        let required = spawn.input_schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "prompt"));
        assert!(!required.iter().any(|v| v == "agent_spec_id"));
    }
}
