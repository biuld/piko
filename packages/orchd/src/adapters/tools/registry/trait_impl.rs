use super::*;
use crate::adapters::tools::features::feature_enabled;

#[async_trait]
impl ToolRegistry for ToolRegistryImpl {
    /// Discover tools: build catalog, apply filter, return (tools, routes).
    async fn discover_tools(
        &self,
        context: &ToolDiscoveryContext,
    ) -> Result<(Vec<ToolDef>, HashMap<String, CatalogRoute>), String> {
        let catalog = self.build_catalog(context).await?;

        let features = self.features.read().await;

        // Apply feature gating (F-18) then active tool name restrictions.
        // A tool passes when its feature is enabled (or ungated) and, when a
        // transient allow-list is present, its name is listed.
        let tools: Vec<ToolDef> = if let Some(ref active) = context.active_tool_names {
            catalog
                .iter()
                .filter(|e| {
                    tool_allowed_for_agent(e, context.agent_kind)
                        && feature_enabled(features.as_ref(), e.feature.as_deref())
                        && active.contains(&e.public_name)
                })
                .map(|e| e.tool_def.clone())
                .collect()
        } else {
            catalog
                .iter()
                .filter(|e| feature_enabled(features.as_ref(), e.feature.as_deref()))
                .filter(|e| tool_allowed_for_agent(e, context.agent_kind))
                .map(|e| e.tool_def.clone())
                .collect()
        };

        // Build route map for fast lookup during execution
        let mut routes = HashMap::new();
        for entry in &catalog {
            // If active filter active, only include filtered tools
            if let Some(ref active) = context.active_tool_names
                && !active.contains(&entry.public_name)
            {
                continue;
            }
            if !feature_enabled(features.as_ref(), entry.feature.as_deref()) {
                continue;
            }
            if !tool_allowed_for_agent(entry, context.agent_kind) {
                continue;
            }
            routes.insert(
                entry.public_name.clone(),
                CatalogRoute {
                    provider_id: entry.provider_id.clone(),
                    provider_tool_name: entry.provider_tool_name.clone(),
                    tool_def: entry.tool_def.clone(),
                    execution_mode: entry.execution_mode.clone(),
                    max_concurrent_calls: entry.max_concurrent_calls,
                },
            );
        }

        Ok((tools, routes))
    }

