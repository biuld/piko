use std::sync::Arc;

use llmd::gateway::LlmGateway;

pub const SUMMARIZATION_PROMPT: &str = r#"The messages above are a conversation to summarize. Create a structured context checkpoint summary that another LLM will use to continue the work.

Use this EXACT format:

## Goal
[What is the user trying to accomplish? Can be multiple items if the session covers different tasks.]

## Constraints & Preferences
- [Any constraints, preferences, or requirements mentioned by user]
- [Or "(none)" if none were mentioned]

## Progress
### Done
- [x] [Completed tasks/changes]

### In Progress
- [ ] [Current work]

### Blocked
- [Issues preventing progress, if any]

## Key Decisions
- **[Decision]**: [Brief rationale]

## Next Steps
1. [Ordered list of what should happen next]

## Critical Context
- [Any data, examples, or references needed to continue]
- [Or "(none)" if not applicable]

Keep each section concise. Preserve exact file paths, function names, and error messages."#;

pub const UPDATE_SUMMARIZATION_PROMPT: &str = r#"The messages above are NEW conversation messages to incorporate into the existing summary provided in <previous-summary> tags.

Update the existing structured summary with new information. RULES:
- PRESERVE all existing information from the previous summary
- ADD new progress, decisions, and context from the new messages
- UPDATE the Progress section: move items from "In Progress" to "Done" when completed
- UPDATE "Next Steps" based on what was accomplished
- PRESERVE exact file paths, function names, and error messages
- If something is no longer relevant, you may remove it

Use this EXACT format:

## Goal
[Preserve existing goals, add new ones if the task expanded]

## Constraints & Preferences
- [Preserve existing, add new ones discovered]

## Progress
### Done
- [x] [Include previously done items AND newly completed items]

### In Progress
- [ ] [Current work - update based on progress]

### Blocked
- [Current blockers - remove if resolved]

## Key Decisions
- **[Decision]**: [Brief rationale] (preserve all previous, add new)

## Next Steps
1. [Update based on current state]

## Critical Context
- [Any data, examples, or references needed to continue]
- [Or "(none)" if not applicable]

Keep each section concise. Preserve exact file paths, function names, and error messages."#;

pub async fn summarize_history(
    model_executor: Arc<dyn LlmGateway>,
    model: piko_protocol::messages::Model,
    entries_to_summarize: &[crate::api::SessionTreeEntry],
    previous_summary: Option<&str>,
    file_ops_str: &str,
) -> Result<String, String> {
    let mut history = String::new();
    for entry in entries_to_summarize {
        let role = super::entry_role(entry).unwrap_or("metadata");
        let text = super::entry_text(entry);
        if !text.is_empty() {
            history.push_str(&format!("{}:\n{}\n\n", role, text));
        }
    }

    let mut system_prompt = String::new();
    if let Some(prev) = previous_summary {
        system_prompt.push_str(&format!(
            "<previous-summary>\n{}\n</previous-summary>\n\n",
            prev
        ));
        system_prompt.push_str(UPDATE_SUMMARIZATION_PROMPT);
    } else {
        system_prompt.push_str(SUMMARIZATION_PROMPT);
    }
    system_prompt.push_str("\n\nDo NOT continue the conversation. Do NOT respond to any questions in the conversation. ONLY output the structured summary.");
    system_prompt.push_str(file_ops_str);

    let messages = vec![piko_protocol::messages::Message::User {
        content: piko_protocol::messages::MessageContent::String(history),
        timestamp: None,
    }];

    model_executor
        .llm_call(
            model,
            Some(system_prompt),
            messages,
            piko_protocol::model::ModelRunSettings::default(),
        )
        .await
}
