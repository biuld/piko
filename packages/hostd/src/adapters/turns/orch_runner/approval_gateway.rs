use async_trait::async_trait;
use piko_orchd_api::{ApprovalGateway, ToolApprovalDecision, ToolApprovalRequest};

use super::OrchAgentRunRunner;
use crate::adapters::turns::approval::ApprovalScope;
use crate::domain::guardian::{GuardianConfig, GuardianReviewRequest, GuardianState};
use crate::domain::safety::WriteSafetyDecision;

#[async_trait]
impl ApprovalGateway for OrchAgentRunRunner {
    async fn request_tool_approval(&self, request: ToolApprovalRequest) -> ToolApprovalDecision {
        let _prompt_turn = self.prompt_gate.lock().await;
        let cwd = request
            .host_context
            .as_ref()
            .and_then(|context| self.session_cwd(&context.session_id))
            .unwrap_or_default();

        if !cwd.is_empty() {
            let store = self.get_approval_store(&cwd);
            if let Some(scope) = store.is_approved(&request.tool_name, &request.tool_args) {
                tracing::event!(
                    target: "tool.approval",
                    tracing::Level::INFO,
                    tool = %request.tool_name,
                    tool_call_id = %request.tool_entity_id,
                    agent_instance_id = %request.agent_instance_id,
                    decision = ?scope,
                    "Tool approval auto-accepted from prior grant"
                );
                tracing::info!(
                    "Auto-accepting pre-approved tool: {} at scope {:?}",
                    request.tool_name,
                    scope
                );
                return match scope {
                    ApprovalScope::Session => ToolApprovalDecision::AcceptSession,
                    ApprovalScope::Workspace => ToolApprovalDecision::AcceptWorkspace,
                    ApprovalScope::Permanent => ToolApprovalDecision::AcceptPermanent,
                };
            }
        }

        // F-12 write safety: a deterministic, host-owned gate before the
        // guardian and user flows. A write fully inside the sandbox writable
        // roots is auto-approved one-shot (the policy enforces the boundary
        // at execution); a write outside the roots fails closed because
        // execution would deny it regardless of approval.
        if self.safety_config.auto_approve_workspace_writes {
            match crate::domain::safety::assess_write_safety(
                &request.tool_name,
                &request.tool_args,
                request.writable_roots.as_deref().unwrap_or(&[]),
                &cwd,
            ) {
                WriteSafetyDecision::AutoApprove => {
                    tracing::event!(
                        target: "tool.approval",
                        tracing::Level::INFO,
                        tool = %request.tool_name,
                        tool_call_id = %request.tool_entity_id,
                        agent_instance_id = %request.agent_instance_id,
                        session_id = %request
                            .host_context
                            .as_ref()
                            .map(|context| context.session_id.as_str())
                            .unwrap_or(""),
                        "Safety assessment auto-approved workspace write (one-shot, no grant)"
                    );
                    return ToolApprovalDecision::Accept;
                }
                WriteSafetyDecision::Reject { reason } => {
                    tracing::event!(
                        target: "tool.approval",
                        tracing::Level::WARN,
                        tool = %request.tool_name,
                        tool_call_id = %request.tool_entity_id,
                        agent_instance_id = %request.agent_instance_id,
                        session_id = %request
                            .host_context
                            .as_ref()
                            .map(|context| context.session_id.as_str())
                            .unwrap_or(""),
                        reason = %reason,
                        "Safety assessment rejected workspace write"
                    );
                    return ToolApprovalDecision::SafetyRejected { reason };
                }
                WriteSafetyDecision::AskUser => {}
            }
        }

        // F-11 guardian auto-review: when enabled and the session breaker is
        // not tripped, a bounded model review decides before the user flow.
        if let Some(guardian) = &self.guardian_config
            && guardian.enabled
        {
            let session_id = request
                .host_context
                .as_ref()
                .map(|context| context.session_id.clone())
                .unwrap_or_default();
            let state = self.guardian_state(&session_id);
            if !state.tripped {
                let callback = { self.guardian_review.read().unwrap().clone() };
                if let Some(callback) = callback {
                    let review_request = GuardianReviewRequest {
                        agent_instance_id: request.agent_instance_id.clone(),
                        tool_name: request.tool_name.clone(),
                        tool_args: request.tool_args.clone(),
                    };
                    let outcome = tokio::time::timeout(
                        guardian.timeout,
                        callback(session_id.clone(), review_request),
                    )
                    .await;
                    match outcome {
                        Ok(Ok(decision)) if decision.allow => {
                            tracing::event!(
                                target: "tool.approval",
                                tracing::Level::INFO,
                                tool = %request.tool_name,
                                tool_call_id = %request.tool_entity_id,
                                agent_instance_id = %request.agent_instance_id,
                                session_id = %session_id,
                                "Guardian allowed tool approval (one-shot, no grant)"
                            );
                            // One-shot accept: no store grant is written,
                            // so future calls are reviewed again.
                            return ToolApprovalDecision::Accept;
                        }
                        Ok(Ok(decision)) => {
                            self.record_guardian_non_accept(&session_id, guardian);
                            tracing::event!(
                                target: "tool.approval",
                                tracing::Level::WARN,
                                tool = %request.tool_name,
                                tool_call_id = %request.tool_entity_id,
                                agent_instance_id = %request.agent_instance_id,
                                session_id = %session_id,
                                reason = %decision.reason,
                                "Guardian denied tool approval"
                            );
                            return ToolApprovalDecision::GuardianDenied {
                                reason: decision.reason,
                            };
                        }
                        Ok(Err(error)) => {
                            self.record_guardian_non_accept(&session_id, guardian);
                            tracing::event!(
                                target: "tool.approval",
                                tracing::Level::WARN,
                                tool = %request.tool_name,
                                tool_call_id = %request.tool_entity_id,
                                agent_instance_id = %request.agent_instance_id,
                                session_id = %session_id,
                                error = %error,
                                "Guardian review failed; failing closed"
                            );
                            return ToolApprovalDecision::GuardianUnavailable;
                        }
                        Err(_elapsed) => {
                            self.record_guardian_non_accept(&session_id, guardian);
                            tracing::event!(
                                target: "tool.approval",
                                tracing::Level::WARN,
                                tool = %request.tool_name,
                                tool_call_id = %request.tool_entity_id,
                                agent_instance_id = %request.agent_instance_id,
                                session_id = %session_id,
                                timeout_ms = guardian.timeout.as_millis(),
                                "Guardian review timed out; failing closed"
                            );
                            return ToolApprovalDecision::GuardianUnavailable;
                        }
                    }
                }
            }
        }

        let (tx, rx) = piko_comms::reply::<piko_comms::contracts::ApprovalReply, _>();
        let approval_id = request.tool_entity_id.clone();
        let session_id = request
            .host_context
            .as_ref()
            .map(|context| context.session_id.clone());
        let Some(session_id) = session_id else {
            tracing::warn!("declining approval without host session context");
            return ToolApprovalDecision::Decline;
        };
        let snapshot = crate::api::ApprovalSnapshot {
            approval_id: approval_id.clone(),
            agent_instance_id: request.agent_instance_id.clone(),
            tool_name: request.tool_name.clone(),
            request: request.tool_args.clone(),
            status: crate::api::ApprovalStatus::Pending,
        };
        {
            let mut pending = self.pending_approvals.lock().unwrap();
            pending.insert(
                approval_id.clone(),
                super::PendingApprovalEntry {
                    session_id: Some(session_id.clone()),
                    snapshot: snapshot.clone(),
                    tx,
                },
            );
        }

        self.observation_router
            .publish(
                &session_id,
                &request.agent_instance_id,
                &request.agent_id,
                piko_protocol::agent_runtime::SessionEvent::ApprovalRequested {
                    approval: snapshot,
                },
            )
            .await;

        let decision = match tokio::time::timeout(self.approval_timeout, rx).await {
            Ok(Ok(d)) => d,
            Ok(Err(_)) => piko_protocol::ApprovalDecision::Decline,
            Err(_elapsed) => {
                // Fail closed: the deadline passed with no user decision.
                // Remove the pending entry first so exactly one resolution
                // path owns the outcome; late user responses become no-ops.
                {
                    let mut pending = self.pending_approvals.lock().unwrap();
                    pending.remove(&approval_id);
                }
                self.observation_router
                    .publish(
                        &session_id,
                        &request.agent_instance_id,
                        &request.agent_id,
                        piko_protocol::agent_runtime::SessionEvent::ApprovalResolved {
                            approval_id: approval_id.clone(),
                            status: piko_protocol::ApprovalStatus::Expired,
                        },
                    )
                    .await;
                tracing::event!(
                    target: "tool.approval",
                    tracing::Level::WARN,
                    tool = %request.tool_name,
                    tool_call_id = %request.tool_entity_id,
                    agent_instance_id = %request.agent_instance_id,
                    timeout_ms = self.approval_timeout.as_millis(),
                    "Tool approval expired: no decision before deadline"
                );
                self.reset_guardian_state(&session_id);
                return ToolApprovalDecision::Expired;
            }
        };
        tracing::event!(
            target: "tool.approval",
            tracing::Level::INFO,
            tool = %request.tool_name,
            tool_call_id = %request.tool_entity_id,
            agent_instance_id = %request.agent_instance_id,
            decision = ?decision,
            "Tool approval decision recorded"
        );

        {
            let mut pending = self.pending_approvals.lock().unwrap();
            pending.remove(&approval_id);
        }
        self.reset_guardian_state(&session_id);

        if !cwd.is_empty() {
            let store = self.get_approval_store(&cwd);
            match decision {
                piko_protocol::ApprovalDecision::AcceptSession => {
                    store.grant(
                        &request.tool_name,
                        &request.tool_args,
                        ApprovalScope::Session,
                    );
                }
                piko_protocol::ApprovalDecision::AcceptWorkspace => {
                    store.grant(
                        &request.tool_name,
                        &request.tool_args,
                        ApprovalScope::Workspace,
                    );
                }
                piko_protocol::ApprovalDecision::AcceptPermanent => {
                    store.grant(
                        &request.tool_name,
                        &request.tool_args,
                        ApprovalScope::Permanent,
                    );
                }
                _ => {}
            }
        }

        match decision {
            piko_protocol::ApprovalDecision::Accept => ToolApprovalDecision::Accept,
            piko_protocol::ApprovalDecision::Decline => ToolApprovalDecision::Decline,
            piko_protocol::ApprovalDecision::Expired => ToolApprovalDecision::Expired,
            piko_protocol::ApprovalDecision::AcceptSession => ToolApprovalDecision::AcceptSession,
            piko_protocol::ApprovalDecision::AcceptWorkspace => {
                ToolApprovalDecision::AcceptWorkspace
            }
            piko_protocol::ApprovalDecision::AcceptPermanent => {
                ToolApprovalDecision::AcceptPermanent
            }
        }
    }
}

impl OrchAgentRunRunner {
    fn guardian_state(&self, session_id: &str) -> GuardianState {
        self.guardian_states
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }

    fn record_guardian_non_accept(&self, session_id: &str, guardian: &GuardianConfig) {
        let mut states = self.guardian_states.lock().unwrap();
        let state = states.entry(session_id.to_string()).or_default();
        state.record_non_accept(guardian.max_consecutive_denials);
    }

    fn reset_guardian_state(&self, session_id: &str) {
        self.guardian_states.lock().unwrap().remove(session_id);
    }
}
