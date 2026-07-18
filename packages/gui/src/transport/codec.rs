//! JSONL codec: encode `Command` to a line and decode `ServerMessage` from a line.

use piko_protocol::{Command, ServerMessage};

/// Serialize a `Command` into a single JSON line (no trailing newline).
pub fn encode_command(command: &Command) -> Result<String, serde_json::Error> {
    serde_json::to_string(command)
}

/// Attempt to decode a single stdout line into a `ServerMessage`.
pub fn decode_server_message(line: &str) -> Result<ServerMessage, DecodeError> {
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|e| DecodeError::InvalidJson {
            source: e.to_string(),
            line: line.to_owned(),
        })?;

    serde_json::from_value(value).map_err(|e| DecodeError::UnknownSchema {
        source: e.to_string(),
    })
}

/// Errors that can occur while decoding a hostd stdout line.
#[derive(Debug, Clone, PartialEq)]
pub enum DecodeError {
    /// The line is not valid JSON at all.
    InvalidJson { source: String, line: String },
    /// Valid JSON but does not match the `ServerMessage` schema.
    UnknownSchema { source: String },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson { source, line } => {
                write!(f, "invalid JSON: {source} (line: {line})")
            }
            Self::UnknownSchema { source } => {
                write!(f, "unknown message schema: {source}")
            }
        }
    }
}

impl std::error::Error for DecodeError {}
