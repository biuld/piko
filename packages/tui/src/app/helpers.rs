use piko_protocol::{Command, ProviderInfo, SessionTreeEntry};

use crate::features::{model_selector::ModelOption, settings::SettingsAction};

pub fn command_id() -> String {
    format!("tui-{}", uuid::Uuid::new_v4())
}

pub fn get_active_branch_entries(
    entries: &[SessionTreeEntry],
    current_leaf_id: Option<&str>,
) -> Vec<SessionTreeEntry> {
    piko_client_core::active_branch_entries(entries, current_leaf_id)
}

pub(crate) fn flatten_models(providers: Vec<ProviderInfo>) -> Vec<ModelOption> {
    providers
        .into_iter()
        .flat_map(|provider| {
            provider.models.into_iter().map(move |model| ModelOption {
                provider: provider.provider.clone(),
                id: model.id,
                name: model.name,
                has_auth: provider.has_auth,
                reasoning_efforts: model.reasoning_efforts,
            })
        })
        .collect()
}

pub(crate) fn config_command_for_setting(action: SettingsAction) -> Command {
    let patch = match action {
        SettingsAction::Thinking(level) => {
            serde_json::json!({
                "default-thinking-level": level
            })
        }
        SettingsAction::HideThinking(value) => {
            // TUI-only presentation; lives under `[tui]`.
            serde_json::json!({
                "tui": { "hide_thinking_block": value }
            })
        }
        SettingsAction::Compaction(value) => {
            serde_json::json!({
                "compaction": {
                    "enabled": value
                }
            })
        }
        SettingsAction::CompactionKeep(value) => {
            serde_json::json!({
                "compaction": {
                    "keep-recent-tokens": value
                }
            })
        }
        SettingsAction::CompactionReserve(value) => {
            serde_json::json!({
                "compaction": {
                    "reserve-tokens": value
                }
            })
        }
        SettingsAction::CompactionMinGrowthFraction(value) => {
            serde_json::json!({ "compaction": { "min-growth-fraction": value } })
        }
        SettingsAction::TranscriptMaxToolOutput(value) => {
            serde_json::json!({ "transcript": { "max-tool-output-tokens": value } })
        }
        SettingsAction::Theme(value) => {
            // Theme is TUI presentation; lives under `[tui].theme.name`.
            serde_json::json!({
                "tui": { "theme": { "name": value } }
            })
        }
        SettingsAction::Transport(value) => {
            serde_json::json!({
                "transport": value
            })
        }
        SettingsAction::Retry(value) => {
            serde_json::json!({
                "retry": {
                    "enabled": value
                }
            })
        }
        SettingsAction::RetryMaxRetries(value) => {
            serde_json::json!({ "retry": { "max-retries": value } })
        }
        SettingsAction::RetryBaseDelay(value) => {
            serde_json::json!({ "retry": { "base-delay-ms": value } })
        }
        SettingsAction::RetryMaxDelay(value) => {
            serde_json::json!({ "retry": { "max-delay-ms": value } })
        }
        SettingsAction::RetryBudget(value) => {
            serde_json::json!({ "retry": { "budget-ms": value } })
        }
        SettingsAction::ApprovalTimeout(value) => {
            serde_json::json!({ "approvals": { "timeout-secs": value } })
        }
        SettingsAction::Guardian(value) => {
            serde_json::json!({ "guardian": { "enabled": value } })
        }
        SettingsAction::GuardianTimeout(value) => {
            serde_json::json!({ "guardian": { "timeout-secs": value } })
        }
        SettingsAction::GuardianMaxDenials(value) => {
            serde_json::json!({ "guardian": { "max-consecutive-denials": value } })
        }
        SettingsAction::SafeWorkspaceWrites(value) => {
            serde_json::json!({ "safety": { "auto-approve-workspace-writes": value } })
        }
        SettingsAction::PermissionProfile(value) => {
            serde_json::json!({ "permissions": { "profile": value } })
        }
        SettingsAction::Feature(key, value) => {
            serde_json::json!({ "features": { (key): value } })
        }
        SettingsAction::McpConnectTimeout(value) => {
            serde_json::json!({ "mcp": { "connect-timeout-ms": value } })
        }
        SettingsAction::PromptCache(value) => {
            serde_json::json!({ "prompt": { "cache-policy": value } })
        }
        SettingsAction::Observability(value) => {
            serde_json::json!({
                "observability": {
                    "enabled": value
                }
            })
        }
        SettingsAction::ObservabilityEndpoint(endpoint) => {
            serde_json::json!({
                "observability": {
                    "otel-endpoint": endpoint
                }
            })
        }
        SettingsAction::EditorMultiline(value) => {
            serde_json::json!({ "tui": { "editor": { "multiline": value } } })
        }
        SettingsAction::EditorAutoResize(value) => {
            serde_json::json!({ "tui": { "editor": { "autoResize": value } } })
        }
        SettingsAction::EditorMaxLines(value) => {
            serde_json::json!({ "tui": { "editor": { "maxLines": value } } })
        }
        SettingsAction::EditorHistoryLimit(value) => {
            serde_json::json!({ "tui": { "editor": { "historyLimit": value } } })
        }
        SettingsAction::TreeFilter(value) => {
            serde_json::json!({ "tui": { "tree": { "filter_mode": value } } })
        }
        SettingsAction::BottomBarPreset(value) => {
            let items = match value {
                "compact" => vec!["agent", "model", "context"],
                "minimal" => vec!["agent", "model"],
                _ => vec!["agent", "model", "cwd", "context", "cost"],
            };
            serde_json::json!({ "tui": { "bottom_bar": { "items": items } } })
        }
        SettingsAction::EnableAllTools => {
            serde_json::json!({
                "active-tool-names": serde_json::Value::Null
            })
        }
        SettingsAction::DisableTools => {
            serde_json::json!({
                "active-tool-names": []
            })
        }
    };
    Command::ConfigUpdate {
        command_id: command_id(),
        patch,
    }
}
