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
    CopyToClipboard { notification_id: u64, text: String },
    ReadClipboardImage,
}

impl Effect {
    pub fn send(command: Command) -> Self {
        Self::Send(command)
    }

    pub fn open_url(url: impl Into<String>) -> Self {
        Self::OpenUrl(url.into())
    }

    pub fn copy_to_clipboard(notification_id: u64, text: impl Into<String>) -> Self {
        Self::CopyToClipboard {
            notification_id,
            text: text.into(),
        }
    }

    pub fn read_clipboard_image() -> Self {
        Self::ReadClipboardImage
    }
}
