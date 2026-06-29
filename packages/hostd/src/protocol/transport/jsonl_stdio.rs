use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

enum OutboundLine {
    Event(String),
    Done,
}

use crate::api::{Command, CommandAck};
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
    mut reader: R,
    mut writer: W,
    server: HostServer,
) -> Result<(), Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let (event_tx, mut event_rx) = unbounded_channel::<OutboundLine>();
    let mut line = String::new();
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
                    OutboundLine::Event(encoded) => {
                        writer.write_all(encoded.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;
                    }
                    OutboundLine::Done => {
                        active_streams = active_streams.saturating_sub(1);
                    }
                }
            }
            read = reader.read_line(&mut line), if !input_closed => {
                let read = read?;
                if read == 0 {
                    input_closed = true;
                    continue;
                }
                let raw = line.trim().to_string();
                line.clear();
                if raw.is_empty() {
                    continue;
                }
                let parsed = serde_json::from_str::<Command>(&raw);
                match parsed {
                    Ok(command) => {
                        let command_id = command.command_id().to_string();
                        write_ack(&mut writer, CommandAck::CommandAccepted { command_id }).await?;
                        active_streams += 1;
                        forward_events(server.handle_command_stream(command), event_tx.clone());
                    }
                    Err(err) => {
                        write_ack(
                            &mut writer,
                            CommandAck::CommandRejected {
                                command_id: "unknown".to_string(),
                                reason: format!("invalid json command: {err}"),
                            },
                        )
                        .await?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn forward_events(
    mut events: tokio::sync::mpsc::UnboundedReceiver<crate::api::Event>,
    event_tx: UnboundedSender<OutboundLine>,
) {
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            let Ok(encoded) = serde_json::to_string(&event) else {
                continue;
            };
            if event_tx.send(OutboundLine::Event(encoded)).is_err() {
                break;
            }
        }
        let _ = event_tx.send(OutboundLine::Done);
    });
}

async fn write_ack<W>(writer: &mut W, ack: CommandAck) -> Result<(), Box<dyn std::error::Error>>
where
    W: AsyncWrite + Unpin,
{
    let encoded = serde_json::to_string(&ack)?;
    writer.write_all(encoded.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}
