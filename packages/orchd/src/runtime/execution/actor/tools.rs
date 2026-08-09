use super::*;

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
        for group in tool_batch::group_tool_calls(tool_calls, routes) {
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
        Ok(())
    }
}
