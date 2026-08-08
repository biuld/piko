use super::*;

#[async_trait]
impl ToolProvider for McpProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Mcp
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        self.tools.clone()
    }

    async fn execute(&self, call: ToolCall, _context: ToolExecutionContext) -> ToolExecResult {
        let tool_name = call.name.clone();
        let arguments = call.arguments.clone();

        match self
            .rpc_call(
                "tools/call",
                Some(serde_json::json!({
                    "name": tool_name,
                    "arguments": arguments
                })),
            )
            .await
        {
            Ok(result) => {
                // Parse MCP call result
                let call_result: Result<McpCallToolResult, _> =
                    serde_json::from_value(result.clone());

                match call_result {
                    Ok(cr) => {
                        let text = cr
                            .content
                            .iter()
                            .filter(|c| c.content_type == "text")
                            .map(|c| c.text.as_str())
                            .collect::<Vec<_>>()
                            .join("\n");

                        if cr.is_error {
                            let msg = text.clone();
                            ToolExecResult {
                                ok: false,
                                value: Some(serde_json::Value::String(text)),
                                error: Some(ToolExecError {
                                    code: "mcp_tool_error".into(),
                                    message: msg,
                                    retryable: Some(false),
                                }),
                            }
                        } else {
                            ToolExecResult {
                                ok: true,
                                value: Some(serde_json::Value::String(text)),
                                error: None,
                            }
                        }
                    }
                    Err(_) => ToolExecResult {
                        ok: true,
                        value: Some(result),
                        error: None,
                    },
                }
            }
            Err(e) => ToolExecResult {
                ok: false,
                value: None,
                error: Some(ToolExecError {
                    code: "mcp_error".into(),
                    message: e,
                    retryable: Some(true),
                }),
            },
        }
    }
}
