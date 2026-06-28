use std::path::PathBuf;

use tokio::sync::mpsc::UnboundedSender;

use crate::api::{
    ContentBlock, Event, Message, MessageContent, MessageEntry, ProtocolError, SessionTreeEntry,
};
use crate::prompts::{
    BuildSystemPromptOptions, build_system_prompt, expand_prompt_template, load_context_files,
    load_prompt_templates,
};
use crate::skills::load_skills;
use crate::state::HostState;
use crate::turn::runner::TurnRunInput;

use super::{HostServer, now_ms, send_event, storage_error};

impl HostServer {
    pub(super) async fn apply_turn_submit(
        &self,
        _command_id: String,
        session_id: String,
        text: String,
        tx: &UnboundedSender<Event>,
    ) -> Result<(), ProtocolError> {
        let cwd = {
            let state = self.state.lock().await;
            state.session_cwd(&session_id)?
        };
        let templates = load_prompt_templates(&cwd);
        let expanded_text = expand_prompt_template(&text, &templates);
        let context_files = load_context_files(&cwd);
        let skills = load_skills(&cwd).skills;
        let system_prompt = build_system_prompt(BuildSystemPromptOptions {
            cwd: PathBuf::from(&cwd),
            context_files,
            skills,
            prompt_templates: templates,
            ..BuildSystemPromptOptions::default()
        });

        let (turn_id, start_events, user_parent_id) = {
            let mut state = self.state.lock().await;
            let (turn_id, start_events) = state.start_turn(&session_id)?;
            let parent_id = state.session(&session_id)?.current_leaf_id.clone();
            (turn_id, start_events, parent_id)
        };
        for event in start_events {
            send_event(tx, event);
        }

        let user_message = Message::User {
            content: MessageContent::String(expanded_text.clone()),
            timestamp: Some(now_ms()),
        };
        let user_entry = if let Some(storage) = &self.storage {
            let path = {
                let paths = self.session_paths.lock().await;
                paths.get(&session_id).cloned()
            };
            if let Some(path) = path {
                storage
                    .append_message(&path, user_parent_id.as_deref(), &user_message)
                    .map_err(storage_error)?
            } else {
                SessionTreeEntry::Message(MessageEntry {
                    id: format!("msg_{}", uuid::Uuid::new_v4()),
                    parent_id: user_parent_id.clone(),
                    timestamp: now_ms().to_string(),
                    message: user_message,
                })
            }
        } else {
            SessionTreeEntry::Message(MessageEntry {
                id: format!("msg_{}", uuid::Uuid::new_v4()),
                parent_id: user_parent_id.clone(),
                timestamp: now_ms().to_string(),
                message: user_message,
            })
        };
        let user_message_id = user_entry.id().to_string();
        let config_parent_id = {
            let mut state = self.state.lock().await;
            state.append_entry(&session_id, user_entry)?;
            state.session(&session_id)?.current_leaf_id.clone()
        };

        if let Some(storage) = &self.storage {
            let path = {
                let paths = self.session_paths.lock().await;
                paths.get(&session_id).cloned()
            };
            if let Some(path) = path {
                let settings = self.settings.lock().await;
                if let Ok(entries) = storage.append_config_metadata(
                    &path,
                    config_parent_id.as_deref(),
                    settings.default_model.as_deref(),
                    settings.default_provider.as_deref(),
                    settings.default_thinking_level.as_ref().map(|l| l.as_str()),
                ) {
                    let mut state = self.state.lock().await;
                    for entry in entries {
                        let _ = state.append_entry(&session_id, entry);
                    }
                }
            }
        }

        send_event(
            tx,
            Event::UserMessageSubmitted {
                session_id: session_id.clone(),
                message_id: user_message_id,
                task_id: turn_id.clone(),
                text: expanded_text.clone(),
                timestamp: now_ms(),
            },
        );

        let active_tool_names = self.settings.lock().await.active_tool_names.clone();
        let runner = self.turn_supervisor.runner().await;
        let run_result = runner
            .run_turn(
                TurnRunInput {
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                    prompt: expanded_text,
                    system_prompt,
                    active_tool_names,
                },
                Some(tx.clone()),
            )
            .await;
        let session_path = if self.storage.is_some() {
            let paths = self.session_paths.lock().await;
            paths.get(&session_id).cloned()
        } else {
            None
        };

        match run_result {
            Ok(output) => {
                for event in output.events {
                    let mut state = self.state.lock().await;
                    if let Event::AssistantMessageCompleted { usage, .. } = &event
                        && let Some(usage) = usage
                    {
                        state
                            .session_mut(&session_id)
                            .ok()
                            .map(|s| s.accumulate_usage(usage));
                    }
                    persist_completed_message_event(
                        &self.storage,
                        session_path.as_ref(),
                        &mut state,
                        &session_id,
                        &event,
                    )?;
                    drop(state);
                    send_event(tx, event);
                }

                let complete_event = {
                    let mut state = self.state.lock().await;
                    state.clear_active_turn(&session_id, &turn_id)?;
                    Event::TurnCompleted {
                        session_id: session_id.clone(),
                        turn_id: turn_id.clone(),
                        total_tasks: output.total_tasks.max(1),
                        timestamp: now_ms(),
                    }
                };
                send_event(tx, complete_event);
            }
            Err(error) => {
                let fail_event = {
                    let mut state = self.state.lock().await;
                    state.fail_turn(&session_id, &turn_id, error.to_string())?
                };
                send_event(tx, fail_event);
                return Ok(());
            }
        }

        let context_window = {
            let settings = self.settings.lock().await;
            settings
                .compaction
                .as_ref()
                .and_then(|c| c.reserve_tokens)
                .unwrap_or(16384)
                + settings
                    .compaction
                    .as_ref()
                    .and_then(|c| c.keep_recent_tokens)
                    .unwrap_or(20000)
        };
        self.compact_session_if_needed(&session_id, context_window, tx)
            .await;

        let mut queued: Vec<String> = Vec::new();
        let mut state = self.state.lock().await;
        while let Some(next_text) = drain_one_queued(&mut state, &session_id) {
            queued.push(next_text);
        }
        drop(state);

        for next_text in queued {
            {
                let state = self.state.lock().await;
                let queue_event: Event = state.build_queue_update(&session_id).into();
                drop(state);
                send_event(tx, queue_event);
            }
            Box::pin(self.apply_turn_submit(
                format!("auto-{}", uuid::Uuid::new_v4()),
                session_id.clone(),
                next_text,
                tx,
            ))
            .await?;
        }
        Ok(())
    }
}

