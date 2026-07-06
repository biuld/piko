use serde::{Deserialize, Serialize};

use crate::messages::{ContentBlock, Message, MessageContent};
use crate::session::{CustomMessageContent, SessionTreeEntry};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum AgentMessage {
    Standard(Message),
    Custom(CustomAgentMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "role", rename_all = "camelCase")]
pub enum CustomAgentMessage {
    #[serde(rename = "bashExecution")]
    BashExecution {
        command: String,
        output: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        cancelled: bool,
        truncated: bool,
        exclude_from_context: bool,
        timestamp: i64,
    },
    #[serde(rename = "custom")]
    Custom {
        custom_type: String,
        content: CustomMessageContent,
        display: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
        timestamp: i64,
    },
    #[serde(rename = "branchSummary")]
    BranchSummary {
        summary: String,
        from_id: String,
        timestamp: i64,
    },
    #[serde(rename = "compactionSummary")]
    CompactionSummary {
        summary: String,
        tokens_before: u64,
        timestamp: i64,
    },
}

pub fn convert_agent_messages_to_llm(messages: &[AgentMessage]) -> Vec<Message> {
    messages
        .iter()
        .filter_map(|message| match message {
            AgentMessage::Standard(message) => Some(message.clone()),
            AgentMessage::Custom(custom) => custom_agent_message_to_llm(custom),
        })
        .collect()
}

fn custom_agent_message_to_llm(message: &CustomAgentMessage) -> Option<Message> {
    match message {
        CustomAgentMessage::BashExecution {
            command,
            output,
            exclude_from_context,
            timestamp,
            ..
        } => {
            if *exclude_from_context {
                return None;
            }
            Some(user_message(
                format!("Command executed: {command}\nOutput:\n{output}"),
                *timestamp,
            ))
        }
        CustomAgentMessage::Custom {
            content,
            display,
            timestamp,
            ..
        } => {
            if !display {
                return None;
            }
            Some(user_message(
                custom_message_content_text(content),
                *timestamp,
            ))
        }
        CustomAgentMessage::BranchSummary {
            summary, timestamp, ..
        } => Some(user_message(
            format!("[Previous conversation summary]:\n{summary}"),
            *timestamp,
        )),
        CustomAgentMessage::CompactionSummary {
            summary,
            tokens_before,
            timestamp,
        } => Some(user_message(
            format!("[Context compaction (~{tokens_before} tokens)]:\n{summary}"),
            *timestamp,
        )),
    }
}

fn user_message(content: String, timestamp: i64) -> Message {
    Message::User {
        content: MessageContent::String(content),
        timestamp: Some(timestamp),
    }
}

