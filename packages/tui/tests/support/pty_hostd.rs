use std::io::{self, BufRead, Write};

use piko_protocol::{
    AgentActivity, AgentInfo, AgentInstanceLifecycle, AgentStatus, Command, CommandResult,
    ReconcileReason, ServerMessage, SessionReconciledEvent, SessionSnapshot,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log_path = std::env::var_os("PIKO_TUI_PTY_LOG");
    let mut log = log_path
        .map(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
        })
        .transpose()?;
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout());

    for line in stdin.lock().lines() {
        let line = line?;
        let command: Command = serde_json::from_str(&line)?;
        if let Some(log) = log.as_mut() {
            writeln!(log, "{}", serde_json::to_string(&command)?)?;
            log.flush()?;
        }

        let command_id = command.command_id().to_string();
        if let Command::SessionCreate { cwd, .. } = &command {
            write_message(
                &mut stdout,
                &ServerMessage::CommandResponse {
                    command_id: command_id.clone(),
                    result: Ok(CommandResult::SessionCreated {
                        session_id: "pty-session".into(),
                        cwd: cwd.clone(),
                        timestamp: 0,
                    }),
                },
            )?;
            write_message(
                &mut stdout,
                &ServerMessage::SessionReconciled(SessionReconciledEvent {
                    session_id: "pty-session".into(),
                    reason: ReconcileReason::InitialHydration,
                    cursor: piko_protocol::agent_runtime::SessionCursor {
                        epoch: "pty".into(),
                        seq: 0,
                    },
                    snapshot: SessionSnapshot {
                        session_id: "pty-session".into(),
                        cwd: cwd.clone(),
                        seq: 0,
                        entries: Vec::new(),
                        model_steps: Vec::new(),
                        current_leaf_id: None,
                        selected_agent_instance_id: Some("pty-agent".into()),
                        active_turns: Vec::new(),
                        pending_approvals: Vec::new(),
                        pending_interactions: Vec::new(),
                        name: None,
                        cumulative_usage: None,
                        agent_usage: Vec::new(),
                        todo_lists: Vec::new(),
                    },
                    agents: vec![AgentInfo {
                        session_id: "pty-session".into(),
                        agent_instance_id: "pty-agent".into(),
                        agent_id: "main".into(),
                        parent_agent_instance_id: None,
                        lifecycle: AgentInstanceLifecycle::Open,
                        activity: AgentActivity::Idle,
                        unread_report_count: 0,
                        name: "Main".into(),
                        role: "main".into(),
                        status: AgentStatus::Idle,
                    }],
                }),
            )?;
        } else {
            write_message(
                &mut stdout,
                &ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(result_for(&command)),
                },
            )?;
        }
    }

    Ok(())
}

fn result_for(command: &Command) -> CommandResult {
    match command {
        Command::ConfigGet { namespace, .. } => CommandResult::ConfigEntry {
            namespace: namespace.clone(),
            value: serde_json::json!({}),
        },
        Command::CommandCatalogGet { .. } => CommandResult::CommandCatalogListed {
            commands: Vec::new(),
            timestamp: 0,
        },
        Command::ModelList { .. } => CommandResult::ModelListed {
            providers: Vec::new(),
            timestamp: 0,
        },
        _ => CommandResult::Empty,
    }
}

fn write_message(
    writer: &mut impl Write,
    message: &ServerMessage,
) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer(&mut *writer, message)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}
