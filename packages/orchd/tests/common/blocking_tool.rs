use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use piko_orchd_api::{ToolDiscoveryContext, ToolExecResult, ToolProvider};
use piko_protocol::tools::{ToolSet, ToolSetToolRef};
use piko_protocol::{ToolApprovalRequirement, ToolDef, ToolExecutorRef, ToolProviderSource};

/// Stays inside `execute` until the test releases it so a turn is
/// deterministically mid-flight when a steer arrives.
#[derive(Clone)]
pub struct BlockingToolProvider {
    pub started: Arc<AtomicBool>,
    pub release: Arc<AtomicBool>,
}

impl BlockingToolProvider {
    pub fn new() -> Self {
        Self {
            started: Arc::new(AtomicBool::new(false)),
            release: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn tool_set() -> ToolSet {
        ToolSet {
            id: "block".into(),
            name: "Block".into(),
            description: None,
            feature: None,
            metadata: None,
            policy: None,
            tools: vec![ToolSetToolRef::ProviderNamespace {
                provider_id: "block".into(),
                namespace: "".into(),
                alias: None,
                policy: None,
            }],
        }
    }
}

#[async_trait]
impl ToolProvider for BlockingToolProvider {
    fn id(&self) -> &str {
        "block"
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Orch
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        vec![ToolDef {
            name: "block_until_released".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test", "block/block_until_released"),
            description: "Block until the test releases the gate.".into(),
            input_schema: serde_json::json!({ "type": "object" }),
            executor: ToolExecutorRef {
                kind: "block".into(),
                target: "block_until_released".into(),
                extra: None,
            },
            execution_mode: None,
            exposure: None,
            capabilities: None,
            approval: Some(ToolApprovalRequirement::Never),
            metadata: None,
        }]
    }

    async fn execute(
        &self,
        _call: piko_protocol::ToolCall,
        _context: piko_orchd_api::ToolExecutionContext,
    ) -> ToolExecResult {
        self.started.store(true, Ordering::SeqCst);
        while !self.release.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        ToolExecResult {
            ok: true,
            value: Some(serde_json::json!({ "released": true })),
            error: None,
        }
    }
}
