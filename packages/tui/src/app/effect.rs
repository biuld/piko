use piko_protocol::Command;

use crate::{app::command::Action, host::HostLine};

#[derive(Debug)]
pub enum Msg {
    Action(Action),
    HostLine(HostLine),
    Tick,
}

#[derive(Debug)]
pub enum Effect {
    Send(Command),
    OpenUrl(String),
}

impl Effect {
    pub fn send(command: Command) -> Self {
        Self::Send(command)
    }

    pub fn open_url(url: impl Into<String>) -> Self {
        Self::OpenUrl(url.into())
    }
}
