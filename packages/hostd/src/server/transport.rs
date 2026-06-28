use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::unbounded_channel;

use crate::api::{Command, CommandAck};
use crate::session::JsonlSessionRepository;
use crate::settings::SettingsManager;
use crate::turn::runner::{ErrorTurnRunner, TurnRunner};

use super::{HostServer, build_orch_turn_runner};

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
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line).await?;
        if read == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let parsed = serde_json::from_str::<Command>(line.trim());
        let mut events = match parsed {
            Ok(command) => {
                let command_id = command.command_id().to_string();
                write_ack(&mut writer, CommandAck::CommandAccepted { command_id }).await?;
                server.handle_command_stream(command)
            }
            Err(err) => {
                let (_tx, rx) = unbounded_channel();
                write_ack(
                    &mut writer,
                    CommandAck::CommandRejected {
                        command_id: "unknown".to_string(),
                        reason: format!("invalid json command: {err}"),
                    },
                )
                .await?;
                rx
            }
        };
        while let Some(event) = events.recv().await {
            let encoded = serde_json::to_string(&event)?;
            writer.write_all(encoded.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }
    }
    Ok(())
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
