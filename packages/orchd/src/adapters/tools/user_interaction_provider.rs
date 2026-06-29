// ---- UserInteractionProvider — user-facing interaction tools (stub) ----
//
// Provides ask_user and request_approval tools for agent ↔ user communication.
//
// Currently a stub — execute() will eventually be wired to the host bridge
// via callbacks or a HostBridge trait.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::domain::tools::call::ToolCall;
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutionMode, ToolExecutorRef,
    ToolProviderSource,
};
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::ports::tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};

// ---- Callbacks interface (stub) ----

/// Callbacks that the host layer will eventually implement to bridge
/// tool calls to the TUI / host process.
#[derive(Default, Clone)]
pub struct UserInteractionCallbacks {
    pub ask_user:
        Option<Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = String> + Send>> + Send + Sync>>,
    pub request_approval: Option<
        Arc<
            dyn Fn(
                    String,
                    Option<String>,
                ) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send>>
                + Send
                + Sync,
        >,
    >,
}

// ---- Provider ----

/// Host bridge tool provider.
///
/// Tools are always registered; execution succeeds only when the
/// corresponding callback has been wired by the host layer.
#[derive(Clone)]
pub struct UserInteractionProvider {
    callbacks: Arc<RwLock<UserInteractionCallbacks>>,
}

impl std::fmt::Debug for UserInteractionProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserInteractionProvider").finish()
    }
}

impl Default for UserInteractionProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl UserInteractionProvider {
    pub fn new() -> Self {
        Self {
            callbacks: Arc::new(RwLock::new(UserInteractionCallbacks::default())),
        }
    }

    /// Set callbacks (typically called once during host init).
    pub async fn set_callbacks(&self, cbs: UserInteractionCallbacks) {
        let mut guard = self.callbacks.write().await;
        *guard = cbs;
    }

    fn builtin_tools() -> Vec<ToolDef> {
        vec![
            ToolDef {
                name: "ask_user".into(),
                description: "Ask the user a direct question through the host/TUI.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "Question to ask the user"
                        }
                    },
                    "required": ["question"]
                }),
                executor: ToolExecutorRef {
                    kind: "host".into(),
                    target: "ask_user".into(),
                    extra: None,
                },
                execution_mode: Some(ToolExecutionMode::Sequential),
                exposure: None,
                capabilities: Some(vec![ToolCapability::UserInput]),
                approval: Some(ToolApprovalRequirement::Never),
                metadata: None,
            },
            ToolDef {
                name: "request_approval".into(),
                description:
                    "Request user approval for an action not tied to a tool approval policy.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "description": "Description of the action to approve"
                        },
                        "details": {
                            "type": "string",
                            "description": "Additional context"
                        }
                    },
                    "required": ["action"]
                }),
                executor: ToolExecutorRef {
                    kind: "host".into(),
                    target: "request_approval".into(),
                    extra: None,
                },
                execution_mode: Some(ToolExecutionMode::Sequential),
                exposure: None,
                capabilities: Some(vec![ToolCapability::UserInput]),
                approval: Some(ToolApprovalRequirement::Never),
                metadata: None,
            },
        ]
    }
}

#[async_trait]
impl ToolProvider for UserInteractionProvider {
    fn id(&self) -> &str {
        "user_interaction"
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Host
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        Self::builtin_tools()
    }

    async fn execute(&self, call: ToolCall, _context: ToolExecutionContext) -> ToolExecResult {
        let cbs = {
            let guard = self.callbacks.read().await;
            guard.clone()
        };

        let (tool_name, args) = match &call {
            ToolCall::ToolCall {
                name, arguments, ..
            } => (name.clone(), arguments.clone()),
            _ => {
                return ToolExecResult {
                    ok: false,
                    value: None,
                    error: Some(ToolExecError {
                        code: "invalid_call".into(),
                        message: "Expected a ToolCall content block".into(),
                        retryable: Some(false),
                    }),
                };
            }
        };

        let not_wired = |tool: &str| -> ToolExecResult {
            ToolExecResult {
                ok: false,
                value: None,
                error: Some(ToolExecError {
                    code: "no_handler".into(),
                    message: format!("{tool} callback not wired (host bridge stub)"),
                    retryable: Some(false),
                }),
            }
        };

        match tool_name.as_str() {
            "ask_user" => {
                let Some(ref cb) = cbs.ask_user else {
                    return not_wired("ask_user");
                };
                let question = args.get("question").and_then(|v| v.as_str()).unwrap_or("");
                let answer = cb(question.to_string()).await;
                ToolExecResult {
                    ok: true,
                    value: Some(serde_json::json!(answer)),
                    error: None,
                }
            }
            "request_approval" => {
                let Some(ref cb) = cbs.request_approval else {
                    return not_wired("request_approval");
                };
                let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
                let details = args
                    .get("details")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                match cb(action.to_string(), details).await {
                    Ok(approved) => ToolExecResult {
                        ok: true,
                        value: Some(serde_json::json!({ "approved": approved })),
                        error: None,
                    },
                    Err(e) => ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "approval_denied".into(),
                            message: e,
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
                    message: format!("Unknown host tool: {tool_name}"),
                    retryable: Some(false),
                }),
            },
        }
    }
}
