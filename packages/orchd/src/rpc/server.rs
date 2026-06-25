// ---- RPC: server — JSON-RPC 2.0 over stdio transport ----

use std::io::{BufRead, Write};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{error, info, warn};

use crate::orchestrator::core::OrchCore;

// ---- JSON-RPC types ----

/// JSON-RPC 2.0 request.
#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    pub id: Option<Value>,
}

/// JSON-RPC 2.0 success response.
#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
    pub id: Option<Value>,
}

/// JSON-RPC 2.0 error.
#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

// ---- JSON-RPC standard error codes ----

const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;

// ---- Server ----

/// Run the JSON-RPC server on stdio.
///
/// Reads one JSON-RPC request per line from stdin, routes to the
/// orchestrator via `handlers::dispatch`, and writes one JSON-RPC
/// response per line to stdout. Blocks until stdin is closed.
pub async fn run_stdio_server(orch: Arc<OrchCore>) {
    info!("orchd JSON-RPC server starting on stdio");

    let stdin = std::io::stdin();
    let reader = std::io::BufReader::new(stdin.lock());

    for line in reader.lines() {
        match line {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => {
                let response = handle_request(&orch, &line).await;
                let mut out = std::io::stdout().lock();
                let json = serde_json::to_string(&response).unwrap_or_else(|e| {
                    error!("Failed to serialize response: {e}");
                    r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"},"id":null}"#.to_string()
                });
                if writeln!(out, "{json}").is_err() {
                    warn!("stdout write failed, shutting down");
                    break;
                }
                let _ = out.flush();
            }
            Err(e) => {
                warn!("stdin read error: {e}");
                break;
            }
        }
    }

    info!("orchd JSON-RPC server shutting down");
}

async fn handle_request(orch: &Arc<OrchCore>, line: &str) -> RpcResponse {
    // Parse request
    let request: RpcRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            return RpcResponse {
                jsonrpc: "2.0",
                result: None,
                error: Some(RpcError {
                    code: PARSE_ERROR,
                    message: format!("Parse error: {e}"),
                }),
                id: None,
            };
        }
    };

    // Validate jsonrpc version
    if request.jsonrpc != "2.0" {
        return RpcResponse {
            jsonrpc: "2.0",
            result: None,
            error: Some(RpcError {
                code: INVALID_REQUEST,
                message: "Invalid Request: jsonrpc must be \"2.0\"".into(),
            }),
            id: request.id,
        };
    }

    // Route to handler
    match super::handlers::dispatch(orch, None, &request.method, &request.params).await {
        Ok(result) => RpcResponse {
            jsonrpc: "2.0",
            result: Some(result),
            error: None,
            id: request.id,
        },
        Err(err) => {
            let code = match err.as_str() {
                s if s.starts_with("Invalid params") => INVALID_PARAMS,
                s if s.starts_with("Method not found") => METHOD_NOT_FOUND,
                _ => INTERNAL_ERROR,
            };
            RpcResponse {
                jsonrpc: "2.0",
                result: None,
                error: Some(RpcError { code, message: err }),
                id: request.id,
            }
        }
    }
}
