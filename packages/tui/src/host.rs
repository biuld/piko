use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command as ProcessCommand, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
};

use anyhow::{Context, Result};
use piko_protocol::{Command, CommandAck, Event};

pub enum HostLine {
    Ack(CommandAck),
    Event(Event),
    DecodeError(String),
    Closed,
}

pub struct HostdClient {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<HostLine>,
}

impl HostdClient {
    pub fn spawn(command: String, args: Vec<String>) -> Result<Self> {
        let mut child = ProcessCommand::new(&command)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| {
                format!("spawn hostd command `{}`", render_command(&command, &args))
            })?;

        let stdin = child.stdin.take().context("hostd stdin unavailable")?;
        let stdout = child.stdout.take().context("hostd stdout unavailable")?;
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        let _ = tx.send(parse_host_line(&line));
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

    pub fn drain(&mut self) -> Vec<HostLine> {
        let mut lines = Vec::new();
        while let Ok(line) = self.rx.try_recv() {
            lines.push(line);
        }
        lines
    }

    pub fn shutdown(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn render_command(command: &str, args: &[String]) -> String {
    if args.is_empty() {
        command.to_string()
    } else {
        format!("{command} {}", args.join(" "))
    }
}

fn parse_host_line(line: &str) -> HostLine {
    let value = match serde_json::from_str::<serde_json::Value>(line) {
        Ok(value) => value,
        Err(err) => return HostLine::DecodeError(format!("{err}: {line}")),
    };

    match value.get("type").and_then(|value| value.as_str()) {
        Some("command_accepted") | Some("command_rejected") => {
            match serde_json::from_value::<CommandAck>(value) {
                Ok(ack) => HostLine::Ack(ack),
                Err(err) => HostLine::DecodeError(err.to_string()),
            }
        }
        Some(_) => match serde_json::from_value::<Event>(value) {
            Ok(event) => HostLine::Event(event),
            Err(err) => HostLine::DecodeError(err.to_string()),
        },
        None => HostLine::DecodeError(format!("missing type field: {line}")),
    }
}
