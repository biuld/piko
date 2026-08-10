use super::*;

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
                    feature_enabled(features.as_ref(), &e.tool_def)
                        && active.contains(&e.public_name)
                })
                .map(|e| e.tool_def.clone())
                .collect()
        } else {
            catalog
                .iter()
                .filter(|e| feature_enabled(features.as_ref(), &e.tool_def))
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
            if !feature_enabled(features.as_ref(), &entry.tool_def) {
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
            execution_id: context.execution_id.clone(),
            cancellation: context.cancellation.clone(),
            agent_id: context.agent_id.clone(),
            agent_role: context.agent_role.clone(),
            tool_set_ids: context.tool_set_ids.clone(),
            turn_index: context.turn_index,
            event_seq: context.event_seq,
            next_event_seq: context.next_event_seq,
            parent_message_id: context.parent_message_id.clone(),
            content_index: context.content_index,
            tool_call_index: context.tool_call_index,
            tool_entity_id: Some(tool_entity_id.clone()),
            host_context: context.host_context.clone(),
            source_turn_id: context.source_turn_id.clone(),
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
            let retry_args = denial_retry_args(&call_args, denial_message);
            let gateway = self.approval_gateway.read().await;
            let Some(gateway) = gateway.as_ref() else {
                return ToolExecutionRecord {
                    result: exec_result,
                };
            };
            let request = ToolApprovalRequest {
                tool_entity_id: tool_entity_id.clone(),
                call_id: call_id.clone(),
                agent_id: context.agent_id.clone(),
                agent_instance_id: context.agent_instance_id.clone(),
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
                let (code, message) = approval_failure(&decision);
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
                arguments: retry_args,
                partial_json: None,
            };
            return ToolExecutionRecord {
                result: provider.execute(retry_call, exec_context).await,
            };
        }

        ToolExecutionRecord {
            result: exec_result,
        }
    }
}

fn approval_failure(decision: &ToolApprovalDecision) -> (&'static str, String) {
    match decision {
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
    }
}

/// Derive the retry arguments for a sandbox-denied command (F-23 Rev B).
///
/// Absolute denied paths parsed from the platform denial become a
/// `with_additional_permissions` retry with those paths as read roots;
/// `require_escalated` is used only when no representable path can be
/// derived. When the command is a simple narrow program + subcommand and the
/// call did not already propose one, the retry attaches a reusable
/// `prefix_rule` so repeat commands reuse the grant instead of prompting
/// again.
fn denial_retry_args(call_args: &serde_json::Value, denial_message: &str) -> serde_json::Value {
    let mut retry = call_args.clone();
    let denied = denied_absolute_paths(denial_message);
    if denied.is_empty() {
        retry["sandbox_permissions"] = serde_json::json!("require_escalated");
        retry["justification"] = serde_json::json!(
            "The enforced sandbox denied the initial command attempt; retry once with explicit elevation"
        );
    } else {
        retry["sandbox_permissions"] = serde_json::json!("with_additional_permissions");
        retry["justification"] = serde_json::json!(
            "The enforced sandbox denied the initial command attempt; retry once with minimal additional read access"
        );
        let mut additional = retry
            .get("additional_permissions")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let mut reads: Vec<String> = additional
            .get("read_roots")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
        for path in denied {
            if !reads.iter().any(|existing| existing == &path) {
                reads.push(path);
            }
        }
        additional["read_roots"] = serde_json::json!(reads);
        retry["additional_permissions"] = additional;
    }
    if retry.get("prefix_rule").is_none()
        && let Some(prefix) = reusable_retry_prefix(call_args)
    {
        retry["prefix_rule"] = serde_json::json!(prefix);
    }
    retry
}

/// Absolute filesystem paths named by a platform denial message, deduplicated
/// and bounded so the derived additional permissions stay narrow.
fn denied_absolute_paths(message: &str) -> Vec<String> {
    const MAX_PATHS: usize = 8;
    let mut out: Vec<String> = Vec::new();
    for line in message.lines() {
        if !line.to_ascii_lowercase().contains("deny") {
            continue;
        }
        for token in line.split_whitespace() {
            let token = token.trim_matches(|ch| matches!(ch, '"' | '\'' | '(' | ')' | ','));
            if !token.starts_with('/')
                || token.starts_with("//")
                || token.starts_with("/dev/")
                || out.iter().any(|existing| existing == token)
            {
                continue;
            }
            out.push(token.to_string());
            if out.len() >= MAX_PATHS {
                return out;
            }
        }
    }
    out
}

/// Propose a reusable narrow prefix when the command is a simple program +
/// subcommand pair. Shell constructs, interpreters, shell builtins, and
/// commands whose second token is a path or flag are ineligible, mirroring
/// the operator prefix-rule restrictions.
fn reusable_retry_prefix(call_args: &serde_json::Value) -> Option<Vec<String>> {
    let command = call_args.get("cmd")?.as_str()?;
    if command.is_empty() || command.chars().any(|ch| ";|&<>`$\n\r()".contains(ch)) {
        return None;
    }
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if tokens.len() < 2 {
        return None;
    }
    let program = tokens[0];
    let subcommand = tokens[1];
    if !is_reusable_program(program) || !is_subcommand_token(subcommand) {
        return None;
    }
    Some(vec![program.to_string(), subcommand.to_string()])
}

fn is_reusable_program(program: &str) -> bool {
    if program.contains('/') {
        return false;
    }
    !matches!(
        program,
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "sudo"
            | "env"
            | "python"
            | "python3"
            | "node"
            | "ruby"
            | "perl"
            | "rm"
            | "curl"
            | "wget"
            | "cd"
            | "export"
            | "echo"
            | "set"
            | "unset"
            | "alias"
            | "source"
            | "exec"
            | "eval"
            | "test"
            | "printf"
            | "read"
            | "true"
            | "false"
            | "time"
            | "local"
            | "shift"
            | "return"
            | "exit"
            | "type"
            | "ulimit"
            | "umask"
            | "wait"
            | "."
    )
}

fn is_subcommand_token(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    token
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}
