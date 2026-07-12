//! MCP: Model Context Protocol integration.
//!
//! Connects to MCP servers via stdio, discovers tools, and registers
//! them as ToolProvider implementations for orchd's ToolRegistry.
//!
//! Protocol: JSON-RPC 2.0 over stdin/stdout (newline-delimited).

mod init;
mod provider;
mod types;

#[cfg(test)]
mod tests;

pub use init::initialize_mcp_tools;
pub use provider::McpProvider;
