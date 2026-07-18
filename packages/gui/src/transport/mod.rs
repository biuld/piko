//! hostd process lifecycle and JSONL stdin/stdout codec.
//!
//! This module owns:
//! - Resolving the hostd executable path (see [`discovery`]).
//! - Spawning hostd as a child process with piped stdin/stdout.
//! - Encoding `Command` as one JSON line per write to stdin.
//! - A blocking reader thread that decodes stdout lines and sends
//!   [`TransportEvent`] values through an `mpsc` channel. The reader never
//!   mutates shared Client Core state.
//! - Deterministic shutdown: close stdin, wait for child, avoid orphans on Drop.

pub mod codec;
pub mod discovery;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command as ProcessCommand, Stdio};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};
use piko_protocol::{Command, ServerMessage};

use self::codec::DecodeError;

// ─── Transport events ────────────────────────────────────────────────────────

/// Observations produced by the reader thread for later delivery to Client Core.
/// These are GUI-local types; they do not depend on Client Core or GPUI.
#[derive(Debug)]
pub enum TransportEvent {
    /// A successfully decoded host message.
    Message(Box<ServerMessage>),
    /// A line was received but could not be decoded.
    DecodeFailed(DecodeError),
    /// The stdout stream ended (EOF) or the child process exited.
    Closed,
}

type EventTx = piko_comms::ThreadBridgeSender<piko_comms::contracts::GuiHostBridge, TransportEvent>;
type EventRx =
    piko_comms::ThreadBridgeReceiver<piko_comms::contracts::GuiHostBridge, TransportEvent>;

// ─── HostTransport ───────────────────────────────────────────────────────────

/// Manages the hostd child process and provides send/receive over JSONL stdio.
pub struct HostTransport {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: EventRx,
    reader_handle: Option<JoinHandle<()>>,
}

impl HostTransport {
    /// Spawn hostd using the resolved command and given extra arguments.
    ///
    /// Environment variables forwarded:
    /// - `PIKO_LOG_DISABLE`, `PIKO_LOG_FILE`, `PIKO_LOG_LEVEL` (when provided).
    pub fn spawn(args: &[String], env_overrides: &[(&str, &str)]) -> Result<Self> {
        let command = discovery::resolve_hostd_command();
        Self::spawn_command(&command, args, env_overrides)
    }

    /// Lower-level spawn accepting an explicit command path (useful for tests
    /// that inject a mock binary).
    pub fn spawn_command(
        command: &str,
        args: &[String],
        env_overrides: &[(&str, &str)],
    ) -> Result<Self> {
        let mut cmd = ProcessCommand::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        for &(key, val) in env_overrides {
            cmd.env(key, val);
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawn hostd: `{} {}`", command, args.join(" ")))?;

        let stdin = child.stdin.take().context("hostd stdin unavailable")?;
        let stdout = child.stdout.take().context("hostd stdout unavailable")?;

        let (tx, rx) =
            piko_comms::thread_bridge::<piko_comms::contracts::GuiHostBridge, TransportEvent>();

        let reader_handle = thread::spawn(move || {
            run_reader(stdout, tx);
        });

        Ok(Self {
            child,
            stdin: Some(stdin),
            rx,
            reader_handle: Some(reader_handle),
        })
    }

    /// Serialize and write a `Command` as one JSON line to hostd stdin.
    pub fn send(&mut self, command: &Command) -> Result<()> {
        let stdin = self.stdin.as_mut().context("transport already shut down")?;
        let encoded = codec::encode_command(command).context("encode command")?;
        writeln!(stdin, "{encoded}").context("write to hostd stdin")?;
        stdin.flush().context("flush hostd stdin")?;
        Ok(())
    }

    /// Non-blocking drain of all currently available transport events.
    pub fn drain(&self) -> Vec<TransportEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.rx.try_recv() {
            events.push(event);
        }
        events
    }

    /// Blocking receive of the next transport event.
    #[allow(dead_code)] // public API for interactive / smoke harnesses
    pub fn recv(&self) -> Option<TransportEvent> {
        self.rx.recv().ok()
    }

    /// Shut down the transport: close stdin, kill child, join reader thread.
    ///
    /// Returns the child's exit status when reaping succeeds.
    pub fn shutdown(&mut self) -> Option<std::process::ExitStatus> {
        self.stdin.take();
        let _ = self.child.kill();
        let status = self.child.wait().ok();
        if let Some(handle) = self.reader_handle.take() {
            let _ = handle.join();
        }
        status
    }
}

impl Drop for HostTransport {
    fn drop(&mut self) {
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(handle) = self.reader_handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_reader(stdout: impl std::io::Read, tx: EventTx) {
    let reader = BufReader::new(stdout);
    for line_result in reader.lines() {
        let event = match line_result {
            Ok(line) => match codec::decode_server_message(&line) {
                Ok(msg) => TransportEvent::Message(Box::new(msg)),
                Err(err) => TransportEvent::DecodeFailed(err),
            },
            Err(_io_err) => break,
        };
        if tx.send(event).is_err() {
            break;
        }
    }
    let _ = tx.send(TransportEvent::Closed);
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;

#[cfg(test)]
mod hostd_smoke;
