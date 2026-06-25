use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::json;

use crate::protocol::approval::{ApprovalGateway, ToolApprovalDecision, ToolApprovalRequest};
use crate::rpc::peer::RpcPeer;

pub struct RpcApprovalGateway {
    peer: Arc<RpcPeer>,
}

impl RpcApprovalGateway {
    pub fn new(peer: Arc<RpcPeer>) -> Self {
        Self { peer }
    }
}

impl ApprovalGateway for RpcApprovalGateway {
    fn request_tool_approval(
        &self,
        request: ToolApprovalRequest,
    ) -> Pin<Box<dyn Future<Output = ToolApprovalDecision> + Send + '_>> {
        Box::pin(async move {
            let value = self
                .peer
                .request("host.approval.request", json!({ "request": request }))
                .await;
            match value {
                Ok(value) => serde_json::from_value(value).unwrap_or(ToolApprovalDecision::Decline),
                Err(_) => ToolApprovalDecision::Decline,
            }
        })
    }
}
