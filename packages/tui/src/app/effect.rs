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
}

impl Effect {
    pub fn send(command: Command) -> Self {
        Self::Send(command)
    }
}
