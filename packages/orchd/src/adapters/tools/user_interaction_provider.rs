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
use piko_protocol::{
    InteractionAnswer, InteractionChoice, InteractionInput, InteractionQuestion,
    UserInteractionResponse,
};

// ---- Callbacks interface (stub) ----

/// Callbacks that the host layer will eventually implement to bridge
/// tool calls to the TUI / host process.
#[derive(Default, Clone)]
pub struct UserInteractionCallbacks {
    pub request_user_input: Option<
        Arc<
            dyn Fn(
                    UserInteractionRequest,
                ) -> Pin<Box<dyn Future<Output = UserInteractionResponse> + Send>>
                + Send
                + Sync,
        >,
    >,
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

#[derive(Clone, Debug)]
pub struct UserInteractionRequest {
    pub task_id: String,
    pub agent_id: String,
    pub tool_call_id: String,
    pub title: Option<String>,
    pub questions: Vec<InteractionQuestion>,
    pub require_confirm: bool,
    pub auto_resolution_ms: Option<u64>,
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
                name: "request_user_input".into(),
                description: "Ask the user one or more structured multiple-choice questions through the host/TUI.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Optional title for the interaction"
                        },
                        "questions": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string" },
                                    "header": { "type": "string" },
                                    "prompt": { "type": "string" },
                                    "required": { "type": "boolean" },
                                    "choices": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "id": { "type": "string" },
                                                "label": { "type": "string" },
                                                "value": {},
                                                "input": {
                                                    "type": "object",
                                                    "properties": {
                                                        "prompt": { "type": "string" },
                                                        "placeholder": { "type": "string" }
                                                    }
                                                }
                                            },
                                            "required": ["id", "label"]
                                        }
                                    }
                                },
                                "required": ["id", "header", "prompt", "choices"]
                            }
                        },
                        "require_confirm": { "type": "boolean" },
                        "auto_resolution_ms": { "type": "integer" }
                    },
                    "required": ["questions"]
                }),
                executor: ToolExecutorRef {
                    kind: "host".into(),
                    target: "request_user_input".into(),
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

    async fn execute(&self, call: ToolCall, context: ToolExecutionContext) -> ToolExecResult {
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
                let Some(ref cb) = cbs.request_user_input else {
                    return not_wired("ask_user");
                };
                let question = args.get("question").and_then(|v| v.as_str()).unwrap_or("");
                let response = cb(UserInteractionRequest {
                    task_id: context.task_id.clone(),
                    agent_id: context.agent_id.clone(),
                    tool_call_id: call_id_from_call(&call),
                    title: Some("Question".into()),
                    questions: vec![InteractionQuestion {
                        id: "answer".into(),
                        header: "Question".into(),
                        prompt: question.to_string(),
                        choices: vec![InteractionChoice {
                            id: "answer".into(),
                            label: "Answer".into(),
                            value: serde_json::json!("answer"),
                            input: Some(InteractionInput {
                                prompt: "Answer: ".into(),
                                placeholder: None,
                            }),
                        }],
                        required: true,
                    }],
                    require_confirm: false,
                    auto_resolution_ms: None,
                })
                .await;
                match response {
                    UserInteractionResponse::Submit { answers } => ToolExecResult {
                        ok: true,
                        value: Some(serde_json::json!(
                            answers.first().and_then(answer_input).unwrap_or_default()
                        )),
                        error: None,
                    },
                    UserInteractionResponse::Cancel { reason } => ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "cancelled".into(),
                            message: reason.unwrap_or_else(|| "User cancelled".into()),
                            retryable: Some(false),
                        }),
                    },
                }
            }
            "request_user_input" => {
                let Some(ref cb) = cbs.request_user_input else {
                    return not_wired("request_user_input");
                };
                let questions = parse_questions(args.get("questions"));
                if questions.is_empty() {
                    return ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "invalid_request".into(),
                            message: "request_user_input requires at least one question".into(),
                            retryable: Some(false),
                        }),
                    };
                }
                let response = cb(UserInteractionRequest {
                    task_id: context.task_id.clone(),
                    agent_id: context.agent_id.clone(),
                    tool_call_id: call_id_from_call(&call),
                    title: args.get("title").and_then(|v| v.as_str()).map(String::from),
                    questions,
                    require_confirm: args
                        .get("require_confirm")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    auto_resolution_ms: args.get("auto_resolution_ms").and_then(|v| v.as_u64()),
                })
                .await;
                match response {
                    UserInteractionResponse::Submit { answers } => ToolExecResult {
                        ok: true,
                        value: Some(serde_json::json!({ "answers": answers })),
                        error: None,
                    },
                    UserInteractionResponse::Cancel { reason } => ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "cancelled".into(),
                            message: reason.unwrap_or_else(|| "User cancelled".into()),
                            retryable: Some(false),
                        }),
                    },
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

fn call_id_from_call(call: &ToolCall) -> String {
    match call {
        ToolCall::ToolCall { id, .. } => id.clone(),
        _ => String::new(),
    }
}

fn answer_input(answer: &InteractionAnswer) -> Option<String> {
    answer.input.clone()
}

fn parse_questions(value: Option<&serde_json::Value>) -> Vec<InteractionQuestion> {
    let Some(serde_json::Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| parse_question(idx, item))
        .collect()
}

fn parse_question(idx: usize, value: &serde_json::Value) -> Option<InteractionQuestion> {
    let obj = value.as_object()?;
    let id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| format!("q{}", idx + 1));
    let header = obj
        .get("header")
        .and_then(|v| v.as_str())
        .unwrap_or(&id)
        .to_string();
    let prompt = obj
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let choices = obj
        .get("choices")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .enumerate()
                .filter_map(|(choice_idx, item)| parse_choice(choice_idx, item))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if choices.is_empty() {
        return None;
    }
    Some(InteractionQuestion {
        id,
        header,
        prompt,
        choices,
        required: obj
            .get("required")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
    })
}

fn parse_choice(idx: usize, value: &serde_json::Value) -> Option<InteractionChoice> {
    let obj = value.as_object()?;
    let id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| format!("choice{}", idx + 1));
    let label = obj
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or(&id)
        .to_string();
    let input = obj.get("input").and_then(|v| {
        let input = v.as_object()?;
        Some(InteractionInput {
            prompt: input
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            placeholder: input
                .get("placeholder")
                .and_then(|v| v.as_str())
                .map(String::from),
        })
    });
    Some(InteractionChoice {
        id,
        label,
        value: obj.get("value").cloned().unwrap_or(serde_json::Value::Null),
        input,
    })
}