fn persist_completed_message_event(
    storage: &Option<crate::session::JsonlSessionRepository>,
    session_path: Option<&PathBuf>,
    state: &mut HostState,
    session_id: &str,
    event: &Event,
) -> Result<(), ProtocolError> {
    let Some(entry) = completed_message_event_to_entry(state, session_id, event)? else {
        return Ok(());
    };

    if let (Some(storage), Some(path)) = (storage, session_path) {
        storage.append_entry(path, &entry).map_err(storage_error)?;
    }
    state.append_entry(session_id, entry)
}

fn completed_message_event_to_entry(
    state: &HostState,
    session_id: &str,
    event: &Event,
) -> Result<Option<SessionTreeEntry>, ProtocolError> {
    let parent_id = state.session(session_id)?.current_leaf_id.clone();
    let entry = match event {
        Event::AssistantMessageCompleted {
            message_id,
            text,
            tool_calls,
            model,
            provider,
            usage,
            timestamp,
            ..
        } => {
            let mut content = Vec::new();
            if !text.is_empty() {
                content.push(ContentBlock::Text { text: text.clone() });
            }
            content.extend(tool_calls.iter().map(|tool_call| ContentBlock::ToolCall {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                arguments: tool_call.args.clone(),
                partial_json: None,
            }));
            SessionTreeEntry::Message(MessageEntry {
                id: message_id.clone(),
                parent_id,
                timestamp: timestamp.to_string(),
                message: Message::Assistant {
                    content,
                    api: "hostd".to_string(),
                    provider: provider.clone(),
                    model: model.clone(),
                    usage: usage.clone(),
                    stop_reason: None,
                    error_message: None,
                    timestamp: Some(*timestamp),
                },
            })
        }
        Event::ToolResultCommitted {
            message_id,
            tool_call_id,
            tool_name,
            content,
            is_error,
            timestamp,
            ..
        } => SessionTreeEntry::Message(MessageEntry {
            id: message_id.clone(),
            parent_id,
            timestamp: timestamp.to_string(),
            message: Message::ToolResult {
                tool_call_id: tool_call_id.clone(),
                tool_name: Some(tool_name.clone()),
                content: vec![ContentBlock::Text {
                    text: serde_json::to_string_pretty(content)
                        .unwrap_or_else(|_| content.to_string()),
                }],
                details: Some(content.clone()),
                is_error: Some(*is_error),
                timestamp: Some(*timestamp),
            },
        }),
        _ => return Ok(None),
    };
    Ok(Some(entry))
}

fn drain_one_queued(state: &mut HostState, session_id: &str) -> Option<String> {
    state
        .drain_next_follow_up(session_id)
        .or_else(|| state.drain_next_next_turn(session_id))
}
