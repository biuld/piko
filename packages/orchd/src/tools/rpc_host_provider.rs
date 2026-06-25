use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::json;

use crate::protocol::messages::ToolCall;
use crate::protocol::tools::{
    ToolDef, ToolDiscoveryContext, ToolExecError, ToolExecResult, ToolExecutionContext,
    ToolProvider, ToolProviderSource,
};
use crate::rpc::peer::RpcPeer;

pub struct RpcHostToolProvider {
    id: String,
    source: ToolProviderSource,
    peer: Arc<RpcPeer>,
}

impl RpcHostToolProvider {
    pub fn new(id: String, source: ToolProviderSource, peer: Arc<RpcPeer>) -> Self {
        Self { id, source, peer }
    }
}

impl ToolProvider for RpcHostToolProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn source(&self) -> ToolProviderSource {
        self.source.clone()
    }

    fn discover(
        &self,
        context: ToolDiscoveryContext,
    ) -> Pin<Box<dyn Future<Output = Vec<ToolDef>> + Send + '_>> {
        Box::pin(async move {
            let value = self
                .peer
                .request(
                    "host.tools.discover",
                    json!({ "providerId": self.id, "context": context }),
                )
                .await;
            match value {
                Ok(value) => serde_json::from_value(value).unwrap_or_default(),
                Err(_) => vec![],
            }
        })
    }

    fn execute(
        &self,
        call: ToolCall,
        context: ToolExecutionContext,
    ) -> Pin<Box<dyn Future<Output = ToolExecResult> + Send + '_>> {
        Box::pin(async move {
            let call_id = match &call {
                ToolCall::ToolCall { id, .. } => id.clone(),
                _ => "unknown".to_string(),
            };
            let execution_id = context
                .tool_entity_id
                .clone()
                .unwrap_or_else(|| format!("{}:{}", context.task_id, call_id));
            let value = self
                .peer
                .request(
                    "host.tools.execute",
                    json!({
                        "providerId": self.id,
                        "executionId": execution_id,
                        "call": call,
                        "context": context,
                    }),
                )
                .await;
            match value {
                Ok(value) => serde_json::from_value(value).unwrap_or_else(|e| ToolExecResult {
                    ok: false,
                    value: None,
                    error: Some(ToolExecError {
                        code: "invalid_host_result".into(),
                        message: e.to_string(),
                        retryable: Some(false),
                    }),
                }),
                Err(error) => ToolExecResult {
                    ok: false,
                    value: None,
                    error: Some(ToolExecError {
                        code: "host_rpc_error".into(),
                        message: error.message,
                        retryable: Some(false),
                    }),
                },
            }
        })
    }
}
