use super::*;
use piko_protocol::{
    TrajectoryChildRunRecord, TrajectoryIdentity, TrajectoryNotificationKind, TrajectoryRecord,
    TrajectorySystemNotificationRecord, TrajectoryToolCallRecord, TrajectoryToolCallStatus,
};

impl ExecutionActor {
    pub(super) async fn execute_and_commit_tools(
        &mut self,
        tool_calls: &[ToolCallItem],
        routes: &HashMap<String, CatalogRoute>,
        parent_message_id: &str,
        context_remaining: Option<u64>,
    ) -> Result<(), AgentApiError> {
        // Tool calls belong to the assistant message that produced the whole
        // step, regardless of how their executions are scheduled. Commit every
        // declaration first so the durable transcript retains the provider
        // message shape: Assistant -> all ToolCalls -> all ToolResults.
        for tc in tool_calls {
            self.commit_message(
                tool_batch::tool_call_message(tc),
                tool_batch::tool_call_message_id(parent_message_id, tc.tool_call_index),
            )
            .await?;
        }

        // Batch dispatch groups consecutive calls by their effective execution
        // mode (F-06 / D-06): parallel calls in a group overlap under a shared
        // cap, sequential calls run exclusively, and results commit in
        // tool_call_index order so the append-only transcript stays
        // deterministic per run.
        let registry = self.services.tool_registry().clone();
        let model_step_index = self.state.model_step_index;
        let mut fresh_window_requested = false;
        let mut tool_started_at: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        for group in tool_batch::group_tool_calls(tool_calls, routes) {
            for tc in &group.calls {
                let started_at = now_ms();
                tool_started_at.insert(tc.id.clone(), started_at);
                self.record_trajectory_tool(TrajectoryRecord::ToolCall(TrajectoryToolCallRecord {
                    identity: self.trajectory_identity(),
                    call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    arguments: Some(tc.arguments.clone()),
                    status: TrajectoryToolCallStatus::Started,
                    started_at,
                    finished_at: None,
                    duration_ms: None,
                    result: None,
                    error: None,
                    message_id: None,
                }))
                .await;
            }
            let batch_span = tracing::info_span!(
                "tool.batch",
                session_id = %self.identity.session_id,
                run_id = %self.identity.execution_id,
                agent_instance_id = %self.identity.agent_instance_id,
                step_id = format!("step_{model_step_index}"),
                mode = tool_batch::mode_str(&group.mode),
                call_count = group.calls.len(),
                concurrency_cap = tracing::field::Empty,
                tool_names = group
                    .calls
                    .iter()
                    .map(|tc| tc.name.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            );
            let telemetry = self.services.telemetry();
            let batch_result: Result<(), AgentApiError> = async {
                match group.mode {
                    ToolExecutionMode::Sequential => {
                        for tc in group.calls {
                            let record = if self.cancel.is_cancelled() {
                                Some(tool_batch::aborted_tool_exec_result())
                            } else {
                                match routes.get(&tc.name) {
                                    Some(route) => Some(
                                        tool_batch::execute_sequential_call(
                                            registry.clone(),
                                            self.cancel.clone(),
                                            model_step_index,
                                            &self.identity,
                                            tc,
                                            route,
                                            parent_message_id,
                                            context_remaining,
                                            Arc::clone(&telemetry),
                                        )
                                        .await,
                                    ),
                                    None => {
                                        Some(tool_batch::no_route_error(&registry, &tc.name).await)
                                    }
                                }
                            };
                            let result_message = match record {
                                Some(ref record) => build_tool_result(tc, record),
                                None => build_tool_error(
                                    tc,
                                    &format!("No route for tool \"{}\"", tc.name),
                                ),
                            };
                            if tc.name == "new_context_window"
                                && record.as_ref().is_some_and(|result| result.ok)
                            {
                                fresh_window_requested = true;
                            }
                            self.commit_message(
                                result_message,
                                tool_batch::tool_result_message_id(
                                    parent_message_id,
                                    tc.tool_call_index,
                                ),
                            )
                            .await?;
                            if let Some(record) = record {
                                self.record_finished_tool(
                                    tc,
                                    &record,
                                    tool_started_at.remove(&tc.id).unwrap_or_default(),
                                    parent_message_id,
                                )
                                .await;
                            }
                        }
                    }
                    ToolExecutionMode::Parallel => {
                        let results = tool_batch::execute_parallel_group(
                            registry.clone(),
                            self.cancel.clone(),
                            model_step_index,
                            &self.identity,
                            &group.calls,
                            routes,
                            parent_message_id,
                            context_remaining,
                            Arc::clone(&telemetry),
                        )
                        .await;
                        for (tc, result) in group.calls.iter().zip(results) {
                            if tc.name == "new_context_window" && result.ok {
                                fresh_window_requested = true;
                            }
                            self.commit_message(
                                build_tool_result(tc, &result),
                                tool_batch::tool_result_message_id(
                                    parent_message_id,
                                    tc.tool_call_index,
                                ),
                            )
                            .await?;
                            self.record_finished_tool(
                                tc,
                                &result,
                                tool_started_at.remove(&tc.id).unwrap_or_default(),
                                parent_message_id,
                            )
                            .await;
                        }
                    }
                }
                Ok(())
            }
            .instrument(batch_span)
            .await;
            batch_result?;
        }
        if fresh_window_requested {
            // The model asked for a fresh window: the durable hostd tree was
            // rewritten through the callback; keep the running execution
            // aligned by dropping everything before the latest user message.
            self.state.transcript.reset_to_recent_user();
        }
        Ok(())
    }

    fn trajectory_identity(&self) -> TrajectoryIdentity {
        TrajectoryIdentity {
            session_id: self.identity.session_id.clone(),
            agent_instance_id: self.identity.agent_instance_id.clone(),
            run_id: self.identity.execution_id.clone(),
            execution_id: None,
            source_turn_id: self.identity.source_turn_id.clone(),
        }
    }

    async fn record_trajectory_tool(&self, record: TrajectoryRecord) {
        if let Some(port) = self.ports.ports().trajectory.clone() {
            port.record(record).await;
        }
    }

    async fn record_finished_tool(
        &self,
        tc: &ToolCallItem,
        result: &piko_orchd_api::ToolExecResult,
        started_at: i64,
        parent_message_id: &str,
    ) {
        let finished_at = now_ms();
        let status = if self.cancel.is_cancelled() {
            TrajectoryToolCallStatus::Cancelled
        } else if result.ok {
            TrajectoryToolCallStatus::Completed
        } else {
            TrajectoryToolCallStatus::Failed
        };
        self.record_trajectory_tool(TrajectoryRecord::ToolCall(TrajectoryToolCallRecord {
            identity: self.trajectory_identity(),
            call_id: tc.id.clone(),
            tool_name: tc.name.clone(),
            arguments: None,
            status,
            started_at,
            finished_at: Some(finished_at),
            duration_ms: Some(finished_at.saturating_sub(started_at) as u64),
            result: result.value.clone(),
            error: result.error.as_ref().map(|error| error.message.clone()),
            message_id: Some(tool_batch::tool_result_message_id(
                parent_message_id,
                tc.tool_call_index,
            )),
        }))
        .await;
        if matches!(tc.name.as_str(), "spawn_agent" | "spawn_agent_detached")
            && let Some(child_id) = result
                .value
                .as_ref()
                .and_then(|value| value.get("agent_instance_id"))
                .and_then(serde_json::Value::as_str)
        {
            self.record_trajectory_tool(TrajectoryRecord::ChildRun(TrajectoryChildRunRecord {
                identity: self.trajectory_identity(),
                child_agent_instance_id: child_id.into(),
                child_run_id: None,
                spawned_at: started_at,
                completed_at: None,
            }))
            .await;
        }
    }

    pub(super) fn resolve_fallback_model(
        &self,
        agent: &piko_protocol::agents::AgentSpec,
    ) -> ModelSpec {
        ModelSpec {
            id: self
                .request
                .config
                .model
                .clone()
                .or_else(|| agent.model.clone())
                .unwrap_or_else(|| "default".into()),
            name: "default".into(),
            provider: self
                .request
                .config
                .provider
                .clone()
                .unwrap_or_else(|| "default".into()),
        }
    }

    #[cfg(test)]
    pub(crate) fn transcript_messages(&self) -> Vec<Message> {
        self.state.transcript.to_vec()
    }

    pub(super) async fn commit_message(
        &mut self,
        message: Message,
        message_id: String,
    ) -> Result<(), AgentApiError> {
        if let Message::Assistant {
            usage: Some(usage), ..
        } = &message
        {
            self.state.usage.accumulate(usage);
        }
        let committed = MessageCommitScope::new(
            &self.identity,
            self.state.head_message_id.clone(),
            message_id,
            message,
        )
        .commit(&self.ports.ports().commit)
        .await?;
        committed.apply(&mut self.state);
        Ok(())
    }

    pub(super) async fn commit_steering(
        &mut self,
        steering: &SteerExecutionRequest,
    ) -> Result<(), AgentApiError> {
        let message = Message::User {
            content: steering.content.clone(),
            timestamp: Some(steering.submitted_at),
        };
        self.commit_message(message, steering.message_id.clone())
            .await?;
        let steer_summary = match &steering.content {
            piko_protocol::MessageContent::String(text) => text.clone(),
            piko_protocol::MessageContent::Blocks(blocks) => blocks
                .iter()
                .map(|block| format!("{block:?}"))
                .collect::<Vec<_>>()
                .join("\n"),
        };
        self.record_trajectory_tool(TrajectoryRecord::SystemNotification(
            TrajectorySystemNotificationRecord {
                identity: self.trajectory_identity(),
                kind: TrajectoryNotificationKind::SteerDelivered,
                summary: steer_summary.chars().take(512).collect(),
                recorded_at: now_ms(),
            },
        ))
        .await;
        // The next model step must answer this message before further tool
        // work (F-35 / ADR-021).
        self.state.respond_after_steer = true;
        Ok(())
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
