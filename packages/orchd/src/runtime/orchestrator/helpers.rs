use tokio::sync::mpsc;

use crate::domain::model::transcript::{ContentBlock, Message};
use crate::runtime::types::TaskControlMessage;

use super::RunContext;

pub(super) fn summarize(msg: &Message) -> String {
    let text: String = match msg {
        Message::Assistant { content, .. } => content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    };
    if text.len() > 200 {
        format!("{}...", &text[..200])
    } else {
        text
    }
}

pub(super) async fn wait_for_next_control_input(
    ctx: &RunContext,
    control_rx: &mut mpsc::UnboundedReceiver<TaskControlMessage>,
) -> Option<TaskControlMessage> {
    tokio::select! {
        _ = ctx.cancel.cancelled() => None,
        msg = control_rx.recv() => msg,
    }
}
