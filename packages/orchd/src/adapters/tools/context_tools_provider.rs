// ---- ContextToolsProvider — model-visible context budget tools (F-05) ----
//
// Two read-mostly tools that keep the model informed about its own context
// budget:
//
//   get_context_remaining — reports the estimated tokens left in the window
//                           from the exact F-04 budget basis of the current
//                           model step (threaded on ToolExecutionContext).
//   new_context_window    — asks hostd to drop history without summarization
//                           (token-budget compact). The rewrite stays
//                           host-owned; orchd only forwards the request
//                           through an optional callback.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::domain::tools::call::ToolCall;
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolDef, ToolExecutionMode, ToolExecutorRef, ToolProviderSource,
};
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::ports::tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};

pub const NEW_CONTEXT_WINDOW_MESSAGE: &str =
    "A new context window was started without summarizing conversation history.";

pub type NewContextWindowCallback = Arc<
    dyn Fn(String, String) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>
        + Send
        + Sync,
>;

/// Callbacks wired by the host layer; execution fails closed while unset.
#[derive(Default, Clone)]
pub struct ContextToolsCallbacks {
    pub new_context_window: Option<NewContextWindowCallback>,
}

#[derive(Clone)]
pub struct ContextToolsProvider {
    callbacks: Arc<RwLock<ContextToolsCallbacks>>,
}

impl std::fmt::Debug for ContextToolsProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextToolsProvider").finish()
    }
}

impl Default for ContextToolsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextToolsProvider {
    pub fn new() -> Self {
        Self {
            callbacks: Arc::new(RwLock::new(ContextToolsCallbacks::default())),
        }
    }

    /// Set the host callbacks (typically once during host init).
    pub async fn set_callbacks(&self, callbacks: ContextToolsCallbacks) {
        let mut guard = self.callbacks.write().await;
        *guard = callbacks;
    }

    fn tools() -> Vec<ToolDef> {
        vec![
            ToolDef {
                name: "get_context_remaining".into(),
                version: "1".into(),
                provenance: piko_protocol::PromptSource::new(
                    "built-in-tool",
                    "context/get_context_remaining",
                ),
                description: "Report the estimated number of tokens remaining in the current model context window. Use this when deciding whether to continue a long session or start a new context window.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
                executor: ToolExecutorRef {
                    kind: "context".into(),
                    target: "get_context_remaining".into(),
                    extra: None,
                },
                execution_mode: Some(ToolExecutionMode::Sequential),
                exposure: None,
                capabilities: None,
                approval: Some(ToolApprovalRequirement::Never),
                metadata: None,
            },
            ToolDef {
                name: "new_context_window".into(),
                version: "1".into(),
                provenance: piko_protocol::PromptSource::new(
                    "built-in-tool",
                    "context/new_context_window",
                ),
                description: "Start a new context window by dropping summarized history without calling the model. The most recent user message is retained and a checkpoint entry is appended. Use when context is nearly exhausted and you want to continue the same session.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
                executor: ToolExecutorRef {
                    kind: "context".into(),
                    target: "new_context_window".into(),
                    extra: None,
                },
                execution_mode: Some(ToolExecutionMode::Sequential),
                exposure: None,
                capabilities: None,
                approval: Some(ToolApprovalRequirement::Never),
                metadata: None,
            },
        ]
    }
}

#[async_trait]
impl ToolProvider for ContextToolsProvider {
    fn id(&self) -> &str {
        "context"
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Orch
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        Self::tools()
    }

