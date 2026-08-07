//! Spawn spec / when resolution and shared tool helpers (F-21).

use piko_orchd_api::AgentApiError;
use piko_protocol::AgentActivity;
use piko_protocol::agents::AgentSpec;
use piko_protocol::tools::{
    ToolApprovalRequirement, ToolCapability, ToolDef, ToolExecutionMode, ToolExecutorRef,
};

pub(super) const DEFAULT_SPAWN_SPEC_ID: &str = "general";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MessageWhen {
    Queue,
    Steer,
}

impl MessageWhen {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Queue => "queue",
            Self::Steer => "steer",
        }
    }
}

#[derive(Debug)]
pub(super) struct ToolFail {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl ToolFail {
    pub(super) fn from_agent(error: AgentApiError) -> Self {
        let retryable = matches!(
            error,
            AgentApiError::Overload | AgentApiError::RuntimeUnavailable
        );
        let code = match error {
            AgentApiError::AgentNotFound => "agent_not_found",
            AgentApiError::AgentSpecNotFound => "agent_spec_not_found",
            AgentApiError::InputRejected => "invalid_argument",
            AgentApiError::Cancelled => "cancelled",
            _ => "agent_runtime_error",
        };
        Self {
            code: code.into(),
            message: error.to_string(),
            retryable,
        }
    }
}

pub(super) fn resolve_when(args: &serde_json::Value) -> Result<MessageWhen, ToolFail> {
    match args.get("when").and_then(serde_json::Value::as_str) {
        None | Some("") => Ok(MessageWhen::Queue),
        Some("queue") => Ok(MessageWhen::Queue),
        Some("steer") => Ok(MessageWhen::Steer),
        Some(other) => Err(ToolFail {
            code: "invalid_argument".into(),
            message: format!("Invalid when \"{other}\". Must be \"queue\" or \"steer\"."),
            retryable: false,
        }),
    }
}

pub(super) fn resolve_spawn_spec_id(
    args: &serde_json::Value,
    specs: &[AgentSpec],
) -> Result<String, ToolFail> {
    let available: Vec<String> = specs.iter().map(|spec| spec.id.clone()).collect();
    let available_csv = available_ids_csv(&available);
    let explicit = args
        .get("agent_spec_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let id = match explicit {
        Some(id) => id,
        None => {
            if available.iter().any(|id| id == DEFAULT_SPAWN_SPEC_ID) {
                DEFAULT_SPAWN_SPEC_ID.to_string()
            } else {
                return Err(ToolFail {
                    code: "agent_spec_required".into(),
                    message: format!(
                        "agent_spec_id is required (no default). Valid ids: {available_csv}. Call list_agent_specs for details."
                    ),
                    retryable: false,
                });
            }
        }
    };

    if available.iter().any(|known| known == &id) {
        Ok(id)
    } else {
        Err(ToolFail {
            code: "agent_spec_not_found".into(),
            message: format!(
                "Unknown agent_spec_id \"{id}\". Valid ids: {available_csv}. Call list_agent_specs for details."
            ),
            retryable: false,
        })
    }
}

pub(super) fn catalog_value(specs: &[AgentSpec]) -> serde_json::Value {
    let default = specs
        .iter()
        .any(|spec| spec.id == DEFAULT_SPAWN_SPEC_ID)
        .then_some(DEFAULT_SPAWN_SPEC_ID);
    let entries: Vec<serde_json::Value> = specs
        .iter()
        .map(|spec| {
            let mut entry = serde_json::json!({
                "id": spec.id,
                "name": spec.name,
                "role": spec.role,
            });
            if let Some(description) = &spec.description
                && !description.is_empty()
            {
                entry.as_object_mut().unwrap().insert(
                    "description".into(),
                    serde_json::Value::String(description.clone()),
                );
            }
            entry
        })
        .collect();
    serde_json::json!({
        "specs": entries,
        "default_spawn_spec_id": default,
    })
}

pub(super) fn available_ids_csv(ids: &[String]) -> String {
    if ids.is_empty() {
        "(none)".into()
    } else {
        ids.join(", ")
    }
}

pub(super) fn map_spawn_agent_error(error: AgentApiError, specs: &[AgentSpec]) -> ToolFail {
    if matches!(error, AgentApiError::AgentSpecNotFound) {
        let available: Vec<String> = specs.iter().map(|spec| spec.id.clone()).collect();
        return ToolFail {
            code: "agent_spec_not_found".into(),
            message: format!(
                "Unknown agent_spec_id. Valid ids: {}. Call list_agent_specs for details.",
                available_ids_csv(&available)
            ),
            retryable: false,
        };
    }
    ToolFail::from_agent(error)
}

pub(super) fn required_string(
    value: &serde_json::Value,
    name: &str,
) -> Result<String, AgentApiError> {
    value
        .get(name)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or(AgentApiError::InputRejected)
}

pub(super) fn tool(name: &str, description: &str, input_schema: serde_json::Value) -> ToolDef {
    ToolDef {
        name: name.into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("built-in-tool", name),
        description: description.into(),
        input_schema,
        executor: ToolExecutorRef {
            kind: "orchestrator".into(),
            target: name.into(),
            extra: None,
        },
        execution_mode: Some(ToolExecutionMode::Sequential),
        exposure: None,
        capabilities: Some(vec![ToolCapability::Delegation]),
        approval: Some(ToolApprovalRequirement::Never),
        metadata: None,
    }
}

pub(super) fn spawn_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "agent_spec_id": {
                "type": "string",
                "description": "Registry template id from list_agent_specs (e.g. coder, scout, general). Not an agent_instance_id. Omit to use default when available."
            },
            "prompt": {
                "type": "string",
                "description": "Initial task for the child agent."
            }
        },
        "required": ["prompt"]
    })
}

