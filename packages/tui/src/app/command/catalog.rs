//! Adapter from the frontend-neutral host catalog to TUI slash commands.

use piko_protocol::{HostCommandDescriptor, HostCommandInvoke};

use super::{Action, AppAction, ModelAction, SessionAction, SlashAction, SurfaceAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalCommandId {
    Settings,
    Todos,
    Tree,
    Usage,
    Notifications,
    Sessions,
    Models,
    Agents,
    Diff,
    History,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandTarget {
    Local(LocalCommandId),
    Host(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiCommandEntry {
    pub slash: String,
    pub title: String,
    pub detail: String,
    pub invoke: HostCommandInvoke,
    pub target: CommandTarget,
}

const LOCAL: &[(&str, LocalCommandId, &str, &str)] = &[
    (
        "/resume",
        LocalCommandId::Sessions,
        "Sessions",
        "List and open hostd sessions",
    ),
    (
        "/tree",
        LocalCommandId::Tree,
        "Session tree",
        "Inspect and navigate the current session branch tree",
    ),
    (
        "/model",
        LocalCommandId::Models,
        "Model",
        "Select a model and compatible thinking level",
    ),
    (
        "/settings",
        LocalCommandId::Settings,
        "Settings",
        "Open hostd-backed runtime settings",
    ),
    (
        "/todo",
        LocalCommandId::Todos,
        "Todos",
        "Show the viewed agent's current todo list",
    ),
    (
        "/usage",
        LocalCommandId::Usage,
        "Usage",
        "Show per-agent time, token, and cost totals",
    ),
    (
        "/noti",
        LocalCommandId::Notifications,
        "Notifications",
        "Show the in-memory notification queue",
    ),
    (
        "/agents",
        LocalCommandId::Agents,
        "Agents",
        "List agents in the current session and switch the viewed agent",
    ),
    (
        "/diff",
        LocalCommandId::Diff,
        "Work diff",
        "Show workspace diff for the last or active input",
    ),
    (
        "/history",
        LocalCommandId::History,
        "Session history",
        "Inspect journal-derived session history without opening it",
    ),
    ("/quit", LocalCommandId::Quit, "Quit", "Exit the TUI"),
];

const HOST: &[(&str, &str)] = &[
    ("/new", "session.new"),
    ("/fork", "session.fork"),
    ("/rename", "session.rename"),
    ("/import", "session.import"),
    ("/delete", "session.delete"),
    ("/login", "auth.login"),
    ("/logout", "auth.logout"),
    ("/compact", "session.compact"),
    ("/top", "process.list"),
    ("/mcp", "mcp.status"),
];

pub fn merge_command_catalog(host: &[HostCommandDescriptor]) -> Vec<TuiCommandEntry> {
    let mut entries = LOCAL
        .iter()
        .map(|(slash, id, title, detail)| TuiCommandEntry {
            slash: (*slash).into(),
            title: (*title).into(),
            detail: (*detail).into(),
            invoke: HostCommandInvoke::Immediate,
            target: CommandTarget::Local(*id),
        })
        .collect::<Vec<_>>();
    for (slash, id) in HOST {
        if let Some(descriptor) = host.iter().find(|descriptor| descriptor.id == *id) {
            entries.push(TuiCommandEntry {
                slash: (*slash).into(),
                title: descriptor.title.clone(),
                detail: descriptor.detail.clone(),
                invoke: descriptor.invoke.clone(),
                target: CommandTarget::Host(descriptor.id.clone()),
            });
        }
    }
    entries
}

#[derive(Default)]
pub struct HostCommandArgs {
    pub fork_entry_id: Option<String>,
    pub provider: Option<String>,
}

pub fn action_for_local_command(id: LocalCommandId) -> Action {
    match id {
        LocalCommandId::Sessions => SessionAction::RequestList.into(),
        LocalCommandId::Models => ModelAction::RequestList.into(),
        LocalCommandId::Agents => SurfaceAction::OpenAgents.into(),
        LocalCommandId::Tree => SurfaceAction::OpenTree.into(),
        LocalCommandId::Settings => SurfaceAction::OpenSettings.into(),
        LocalCommandId::Todos => SurfaceAction::OpenTodos.into(),
        LocalCommandId::Usage => SurfaceAction::OpenUsage.into(),
        LocalCommandId::Notifications => SurfaceAction::OpenNotifications.into(),
        LocalCommandId::Diff => SlashAction::RequestDiff.into(),
        LocalCommandId::History => SurfaceAction::OpenHistory(None).into(),
        LocalCommandId::Quit => AppAction::Quit.into(),
    }
}

pub fn action_for_host_command(id: &str, args: HostCommandArgs) -> Option<Action> {
    Some(match id {
        "session.new" => SlashAction::New.into(),
        "session.fork" => SlashAction::Fork(args.fork_entry_id).into(),
        "process.list" => SlashAction::ListProcesses.into(),
        "mcp.status" => SlashAction::ListMcpStatus.into(),
        "auth.login" => SlashAction::Login(args.provider).into(),
        "auth.logout" => SlashAction::Logout(args.provider).into(),
        "session.compact" => SlashAction::Compact.into(),
        _ => return None,
    })
}
