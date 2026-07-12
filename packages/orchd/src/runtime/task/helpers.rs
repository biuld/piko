use tokio::sync::mpsc;

use crate::domain::transcript::{ContentBlock, Message};
use crate::runtime::task::mailbox::TaskMailboxMessage;

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
    truncate_at_char_boundary(&text, 200)
}

/// Truncate to at most `max_bytes` without splitting a UTF-8 codepoint.
/// Byte-index slicing (`&s[..n]`) panics mid-character; CJK/emoji summaries hit this.
fn truncate_at_char_boundary(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

pub(super) async fn wait_for_next_mailbox_message(
    ctx: &RunContext,
    control_rx: &mut mpsc::UnboundedReceiver<TaskMailboxMessage>,
) -> Option<TaskMailboxMessage> {
    tokio::select! {
        _ = ctx.cancel.cancelled() => None,
        msg = control_rx.recv() => msg,
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_at_char_boundary;

    #[test]
    fn truncate_does_not_panic_on_multibyte_boundary() {
        // 534-byte Chinese reply from the hung session; byte 200 is mid-codepoint.
        let text = "我是 **piko** 项目内置的 AI 编码助手（Coding Agent），运行在 piko 的 Agent Harness 中。\n\n我的能力包括：\n- **阅读和编辑代码文件**\n- **执行 bash 命令**（编译、测试、搜索等）\n- **编写新文件**\n- **规划和跟踪多步骤任务**\n- **生成并处理子任务**（通过 agent 模板）\n\n我的职责是帮你完成与 piko 项目相关的编码任务——无论是修改代码、调试问题、添加功能，还是浏览和理解代码库。\n\n有什么我可以帮你的吗？😊";
        assert!(text.len() > 200);
        assert!(!text.is_char_boundary(200));
        let out = truncate_at_char_boundary(text, 200);
        assert!(out.ends_with("..."));
        assert!(out.len() <= 203); // truncated prefix + "..."
        assert!(out.is_char_boundary(out.len() - 3));
    }

    #[test]
    fn truncate_keeps_short_text() {
        assert_eq!(truncate_at_char_boundary("hi", 200), "hi");
    }
}