    /// Execute a tool call with approval checks.
    async fn execute_tool(
        &self,
        call: &ToolCall,
        context: &ToolExecutionContext,
        route: &CatalogRoute,
        cancel: Option<CancellationToken>,
    ) -> ToolExecutionRecord {
        let call_id = call.id.clone();
        let call_name = call.name.clone();
        let call_args = call.arguments.clone();

        // A route may outlive the catalog snapshot that produced it. Repeat
        // the capability check at execution time so a stale delegation route
        // cannot be used by a worker agent.
        if !tool_def_allowed_for_agent(&route.tool_def, context.agent_kind) {
            return ToolExecutionRecord {
                result: ToolExecResult {
                    ok: false,
                    value: None,
                    error: Some(ToolExecError {
                        code: "agent_cannot_spawn_children".into(),
                        message: "agent cannot spawn child agents".into(),
                        retryable: Some(false),
                    }),
                },
            };
        }

        // Compute ordering metadata
        let tool_entity_id = context.tool_entity_id.clone().unwrap_or_else(|| {
            runtime_tool_entity_id(
                context.parent_message_id.as_deref().unwrap_or(""),
                context.tool_call_index.unwrap_or(0),
            )
        });

        // ---- Check cancellation ----
        if let Some(ref token) = cancel
            && token.is_cancelled()
        {
            return ToolExecutionRecord {
                result: ToolExecResult {
                    ok: false,
                    value: None,
                    error: Some(ToolExecError {
                        code: "aborted".into(),
                        message: "Task cancelled".into(),
                        retryable: Some(false),
                    }),
                },
            };
        }

        // ---- Look up provider ----
        let providers = self.providers.read().await;
        let provider = match providers.get(&route.provider_id) {
            Some(p) => p,
            None => {
                return ToolExecutionRecord {
                    result: ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "not_found".into(),
                            message: format!(
                                "No provider \"{}\" for tool \"{}\"",
                                route.provider_id, call_name
                            ),
                            retryable: Some(false),
                        }),
                    },
                };
            }
        };

        // ---- Approval check ----
        let effective_approval = route
            .tool_def
            .approval
            .clone()
            .unwrap_or(ToolApprovalRequirement::Never);

        if effective_approval != ToolApprovalRequirement::Never {
            let needs_approval = matches!(
                effective_approval,
                ToolApprovalRequirement::Always | ToolApprovalRequirement::OnRequest
            );

            if needs_approval {
                if let Some(ref token) = cancel
                    && token.is_cancelled()
                {
                    return ToolExecutionRecord {
                        result: ToolExecResult {
                            ok: false,
                            value: None,
                            error: Some(ToolExecError {
                                code: "aborted".into(),
                                message: "Task cancelled".into(),
                                retryable: Some(false),
                            }),
                        },
                    };
                }

                let gateway = self.approval_gateway.read().await;
                if let Some(gw) = gateway.as_ref() {
                    // Race approval against cancellation
                    // F-12 safety evidence: the provider projects its
                    // enforceable writable roots so hostd can assess write
                    // targets deterministically before any user/guardian flow.
                    // F-19: the provider projects the enforcing role's
                    // writable roots so hostd's safety assessment matches
                    // the policy the call will actually run under.
                    let writable_roots = provider.writable_roots_for(context).map(|roots| {
                        roots
                            .iter()
                            .map(|root| root.display().to_string())
                            .collect()
                    });
                    let approval_request = ToolApprovalRequest {
                        tool_entity_id: tool_entity_id.clone(),
                        call_id: call_id.clone(),
                        agent_id: context.agent_id.clone(),
                        agent_instance_id: context.agent_instance_id.clone(),
                        root_input_id: context.root_input_id.clone(),
                        agent_role: context.agent_role.clone(),
                        // F-13: the catalog route's provider id (MCP server
                        // name for MCP tools) so hostd can resolve
                        // server/tool approval templates.
                        provider_id: Some(provider.id().to_string()),
                        tool_name: call_name.clone(),
                        tool_args: call_args.clone(),
                        host_context: context.host_context.clone(),
                        writable_roots,
                    };

                    let decision = if let Some(ref token) = cancel {
                        tokio::select! {
                            d = gw.request_tool_approval(approval_request) => d,
                            _ = token.cancelled() => ToolApprovalDecision::Decline,
                        }
                    } else {
                        gw.request_tool_approval(approval_request).await
                    };

                    if !piko_orchd_api::is_approval_accepted(&decision) {
                        let (code, message): (&str, String) = match decision {
                            ToolApprovalDecision::Expired => (
                                "approval_expired",
                                "Approval request expired before a decision arrived".into(),
                            ),
                            ToolApprovalDecision::GuardianDenied { reason } => (
                                "guardian_denied",
                                format!("Guardian denied approval: {reason}"),
                            ),
                            ToolApprovalDecision::GuardianUnavailable => (
                                "guardian_unavailable",
                                "Guardian review failed; failing closed".into(),
                            ),
                            ToolApprovalDecision::SafetyRejected { reason } => (
                                "safety_rejected",
                                format!("Write rejected by safety assessment: {reason}"),
                            ),
                            ToolApprovalDecision::PermissionDenied { reason } => (
                                "permission_denied",
                                format!("Command denied by permission policy: {reason}"),
                            ),
                            _ => ("declined", "User declined approval".into()),
                        };
                        return ToolExecutionRecord {
                            result: ToolExecResult {
                                ok: false,
                                value: None,
                                error: Some(ToolExecError {
                                    code: code.into(),
                                    message,
                                    retryable: Some(false),
                                }),
                            },
                        };
                    }
                } else {
                    // No approval gateway configured — deny tools that need approval.
                    return ToolExecutionRecord {
                        result: ToolExecResult {
                            ok: false,
                            value: None,
                            error: Some(ToolExecError {
                                code: "approval_unavailable".into(),
                                message: format!(
                                    "Tool '{call_name}' requires approval but no ApprovalGateway is configured"
                                ),
                                retryable: Some(false),
                            }),
                        },
                    };
                }
            }
        }

        // ---- Execute provider ----
        let provider_call = if route.provider_tool_name != call_name {
            ToolCall {
                id: call_id.clone(),
                name: route.provider_tool_name.clone(),
                arguments: call_args.clone(),
                partial_json: None,
            }
        } else {
            call.clone()
        };

        let exec_context = ToolExecutionContext {
            session_id: context.session_id.clone(),
            agent_instance_id: context.agent_instance_id.clone(),
            root_input_id: context.root_input_id.clone(),
            cancellation: context.cancellation.clone(),
            agent_id: context.agent_id.clone(),
            agent_role: context.agent_role.clone(),
            agent_kind: context.agent_kind,
            tool_set_ids: context.tool_set_ids.clone(),
            turn_index: context.turn_index,
            event_seq: context.event_seq,
            next_event_seq: context.next_event_seq,
            parent_message_id: context.parent_message_id.clone(),
            content_index: context.content_index,
            tool_call_index: context.tool_call_index,
            tool_entity_id: Some(tool_entity_id.clone()),
            host_context: context.host_context.clone(),
            context_remaining: context.context_remaining,
        };

        let exec_result = provider
            .execute(provider_call.clone(), exec_context.clone())
            .await;

        // A restricted command attempt may request exactly one broader
        // retry (F-23 Rev B). The retry is a new authority decision owned by
        // hostd; the provider never silently weakens containment and an
        // already elevated call can never recurse here. The retry prefers the
        // narrowest representable additional read authority derived from the
        // denial and falls back to full elevation only when no representable
        // path can be derived.
        if call_name == "exec_command"
            && call_args
                .get("sandbox_permissions")
                .and_then(|value| value.as_str())
                .unwrap_or("use_default")
                != "require_escalated"
            && exec_result
                .error
                .as_ref()
                .is_some_and(|error| error.code == "sandbox_denied")
        {
            let denial_message = exec_result
                .error
                .as_ref()
                .map(|error| error.message.as_str())
                .unwrap_or_default();
            let retry_args = super::denial::denial_retry_args(&call_args, denial_message);
            let gateway = self.approval_gateway.read().await;
            let Some(gateway) = gateway.as_ref() else {
                let mut result = exec_result;
                if let Some(error) = result.error.as_mut() {
                    error.message.push_str(super::denial::NO_GATEWAY_RETRY_NOTE);
                }
                return ToolExecutionRecord { result };
            };
            let request = ToolApprovalRequest {
                tool_entity_id: tool_entity_id.clone(),
                call_id: call_id.clone(),
                agent_id: context.agent_id.clone(),
                agent_instance_id: context.agent_instance_id.clone(),
                root_input_id: context.root_input_id.clone(),
                agent_role: context.agent_role.clone(),
                provider_id: Some(provider.id().to_string()),
                tool_name: call_name.clone(),
                tool_args: retry_args.clone(),
                host_context: context.host_context.clone(),
                writable_roots: provider.writable_roots_for(context).map(|roots| {
                    roots
                        .iter()
                        .map(|root| root.display().to_string())
                        .collect()
                }),
            };
            let decision = if let Some(ref token) = cancel {
                tokio::select! {
                    decision = gateway.request_tool_approval(request) => decision,
                    _ = token.cancelled() => ToolApprovalDecision::Decline,
                }
            } else {
                gateway.request_tool_approval(request).await
            };
            if !piko_orchd_api::is_approval_accepted(&decision) {
                let (code, message) = super::denial::approval_failure(&decision);
                return ToolExecutionRecord {
                    result: ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: code.into(),
                            message,
                            retryable: Some(false),
                        }),
                    },
                };
            }
            let retry_call = ToolCall {
                id: call_id,
                name: provider_call.name,
                arguments: retry_args.clone(),
                partial_json: None,
            };
            return ToolExecutionRecord {
                result: super::denial::attach_approved_grant(
                    provider.execute(retry_call, exec_context).await,
                    &retry_args,
                ),
            };
        }

        ToolExecutionRecord {
            result: exec_result,
        }
    }
}
