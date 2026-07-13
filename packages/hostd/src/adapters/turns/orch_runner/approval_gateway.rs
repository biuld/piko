use async_trait::async_trait;
use tokio::sync::oneshot;

use orchd_api::{ApprovalGateway, ToolApprovalDecision, ToolApprovalRequest};

use crate::adapters::turns::approval::ApprovalScope;
use crate::api::ServerMessage;

use super::OrchTurnRunner;

#[async_trait]
impl ApprovalGateway for OrchTurnRunner {
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

        let (tx, rx) = oneshot::channel();
        let approval_id = request.tool_entity_id.clone();
        let session_id = request
            .host_context
            .as_ref()
            .map(|context| context.session_id.clone());
        {
            let mut pending = self.pending_approvals.lock().unwrap();
            pending.insert(
                approval_id.clone(),
                super::PendingApprovalEntry {
                    session_id,
                    snapshot: crate::api::ApprovalSnapshot {
                        approval_id: approval_id.clone(),
                        tool_name: request.tool_name.clone(),
                        request: request.tool_args.clone(),
                        status: crate::api::ApprovalStatus::Pending,
                    },
                    tx,
                },
            );
        }

        self.emit_ui_event(ServerMessage::Approval(
            crate::api::ApprovalEvent::Requested {
                agent_instance_id: request.agent_instance_id.clone(),
                agent_id: request.agent_id.clone(),
                approval_id: approval_id.clone(),
                tool_name: request.tool_name.clone(),
                tool_args: request.tool_args.clone(),
            },
        ));

        let decision = match rx.await {
            Ok(d) => d,
            Err(_) => piko_protocol::ApprovalDecision::Decline,
        };

        {
            let mut pending = self.pending_approvals.lock().unwrap();
            pending.remove(&approval_id);
        }

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
