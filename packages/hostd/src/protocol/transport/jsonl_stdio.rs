use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::api::{Command, ServerMessage};
use crate::domain::config::SettingsManager;
use crate::infra::storage::JsonlSessionRepository;
use crate::ports::{AgentRunRunner, ErrorAgentRunRunner};

use crate::protocol::{HostServer, build_orch_turn_runner};

const MAX_IN_FLIGHT_COMMANDS: usize = 64;

pub async fn run_stdio_server() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let cwd = std::env::current_dir()?;
    let settings = SettingsManager::create(&cwd)?;
    let session_root = std::env::var("PIKO_SESSION_DIR")
        .ok()
        .map(PathBuf::from)
        .or_else(|| settings.settings().session_dir.clone().map(PathBuf::from))
        .unwrap_or_else(JsonlSessionRepository::default_root);
    let (turn_runner, model_executor) = build_orch_turn_runner(&settings.settings())
        .await
        .unwrap_or_else(|e| {
            (
                Arc::new(ErrorAgentRunRunner::new(e)) as Arc<dyn AgentRunRunner>,
                None,
            )
        });
    let server = HostServer::with_storage_runner_settings(
        JsonlSessionRepository::new(session_root),
        turn_runner,
        settings.settings(),
    );
    let target_settings_path = if settings.project_path().exists() {
        settings.project_path().to_path_buf()
    } else {
        settings.global_path().to_path_buf()
    };
    server
        .project_settings_path
        .lock()
        .await
        .replace(target_settings_path);
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
    let (event_tx, mut event_rx) =
        piko_comms::mailbox::<piko_comms::contracts::HostCommandOutput, ServerMessage>();
    let mut reader = reader;
    let mut line = String::new();
    let mut command_tasks = tokio::task::JoinSet::new();
    let mut input_closed = false;
    loop {
        if input_closed && command_tasks.is_empty() && event_rx.is_empty() {
            break;
        }
        tokio::select! {
            maybe_outbound = event_rx.recv() => {
                let Some(outbound) = maybe_outbound else { continue };
                write_ack(&mut writer, outbound).await?;
            }
            read = reader.read_line(&mut line), if !input_closed => {
                match read {
                    Ok(0) => input_closed = true,
                    Ok(_) => {
                        let raw = line.trim();
                        if raw.is_empty() {
                            line.clear();
                            continue;
                        }
                        match serde_json::from_str::<Command>(raw) {
                            Ok(command) => {
                                if command_tasks.len() >= MAX_IN_FLIGHT_COMMANDS {
                                    write_ack(
                                        &mut writer,
                                        ServerMessage::CommandResponse {
                                            command_id: command.command_id().to_string(),
                                            result: Err(format!(
                                                "host command overload: maximum {MAX_IN_FLIGHT_COMMANDS} in flight"
                                            )),
                                        },
                                    )
                                    .await?;
                                    line.clear();
                                    continue;
                                }
                                let command_server = server.clone();
                                let command_tx = event_tx.clone();
                                command_tasks.spawn(async move {
                                    command_server.handle_command_into(command, command_tx).await;
                                });
                            }
                            Err(err) => {
                                write_ack(
                                    &mut writer,
                                    ServerMessage::CommandResponse {
                                        command_id: "unknown".to_string(),
                                        result: Err(format!("invalid json command: {err}")),
                                    },
                                )
                                .await?;
                            }
                        }
                        line.clear();
                    }
                    Err(err) => {
                        write_ack(
                            &mut writer,
                            ServerMessage::CommandResponse {
                                command_id: "unknown".to_string(),
                                result: Err(format!("read command: {err}")),
                            },
                        )
                        .await?;
                        input_closed = true;
                    }
                }
            }
            _ = command_tasks.join_next(), if !command_tasks.is_empty() => {}
        }
    }
    Ok(())
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
