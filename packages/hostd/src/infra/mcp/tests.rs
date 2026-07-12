use std::collections::HashMap;

use crate::domain::config::McpServerConfig;

#[test]
fn test_mcp_server_config_deserialize() {
    let config: McpServerConfig = serde_json::from_str(
        r#"{"name": "filesystem", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"], "env": {}}"#,
    )
    .unwrap();
    assert_eq!(config.name, "filesystem");
    assert_eq!(config.command, "npx");
    assert_eq!(config.args.len(), 3);
}

#[test]
fn test_mcp_server_config_defaults() {
    let config: McpServerConfig =
        serde_json::from_str(r#"{"name": "echo", "command": "cat"}"#).unwrap();
    assert_eq!(config.args, Vec::<String>::new());
    assert_eq!(config.env, HashMap::new());
}
