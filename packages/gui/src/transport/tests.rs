//! Unit tests for the transport layer.
//!
//! These tests validate JSONL codec behavior and transport event semantics
//! without spawning a real hostd process or requiring GPUI.

use std::io::{BufRead, BufReader, Cursor};
use std::thread;

use piko_protocol::{Command, ServerMessage, SessionListScope};

use super::TransportEvent;
use super::codec::{self, DecodeError};

fn event_bridge() -> (
    piko_comms::ThreadBridgeSender<piko_comms::contracts::GuiHostBridge, TransportEvent>,
    piko_comms::ThreadBridgeReceiver<piko_comms::contracts::GuiHostBridge, TransportEvent>,
) {
    piko_comms::thread_bridge::<piko_comms::contracts::GuiHostBridge, TransportEvent>()
}

fn pump_reader(
    input: impl std::io::Read + Send + 'static,
    tx: piko_comms::ThreadBridgeSender<piko_comms::contracts::GuiHostBridge, TransportEvent>,
) {
    thread::spawn(move || {
        let reader = BufReader::new(input);
        for line_result in reader.lines() {
            let event = match line_result {
                Ok(line) => match codec::decode_server_message(&line) {
                    Ok(msg) => TransportEvent::Message(Box::new(msg)),
                    Err(err) => TransportEvent::DecodeFailed(err),
                },
                Err(_) => break,
            };
            if tx.send(event).is_err() {
                break;
            }
        }
        let _ = tx.send(TransportEvent::Closed);
    });
}

#[test]
fn encode_command_produces_single_json_line() {
    let cmd = Command::SessionList {
        command_id: "cmd-1".to_string(),
        scope: SessionListScope::All,
        cwd: Some("/tmp".to_string()),
    };
    let encoded = codec::encode_command(&cmd).unwrap();

    assert!(
        !encoded.contains('\n'),
        "encoded line must not contain newline"
    );

    let decoded: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded["type"], "session_list");
    assert_eq!(decoded["command_id"], "cmd-1");
    assert_eq!(decoded["cwd"], "/tmp");
}

#[test]
fn encode_decode_command_roundtrip() {
    let cmd = Command::ChatSubmit {
        command_id: "cmd-42".to_string(),
        session_id: "sess-1".to_string(),
        target_agent_instance_id: "agent-1".to_string(),
        text: "hello world".to_string(),
    };
    let encoded = codec::encode_command(&cmd).unwrap();
    let decoded: Command = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, cmd);
}

#[test]
fn decode_valid_command_response() {
    let json = r#"{"kind":"command_response","command_id":"c1","result":{"Ok":{"type":"empty"}}}"#;
    let msg = codec::decode_server_message(json).unwrap();
    match msg {
        ServerMessage::CommandResponse { command_id, .. } => {
            assert_eq!(command_id, "c1");
        }
        other => panic!("unexpected variant: {other:?}"),
    }
}

#[test]
fn decode_invalid_json_returns_error() {
    let line = "not json at all {{{";
    let result = codec::decode_server_message(line);
    match result {
        Err(DecodeError::InvalidJson { line: l, .. }) => {
            assert_eq!(l, line);
        }
        other => panic!("expected InvalidJson, got {other:?}"),
    }
}

#[test]
fn decode_unknown_schema_returns_error() {
    let line = r#"{"kind":"totally_unknown_kind","data":123}"#;
    let result = codec::decode_server_message(line);
    match result {
        Err(DecodeError::UnknownSchema { .. }) => {}
        other => panic!("expected UnknownSchema, got {other:?}"),
    }
}

#[test]
fn reader_thread_sends_closed_on_eof() {
    let input = Cursor::new(Vec::<u8>::new());
    let (tx, rx) = event_bridge();
    pump_reader(input, tx);

    let event = rx.recv().unwrap();
    assert!(matches!(event, TransportEvent::Closed));
}

#[test]
fn reader_thread_sends_message_then_decode_failure() {
    let lines = concat!(
        r#"{"kind":"command_response","command_id":"c1","result":{"Ok":{"type":"empty"}}}"#,
        "\n",
        "broken json\n",
    );
    let input = Cursor::new(lines.as_bytes().to_vec());
    let (tx, rx) = event_bridge();
    pump_reader(input, tx);

    let e1 = rx.recv().unwrap();
    assert!(
        matches!(e1, TransportEvent::Message(_)),
        "first event should be Message"
    );

    let e2 = rx.recv().unwrap();
    assert!(
        matches!(e2, TransportEvent::DecodeFailed(_)),
        "second event should be DecodeFailed"
    );

    let e3 = rx.recv().unwrap();
    assert!(
        matches!(e3, TransportEvent::Closed),
        "third event should be Closed after EOF"
    );
}

#[test]
fn reader_thread_decode_failure_then_valid_line() {
    let lines = concat!(
        "not-json\n",
        r#"{"kind":"command_response","command_id":"c2","result":{"Ok":{"type":"empty"}}}"#,
        "\n",
    );
    let input = Cursor::new(lines.as_bytes().to_vec());
    let (tx, rx) = event_bridge();
    pump_reader(input, tx);

    let e1 = rx.recv().unwrap();
    assert!(matches!(e1, TransportEvent::DecodeFailed(_)));

    let e2 = rx.recv().unwrap();
    assert!(
        matches!(e2, TransportEvent::Message(_)),
        "reader must continue after a bad line"
    );

    let e3 = rx.recv().unwrap();
    assert!(matches!(e3, TransportEvent::Closed));
}

#[test]
fn hostd_shutdown_reaps_child_process() {
    use std::path::Path;

    use crate::transport::HostTransport;
    use crate::transport::discovery::resolve_hostd_command;

    let cmd = resolve_hostd_command();
    if !Path::new(&cmd).is_file() {
        eprintln!("skip process cleanup: hostd not found ({cmd})");
        return;
    }

    let mut transport =
        HostTransport::spawn(&[], &[("PIKO_LOG_DISABLE", "1")]).expect("spawn hostd");
    let status = transport
        .shutdown()
        .expect("child should be reaped on shutdown");
    let _ = status;
}

#[test]
fn reader_thread_stops_when_receiver_dropped() {
    let lines = concat!(
        r#"{"kind":"command_response","command_id":"c1","result":{"Ok":{"type":"empty"}}}"#,
        "\n",
        r#"{"kind":"command_response","command_id":"c2","result":{"Ok":{"type":"empty"}}}"#,
        "\n",
    );
    let input = Cursor::new(lines.as_bytes().to_vec());
    let (tx, rx) = event_bridge();
    drop(rx);

    let handle = thread::spawn(move || {
        let reader = BufReader::new(input);
        for line_result in reader.lines() {
            let event = match line_result {
                Ok(line) => match codec::decode_server_message(&line) {
                    Ok(msg) => TransportEvent::Message(Box::new(msg)),
                    Err(err) => TransportEvent::DecodeFailed(err),
                },
                Err(_) => break,
            };
            if tx.send(event).is_err() {
                break;
            }
        }
    });

    handle.join().expect("reader thread should exit cleanly");
}