fn custom_message_content_text(content: &CustomMessageContent) -> String {
    match content {
        CustomMessageContent::String(text) => text.clone(),
        CustomMessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(content_block_text)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn content_block_text(block: &ContentBlock) -> Option<&str> {
    match block {
        ContentBlock::Text { text } => Some(text),
        _ => None,
    }
}

/// Convert session entries to AgentMessage for resume/transcript loading.
/// Compaction/BranchSummary/CustomMessage entries become AgentMessage::Custom variants.
pub fn entries_to_agent_messages(entries: &[SessionTreeEntry]) -> Vec<AgentMessage> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            SessionTreeEntry::Message(msg_entry) => {
                Some(AgentMessage::Standard(msg_entry.message.clone()))
            }
            SessionTreeEntry::Compaction(compaction) => Some(AgentMessage::Custom(
                CustomAgentMessage::CompactionSummary {
                    summary: compaction.summary.clone(),
                    tokens_before: compaction.tokens_before,
                    timestamp: compaction.timestamp.parse().unwrap_or(0),
                },
            )),
            SessionTreeEntry::BranchSummary(summary) => {
                Some(AgentMessage::Custom(CustomAgentMessage::BranchSummary {
                    summary: summary.summary.clone(),
                    from_id: summary.from_id.clone(),
                    timestamp: summary.timestamp.parse().unwrap_or(0),
                }))
            }
            SessionTreeEntry::CustomMessage(custom) => {
                Some(AgentMessage::Custom(CustomAgentMessage::Custom {
                    custom_type: custom.custom_type.clone(),
                    content: custom.content.clone(),
                    display: custom.display,
                    details: custom.details.clone(),
                    timestamp: custom.timestamp.parse().unwrap_or(0),
                }))
            }
            // Skip metadata-only entries
            SessionTreeEntry::ToolCall(_)
            | SessionTreeEntry::ThinkingLevelChange(_)
            | SessionTreeEntry::ModelChange(_)
            | SessionTreeEntry::ActiveToolsChange(_)
            | SessionTreeEntry::Custom(_)
            | SessionTreeEntry::Label(_)
            | SessionTreeEntry::SessionInfo(_)
            | SessionTreeEntry::Leaf(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_agent_messages_keeps_standard_messages_and_filters_hidden_custom() {
        let messages = vec![
            AgentMessage::Standard(Message::User {
                content: MessageContent::String("hello".into()),
                timestamp: Some(1),
            }),
            AgentMessage::Custom(CustomAgentMessage::BashExecution {
                command: "cat Cargo.toml".into(),
                output: "workspace".into(),
                exit_code: Some(0),
                cancelled: false,
                truncated: false,
                exclude_from_context: false,
                timestamp: 2,
            }),
            AgentMessage::Custom(CustomAgentMessage::Custom {
                custom_type: "hidden".into(),
                content: CustomMessageContent::String("secret".into()),
                display: false,
                details: None,
                timestamp: 3,
            }),
        ];

        let llm_messages = convert_agent_messages_to_llm(&messages);

        assert_eq!(llm_messages.len(), 2);
        assert!(matches!(
            &llm_messages[1],
            Message::User { content: MessageContent::String(text), timestamp: Some(2) }
                if text.contains("Command executed: cat Cargo.toml")
        ));
    }

    #[test]
    fn convert_agent_messages_formats_summaries_as_user_context() {
        let messages = vec![AgentMessage::Custom(
            CustomAgentMessage::CompactionSummary {
                summary: "short version".into(),
                tokens_before: 42,
                timestamp: 7,
            },
        )];

        let llm_messages = convert_agent_messages_to_llm(&messages);

        assert!(matches!(
            &llm_messages[0],
            Message::User { content: MessageContent::String(text), timestamp: Some(7) }
                if text.contains("~42 tokens") && text.contains("short version")
        ));
    }

    #[test]
    fn entries_to_agent_messages_converts_compaction_and_branch_summary() {
        use crate::session::{BranchSummaryEntry, CompactionEntry, MessageEntry, SessionTreeEntry};
        let entries = vec![
            SessionTreeEntry::Message(MessageEntry {
                id: "msg_1".into(),
                parent_id: None,
                timestamp: "1".into(),
                agent_id: None,
                message: Message::User {
                    content: MessageContent::String("hello".into()),
                    timestamp: Some(1),
                },
            }),
            SessionTreeEntry::Compaction(CompactionEntry {
                id: "comp_1".into(),
                parent_id: Some("msg_1".into()),
                timestamp: "2".into(),
                summary: "summary text".into(),
                first_kept_entry_id: "msg_3".into(),
                tokens_before: 100,
                details: None,
                from_hook: None,
            }),
            SessionTreeEntry::BranchSummary(BranchSummaryEntry {
                id: "branch_1".into(),
                parent_id: Some("comp_1".into()),
                timestamp: "3".into(),
                from_id: "msg_2".into(),
                summary: "branch summary".into(),
                details: None,
                from_hook: None,
            }),
        ];

        let agent_messages = entries_to_agent_messages(&entries);
        assert_eq!(agent_messages.len(), 3);
        assert!(matches!(
            &agent_messages[0],
            AgentMessage::Standard(Message::User { .. })
        ));
        assert!(matches!(
            &agent_messages[1],
            AgentMessage::Custom(CustomAgentMessage::CompactionSummary { summary, tokens_before, .. })
                if summary == "summary text" && *tokens_before == 100
        ));
        assert!(matches!(
            &agent_messages[2],
            AgentMessage::Custom(CustomAgentMessage::BranchSummary { summary, .. })
                if summary == "branch summary"
        ));

        // Round-trip: AgentMessage → LLM Message
        let llm_messages = convert_agent_messages_to_llm(&agent_messages);
        assert_eq!(llm_messages.len(), 3);
        assert!(matches!(&llm_messages[0], Message::User { .. }));
        assert!(
            matches!(&llm_messages[1], Message::User { content: MessageContent::String(text), .. }
            if text.contains("~100 tokens") && text.contains("summary text"))
        );
        assert!(
            matches!(&llm_messages[2], Message::User { content: MessageContent::String(text), .. }
            if text.contains("Previous conversation summary") && text.contains("branch summary"))
        );
    }
}
