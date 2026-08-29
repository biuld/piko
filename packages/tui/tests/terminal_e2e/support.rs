use std::{
    io::Read,
    path::{Path, PathBuf},
    thread,
};

use piko_comms::{ThreadBridgeReceiver, contracts::TuiHostBridge, thread_bridge};
use serde_json::Value;

pub fn spawn_reader(
    mut reader: Box<dyn Read + Send>,
) -> ThreadBridgeReceiver<TuiHostBridge, Vec<u8>> {
    let (sender, receiver) = thread_bridge::<TuiHostBridge, Vec<u8>>();
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    if sender.send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    receiver
}

pub fn read_records(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

pub fn trace_summary(records: &[Value]) -> String {
    let summary = records
        .iter()
        .filter_map(|record| {
            let kind = record["kind"].as_str()?;
            let value = &record["value"];
            match kind {
                "gateway" => Some(serde_json::json!({
                    "record_kind": kind,
                    "step": value["step"],
                    "user_messages": value["user_messages"],
                })),
                "command" => Some(serde_json::json!({
                    "record_kind": kind,
                    "type": value["type"],
                    "command_id": value["command_id"],
                    "text": value["text"],
                    "message": value["message"],
                })),
                "event" => Some(serde_json::json!({
                    "record_kind": kind,
                    "type": value["type"],
                    "kind": value["kind"],
                    "command_id": value["command_id"],
                })),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&summary).unwrap_or_else(|_| "<invalid trace>".into())
}

pub fn server_message(record: &Value) -> Option<piko_protocol::ServerMessage> {
    (record["kind"].as_str() == Some("event"))
        .then(|| serde_json::from_value(record["value"].clone()).ok())
        .flatten()
}

pub fn binary_path(name: &str) -> PathBuf {
    let env_name = format!("CARGO_BIN_EXE_{}", name.replace('-', "_"));
    std::env::var_os(env_name)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/debug")
                .join(name)
        })
}

pub fn temp_path(prefix: &str, suffix: u128) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{suffix}"))
}

pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

pub fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos()
}
