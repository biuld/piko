# Design Doc: Unified JSON-based Config Update & Hook Architecture

## Status vs Goal

Currently, the TUI configures host settings by sending a strongly-typed `Command::ConfigSet` containing multiple optional fields corresponding to individual configurations. This binds TUI settings actions tightly to the protocol schema, requiring changes across multiple crates whenever a new configuration field is introduced.

The goal is to unify all configuration updates into a single generic `Command::ConfigUpdate` taking a JSON payload representing a JSON Merge Patch (RFC 7386). The host daemon (`hostd`) applies this patch dynamically to its settings, validates the resulting configuration, persists it to disk, and triggers business-logic observer hooks to emit downstream events (such as `Event::ModelConfigChanged` or `Event::ConfigEntry`).

## Protocol Changes

In `packages/protocol/src/command.rs`, we replace the old `Command::ConfigSet` command:

```rust
pub enum Command {
    // ...
    ConfigUpdate {
        command_id: CommandId,
        patch: serde_json::Value,
    },
    // ...
}
```

## Hostd Processing & Observer Hooks

When `hostd` receives `Command::ConfigUpdate`, it performs the following steps:

1. **JSON Merging**: Merges the incoming `patch` value into the current configuration JSON representation.
2. **Type Validation**: Deserializes the merged JSON value into a `HostSettings` struct, validating field types and structure.
3. **Persistence**: Serializes the updated `HostSettings` back to TOML format and saves it to the project or global settings file.
4. **Observer Hooks**: Compares the old configuration state with the new configuration state and runs hooks if changes are detected:
   - **Model Change Hook**: If `default_model` or `default_provider` changes, updates active LLM client runners and returns `Event::ModelConfigChanged`.
   - **TUI Config Hook**: If namespaced `tui` settings change, returns `Event::ConfigEntry` for the `tui` namespace.

```rust
fn merge_json(base: &mut serde_json::Value, patch: &serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(patch_map)) => {
            for (k, v) in patch_map {
                if v.is_null() {
                    base_map.remove(k);
                } else {
                    merge_json(base_map.entry(k.clone()).or_insert(serde_json::Value::Null), v);
                }
            }
        }
        (base, patch) => {
            *base = patch.clone();
        }
    }
}
```

## TUI Side Changes

TUI constructs a JSON merge patch instead of a strongly-typed `ConfigSet` command.

For settings updates:
```rust
fn json_patch_for_action(action: SettingsAction) -> serde_json::Value {
    match action {
        SettingsAction::Thinking(level) => {
            serde_json::json!({
                "default-thinking-level": level
            })
        }
        SettingsAction::HideThinking(value) => {
            serde_json::json!({
                "hide-thinking-block": value
            })
        }
        SettingsAction::Compaction(value) => {
            serde_json::json!({
                "compaction": {
                    "enabled": value
                }
            })
        }
        SettingsAction::Theme(value) => {
            serde_json::json!({
                "theme": value
            })
        }
        SettingsAction::Transport(value) => {
            serde_json::json!({
                "transport": value
            })
        }
        SettingsAction::DisableTools => {
            serde_json::json!({
                "active-tool-names": []
            })
        }
    }
}
```

For model selection updates:
```rust
let patch = serde_json::json!({
    "default-provider": provider,
    "default-model": model_id
});
```

This guarantees TUI sends a clean JSON merge patch payload, and hostd applies it and manages side-effects via observer hooks, returning the original event stream to preserve TUI display logic.
