//! Shared hostd stdio client: spawn, JSON-lines decode, bounded drain.
//!
//! Frontends select their bridge contract (`TuiHostBridge` or
//! `DesktopHostBridge`) and share one wire client implementation.

use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command as ProcessCommand, Stdio},
    thread,
};

use anyhow::{Context, Result};
use piko_protocol::{Command, ServerMessage};

use crate::{
    ThreadBridgeContract,
    wrappers::{ThreadBridgeReceiver, thread_bridge},
};

/// One decoded line from the host stdout reader thread.
#[derive(Debug)]
pub enum HostLine {
    Message(Box<ServerMessage>),
    DecodeError(String),
    Closed,
}

/// Spawns `command` as a child process and bridges its JSON-lines stdout to
/// the frontend over the declared thread bridge.
pub struct HostdClient<C: ThreadBridgeContract> {
    child: Child,
    stdin: ChildStdin,
    rx: ThreadBridgeReceiver<C, HostLine>,
}

impl<C: ThreadBridgeContract> HostdClient<C> {
    pub fn spawn(command: String, args: Vec<String>, log_level: Option<&str>) -> Result<Self> {
        let mut cmd = ProcessCommand::new(&command);
        cmd.args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        if let Some(level) = log_level {
            cmd.env("PIKO_LOG_LEVEL", level);
        }

        let mut child = cmd.spawn().with_context(|| {
            format!("spawn hostd command `{}`", render_command(&command, &args))
        })?;

        let stdin = child.stdin.take().context("hostd stdin unavailable")?;
        let stdout = child.stdout.take().context("hostd stdout unavailable")?;
        let (tx, rx) = thread_bridge::<C, HostLine>();

        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        let _ = tx.send(decode_host_line(&line));
                    }
                    Err(err) => {
                        let _ = tx.send(HostLine::DecodeError(err.to_string()));
                        break;
                    }
                }
            }
            let _ = tx.send(HostLine::Closed);
        });

        Ok(Self { child, stdin, rx })
    }

    pub fn send(&mut self, command: Command) -> Result<()> {
        let encoded = serde_json::to_string(&command).context("encode host command")?;
        writeln!(self.stdin, "{encoded}").context("write host command")?;
        self.stdin.flush().context("flush host command")?;
        Ok(())
    }

    /// Receive at most `limit` currently queued lines. Keeping this bounded
    /// guarantees that a continuously streaming host cannot starve the
    /// frontend loop.
    pub fn drain_up_to(&mut self, limit: usize) -> Vec<HostLine> {
        drain_up_to(&self.rx, limit)
    }

    pub fn shutdown(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl<C: ThreadBridgeContract> Drop for HostdClient<C> {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn drain_up_to<C: ThreadBridgeContract>(
    rx: &ThreadBridgeReceiver<C, HostLine>,
    limit: usize,
) -> Vec<HostLine> {
    let mut lines = Vec::with_capacity(limit);
    for _ in 0..limit {
        let Ok(line) = rx.try_recv() else {
            break;
        };
        lines.push(line);
    }
    lines
}

fn render_command(command: &str, args: &[String]) -> String {
    if args.is_empty() {
        command.to_string()
    } else {
        format!("{command} {}", args.join(" "))
    }
}

/// Decode one JSON-lines record. Public so frontend integration tests can
/// replay captured host traffic through the exact production decoder.
pub fn decode_host_line(line: &str) -> HostLine {
    let value = match serde_json::from_str::<serde_json::Value>(line) {
        Ok(value) => value,
        Err(err) => return HostLine::DecodeError(format!("{err}: {line}")),
    };

    match serde_json::from_value::<ServerMessage>(value) {
        Ok(message) => HostLine::Message(Box::new(message)),
        Err(err) => HostLine::DecodeError(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::TuiHostBridge;

    #[test]
    fn drain_preserves_backlog_past_the_frame_limit() {
        let (tx, rx) = thread_bridge::<TuiHostBridge, HostLine>();
        tx.send(HostLine::DecodeError("one".into())).unwrap();
        tx.send(HostLine::DecodeError("two".into())).unwrap();
        tx.send(HostLine::DecodeError("three".into())).unwrap();

        assert_eq!(drain_up_to(&rx, 2).len(), 2);
        assert_eq!(drain_up_to(&rx, 2).len(), 1);
    }

    #[test]
    fn parse_rejects_malformed_json_and_unknown_shapes() {
        assert!(matches!(
            decode_host_line("not json"),
            HostLine::DecodeError(_)
        ));
        assert!(matches!(
            decode_host_line(r#"{"unknown":"shape"}"#),
            HostLine::DecodeError(_)
        ));
    }
}