    async fn execute(&self, call: ToolCall, context: ToolExecutionContext) -> ToolExecResult {
        match call.name.as_str() {
            "get_context_remaining" => ToolExecResult {
                ok: true,
                value: Some(serde_json::json!({
                    "tokens_left": context.context_remaining,
                })),
                error: None,
            },
            "new_context_window" => {
                let callback = {
                    let guard = self.callbacks.read().await;
                    guard.new_context_window.clone()
                };
                let Some(callback) = callback else {
                    return ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "unavailable".into(),
                            message: "new_context_window is not wired to a host session"
                                .to_string(),
                            retryable: Some(false),
                        }),
                    };
                };
                match callback(context.session_id, context.agent_instance_id).await {
                    Ok(()) => ToolExecResult {
                        ok: true,
                        value: Some(serde_json::json!({
                            "message": NEW_CONTEXT_WINDOW_MESSAGE,
                        })),
                        error: None,
                    },
                    Err(error) => ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "compact_failed".into(),
                            message: error,
                            retryable: Some(false),
                        }),
                    },
                }
            }
            _ => ToolExecResult {
                ok: false,
                value: None,
                error: Some(ToolExecError {
                    code: "unknown_tool".into(),
                    message: format!("Unknown context tool: {}", call.name),
                    retryable: Some(false),
                }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str) -> ToolCall {
        ToolCall {
            id: format!("call-{name}"),
            name: name.into(),
            arguments: serde_json::json!({}),
            partial_json: None,
        }
    }

    fn context_with_remaining(remaining: Option<u64>) -> ToolExecutionContext {
        ToolExecutionContext {
            session_id: "session-1".into(),
            agent_instance_id: "agent_session-1_root".into(),
            execution_id: "exec-1".into(),
            cancellation: None,
            agent_id: "main".into(),
            agent_role: None,
            agent_kind: piko_protocol::AgentKind::Supervisor,
            tool_set_ids: vec![],
            turn_index: None,
            event_seq: None,
            next_event_seq: None,
            parent_message_id: None,
            content_index: None,
            tool_call_index: None,
            tool_entity_id: None,
            host_context: None,
            source_turn_id: None,
            context_remaining: remaining,
        }
    }

    #[tokio::test]
    async fn get_context_remaining_reports_budget_basis() {
        let provider = ContextToolsProvider::new();
        let result = provider
            .execute(
                call("get_context_remaining"),
                context_with_remaining(Some(42_000)),
            )
            .await;
        assert!(result.ok);
        assert_eq!(result.value.unwrap()["tokens_left"], 42_000);

        let unknown = provider
            .execute(call("get_context_remaining"), context_with_remaining(None))
            .await;
        assert!(unknown.ok);
        assert!(unknown.value.unwrap()["tokens_left"].is_null());
    }

    #[tokio::test]
    async fn new_context_window_fails_closed_without_callback() {
        let provider = ContextToolsProvider::new();
        let result = provider
            .execute(call("new_context_window"), context_with_remaining(None))
            .await;
        assert!(!result.ok);
        assert_eq!(result.error.unwrap().code, "unavailable");
    }

    #[tokio::test]
    async fn new_context_window_invokes_host_callback_once() {
        let provider = ContextToolsProvider::new();
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let calls_arc = Arc::clone(&calls);
        let callback: NewContextWindowCallback = Arc::new(move |session_id, agent_instance_id| {
            let calls_arc = Arc::clone(&calls_arc);
            Box::pin(async move {
                calls_arc
                    .lock()
                    .unwrap()
                    .push(format!("{session_id}/{agent_instance_id}"));
                Ok(())
            })
        });
        provider
            .set_callbacks(ContextToolsCallbacks {
                new_context_window: Some(callback),
            })
            .await;

        let result = provider
            .execute(call("new_context_window"), context_with_remaining(None))
            .await;
        assert!(result.ok);
        assert_eq!(result.value.unwrap()["message"], NEW_CONTEXT_WINDOW_MESSAGE);
        assert_eq!(calls.lock().unwrap().len(), 1);
        assert_eq!(calls.lock().unwrap()[0], "session-1/agent_session-1_root");
    }

    #[tokio::test]
    async fn new_context_window_surfaces_callback_failure() {
        let provider = ContextToolsProvider::new();
        let callback: NewContextWindowCallback =
            Arc::new(|_, _| Box::pin(async { Err("no user message to retain".to_string()) }));
        provider
            .set_callbacks(ContextToolsCallbacks {
                new_context_window: Some(callback),
            })
            .await;

        let result = provider
            .execute(call("new_context_window"), context_with_remaining(None))
            .await;
        assert!(!result.ok);
        assert_eq!(result.error.unwrap().code, "compact_failed");
    }
}