pub(super) fn agent_target_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "agent_instance_id": {
                "type": "string",
                "description": "Live AgentInstance id from list_agents."
            }
        },
        "required": ["agent_instance_id"]
    })
}

pub(super) fn stable_runtime_id(execution_id: &str, tool_call_id: &str) -> String {
    piko_orchd_api::stable_internal_id("spawn", &[execution_id, tool_call_id])
}

pub(super) fn report_value(report: &piko_protocol::AgentRunReport) -> serde_json::Value {
    serde_json::json!({
        "agent_instance_id": report.agent_instance_id,
        "outcome": report.outcome,
        "summary": report.summary,
        "usage": report.usage,
        "artifacts": report.artifacts,
    })
}

pub(super) fn activity_str(activity: &AgentActivity) -> &'static str {
    match activity {
        AgentActivity::Idle => "idle",
        AgentActivity::Running => "running",
        AgentActivity::WaitingForApproval => "waiting_for_approval",
        AgentActivity::Cancelling => "cancelling",
    }
}

pub(super) fn multi_agent_tools() -> Vec<ToolDef> {
    vec![
        tool(
            "list_agent_specs",
            "List spawnable agent templates (AgentSpec registry). Use each entry's id as agent_spec_id when calling spawn_agent or spawn_agent_detached. This is not the live agent tree; for live instances call list_agents.",
            serde_json::json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "spawn_agent",
            "Create a child AgentInstance and wait for its first execution report. agent_spec_id comes from list_agent_specs (e.g. coder, scout, general). Omit agent_spec_id to use the default template when available (general). Not an agent_instance_id.",
            spawn_schema(),
        ),
        tool(
            "spawn_agent_detached",
            "Create a child AgentInstance that continues independently and reports to the caller inbox. Same agent_spec_id rules as spawn_agent; returns immediately with status accepted instead of waiting for a report.",
            spawn_schema(),
        ),
        tool(
            "message_agent",
            "Send work to a live child AgentInstance (id from list_agents). Default when=queue: start a new turn if idle, or durable-queue if busy. Use when=steer only to redirect an already running turn; fails if the agent is idle.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "agent_instance_id": {
                        "type": "string",
                        "description": "Live AgentInstance id from list_agents (not a template/spec id)."
                    },
                    "message": {
                        "type": "string",
                        "description": "Task text (when=queue) or mid-turn steer text (when=steer)."
                    },
                    "when": {
                        "type": "string",
                        "enum": ["queue", "steer"],
                        "description": "queue (default): start a turn if idle, or durable-queue if busy. steer: inject into the active turn only; fails if idle."
                    }
                },
                "required": ["agent_instance_id", "message"]
            }),
        ),
        tool(
            "collect_agent_reports",
            "Collect and durably consume unread detached reports for the calling AgentInstance.",
            serde_json::json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "close_agent",
            "Close an existing direct child AgentInstance to new input. Target agent_instance_id from list_agents.",
            agent_target_schema(),
        ),
        tool(
            "reopen_agent",
            "Reopen an existing direct child AgentInstance. Target agent_instance_id from list_agents.",
            agent_target_schema(),
        ),
        tool(
            "interrupt_agent",
            "Interrupt an agent's current turn and report its previous activity; the agent stays available for later message_agent calls. Target agent_instance_id from list_agents.",
            agent_target_schema(),
        ),
        tool(
            "list_agents",
            "List live AgentInstances in this session (parents before children). Use agent_instance_id for message_agent, wait_agent, interrupt, close, and reopen. agent_spec_id on each row is the template used at spawn only. To discover spawnable templates, call list_agent_specs.",
            serde_json::json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "wait_agent",
            "Wait (bounded by timeout_ms) for the next mailbox update from any live agent, optionally filtered to one agent_instance_id from list_agents.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "timeout_ms": { "type": "integer", "minimum": 1 },
                    "agent_instance_id": {
                        "type": "string",
                        "description": "Optional live AgentInstance id filter from list_agents."
                    }
                },
                "required": ["timeout_ms"]
            }),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::PromptSource;

    fn spec(id: &str) -> AgentSpec {
        AgentSpec {
            id: id.into(),
            version: "1".into(),
            provenance: PromptSource::new("test", id),
            name: id.into(),
            role: "test".into(),
            description: Some(format!("{id} desc")),
            base_instructions: "hi".into(),
            model: None,
            thinking_level: None,
            tool_set_ids: Vec::new(),
            active_tool_names: None,
        }
    }

    #[test]
    fn resolve_spawn_defaults_to_general() {
        let specs = vec![spec("coder"), spec("general")];
        let id = resolve_spawn_spec_id(&serde_json::json!({}), &specs).unwrap();
        assert_eq!(id, "general");
    }

    #[test]
    fn resolve_spawn_explicit_id() {
        let specs = vec![spec("coder"), spec("general")];
        let id = resolve_spawn_spec_id(&serde_json::json!({ "agent_spec_id": "coder" }), &specs)
            .unwrap();
        assert_eq!(id, "coder");
    }

    #[test]
    fn resolve_spawn_unknown_lists_valid_ids() {
        let specs = vec![spec("coder")];
        let err = resolve_spawn_spec_id(
            &serde_json::json!({ "agent_spec_id": "agents/main" }),
            &specs,
        )
        .unwrap_err();
        assert_eq!(err.code, "agent_spec_not_found");
        assert!(err.message.contains("coder"));
        assert!(err.message.contains("agents/main"));
    }

    #[test]
    fn resolve_spawn_missing_without_default() {
        let specs = vec![spec("coder")];
        let err = resolve_spawn_spec_id(&serde_json::json!({}), &specs).unwrap_err();
        assert_eq!(err.code, "agent_spec_required");
    }

    #[test]
    fn resolve_when_defaults_to_queue() {
        assert_eq!(
            resolve_when(&serde_json::json!({})).unwrap(),
            MessageWhen::Queue
        );
        assert_eq!(
            resolve_when(&serde_json::json!({ "when": "steer" })).unwrap(),
            MessageWhen::Steer
        );
        assert_eq!(
            resolve_when(&serde_json::json!({ "when": "nope" }))
                .unwrap_err()
                .code,
            "invalid_argument"
        );
    }

    #[test]
    fn catalog_value_includes_default_when_general_present() {
        let specs = vec![spec("coder"), spec("general")];
        let value = catalog_value(&specs);
        assert_eq!(value["default_spawn_spec_id"], "general");
        assert_eq!(value["specs"].as_array().unwrap().len(), 2);
        assert_eq!(value["specs"][0]["id"], "coder");
    }
}
