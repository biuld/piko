use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

enum OutboundLine {
    ServerMessage(String),
    Done,
}

enum InboundLine {
    Command(Command),
    Rejected(Box<ServerMessage>),
    Closed,
}

use crate::api::{Command, ServerMessage};
use crate::domain::config::SettingsManager;
use crate::domain::turns::{ErrorTurnRunner, TurnRunner};
use crate::infra::storage::JsonlSessionRepository;

use crate::protocol::{HostServer, build_orch_turn_runner};

pub async fn run_stdio_server() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let cwd = std::env::current_dir()?;
    let settings = SettingsManager::create(&cwd)?;
    let session_root = settings
        .settings()
        .session_dir
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(JsonlSessionRepository::default_root);
    let (turn_runner, model_executor) = build_orch_turn_runner(&settings.settings())
        .await
        .unwrap_or_else(|e| {
            (
                Arc::new(ErrorTurnRunner::new(e)) as Arc<dyn TurnRunner>,
                None,
            )
        });
    let server = HostServer::with_storage_runner_settings(
        JsonlSessionRepository::new(session_root),
        turn_runner,
        settings.settings(),
    );
    server
        .project_settings_path
        .lock()
        .await
        .replace(cwd.join(".piko").join("settings.toml"));
    if let Some(executor) = model_executor {
        server.set_model_executor(executor).await;
    }
    run_jsonl_server(BufReader::new(stdin), stdout, server).await
}

pub async fn run_jsonl_server<R, W>(
    reader: R,
    mut writer: W,
    server: HostServer,
) -> Result<(), Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin,
{
    let (event_tx, mut event_rx) = unbounded_channel::<OutboundLine>();
    let (command_tx, mut command_rx) = unbounded_channel::<InboundLine>();
    read_commands(reader, command_tx);
    let mut input_closed = false;
    let mut active_streams = 0usize;
    loop {
        if input_closed && active_streams == 0 {
            break;
        }
        tokio::select! {
            maybe_outbound = event_rx.recv(), if active_streams > 0 => {
                let Some(outbound) = maybe_outbound else {
                    active_streams = 0;
                    continue;
                };
                match outbound {
                    OutboundLine::ServerMessage(encoded) => {
                        writer.write_all(encoded.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;
                    }
                    OutboundLine::Done => {
                        active_streams = active_streams.saturating_sub(1);
                    }
                }
            }
            inbound = command_rx.recv(), if !input_closed => {
                match inbound {
                    Some(InboundLine::Command(command)) => {
                        let command_id = command.command_id().to_string();
                        write_ack(&mut writer, ServerMessage::CommandAccepted { command_id }).await?;
                        active_streams += 1;
                        forward_events(server.handle_command_stream(command), event_tx.clone());
                    }
                    Some(InboundLine::Rejected(message)) => {
                        write_ack(&mut writer, *message).await?;
                    }
                    Some(InboundLine::Closed) | None => input_closed = true,
                }
            }
        }
    }
    Ok(())
}

fn read_commands<R>(mut reader: R, command_tx: UnboundedSender<InboundLine>)
where
    R: AsyncBufRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            let read = match reader.read_line(&mut line).await {
                Ok(read) => read,
                Err(err) => {
                    let _ =
                        command_tx.send(InboundLine::Rejected(Box::new(ServerMessage::CommandRejected {
                            command_id: "unknown".to_string(),
                            reason: format!("read command: {err}"),
                        })));
                    break;
                }
            };
            if read == 0 {
                break;
            }
            let raw = line.trim();
            if raw.is_empty() {
                continue;
            }
            match serde_json::from_str::<Command>(raw) {
                Ok(command) => {
                    if command_tx.send(InboundLine::Command(command)).is_err() {
                        return;
                    }
                }
                Err(err) => {
                    if command_tx
                        .send(InboundLine::Rejected(Box::new(ServerMessage::CommandRejected {
                            command_id: "unknown".to_string(),
                            reason: format!("invalid json command: {err}"),
                        })))
                        .is_err()
                    {
                        return;
                    }
                }
            }
        }
        let _ = command_tx.send(InboundLine::Closed);
    });
}

fn forward_events(
    mut events: tokio::sync::mpsc::UnboundedReceiver<crate::api::ServerMessage>,
    event_tx: UnboundedSender<OutboundLine>,
) {
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            let Ok(encoded) = serde_json::to_string(&event) else {
                continue;
            };
            if event_tx.send(OutboundLine::ServerMessage(encoded)).is_err() {
                break;
            }
        }
        let _ = event_tx.send(OutboundLine::Done);
    });
}

async fn write_ack<W>(writer: &mut W, ack: ServerMessage) -> Result<(), Box<dyn std::error::Error>>
where
    W: AsyncWrite + Unpin,
{
    let encoded = serde_json::to_string(&ack)?;
    writer.write_all(encoded.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}
