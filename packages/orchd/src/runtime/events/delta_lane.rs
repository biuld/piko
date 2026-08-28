use async_trait::async_trait;
use piko_llmd::gateway::{FinishReason, InferenceEvent};
use std::time::Instant;

use crate::domain::model::step::ModelSpec;
use crate::domain::transcript::{ContentBlock, MessageUsage};
use crate::ports::clock::now_ms;
use piko_protocol::Message;
use piko_protocol::agent_runtime::RealtimeDelta;

use crate::domain::RealtimeFrame;
use crate::runtime::events::collector::SharedRealtimeCollector;
use crate::runtime::events::identity::{AgentDispatchContext, StepEventConsumer};

fn protocol_upstream_status(
    status: piko_llmd::tools::UpstreamActivityStatus,
) -> piko_protocol::messages::UpstreamActivityStatus {
    match status {
        piko_llmd::tools::UpstreamActivityStatus::Started => {
            piko_protocol::messages::UpstreamActivityStatus::Started
        }
        piko_llmd::tools::UpstreamActivityStatus::InProgress => {
            piko_protocol::messages::UpstreamActivityStatus::InProgress
        }
        piko_llmd::tools::UpstreamActivityStatus::Completed => {
            piko_protocol::messages::UpstreamActivityStatus::Completed
        }
        piko_llmd::tools::UpstreamActivityStatus::Failed => {
            piko_protocol::messages::UpstreamActivityStatus::Failed
        }
    }
}

fn append_text_block(order: &mut Vec<ContentBlock>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(ContentBlock::Text { text: current }) = order.last_mut() {
        current.push_str(text);
    } else {
        order.push(ContentBlock::Text {
            text: text.to_string(),
        });
    }
}

fn append_thinking_block(order: &mut Vec<ContentBlock>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(ContentBlock::Thinking { thinking, .. }) = order.last_mut() {
        thinking.push_str(text);
    } else {
        order.push(ContentBlock::Thinking {
            thinking: text.to_string(),
            thinking_signature: None,
            duration_ms: None,
        });
    }
}

fn push_semantic(
    semantic: &mut Vec<ContentBlock>,
    order: &mut Vec<ContentBlock>,
    block: ContentBlock,
) {
    if let Some(id) = upstream_block_id(&block).map(str::to_string) {
        upsert_upstream_block(semantic, &id, block.clone());
        upsert_upstream_block(order, &id, block);
    } else {
        semantic.push(block.clone());
        order.push(block);
    }
}

fn upstream_block_id(block: &ContentBlock) -> Option<&str> {
    match block {
        ContentBlock::UpstreamToolActivity { activity_id, .. } => Some(activity_id),
        ContentBlock::UpstreamToolApproval { approval_id, .. } => Some(approval_id),
        _ => None,
    }
}

fn upsert_upstream_block(blocks: &mut Vec<ContentBlock>, id: &str, block: ContentBlock) {
    if let Some(existing) = blocks
        .iter_mut()
        .find(|existing| upstream_block_id(existing) == Some(id))
    {
        // Keep previously captured arguments when the later lifecycle block
        // omits them (e.g. `output_item.done` may not repeat `action`).
        let keep_args = match (&*existing, &block) {
            (
                ContentBlock::UpstreamToolActivity {
                    arguments: Some(old),
                    ..
                },
                ContentBlock::UpstreamToolActivity {
                    arguments: None, ..
                },
            ) => Some(old.clone()),
            _ => None,
        };
        let keep_action = match (&*existing, &block) {
            (
                ContentBlock::UpstreamToolActivity {
                    action: Some(old), ..
                },
                ContentBlock::UpstreamToolActivity { action: None, .. },
            ) => Some(old.clone()),
            _ => None,
        };
        // Replace in place: the card keeps its tool-start position and only the
        // latest lifecycle status/args are reflected.
        *existing = block;
        if let Some(args) = keep_args
            && let ContentBlock::UpstreamToolActivity { arguments, .. } = existing
        {
            *arguments = Some(args);
        }
        if let Some(action) = keep_action
            && let ContentBlock::UpstreamToolActivity {
                action: action_slot,
                ..
            } = existing
        {
            *action_slot = Some(action);
        }
    } else {
        blocks.push(block);
    }
}

#[derive(Clone)]
pub(crate) struct AssistantMessageState {
    pub(crate) text: String,
    pub(crate) reasoning: String,
    pub(crate) usage: Option<MessageUsage>,
    pub(crate) stop_reason: String,
    pub(crate) error_message: Option<String>,
    pub(crate) checkpoint: Option<piko_protocol::OpaqueModelCheckpoint>,
    pub(crate) semantic_blocks: Vec<ContentBlock>,
    /// Content blocks in arrival order (thinking/text interleaved with
    /// semantic blocks), so `build_message` preserves the true timeline order.
    order: Vec<ContentBlock>,
    /// Current ordered thinking segment. The index is emitted on realtime
    /// deltas; committed content derives the same ordinal from its blocks.
    thinking_index: Option<u32>,
    next_thinking_index: u32,
    thinking_started_at: Option<Instant>,
}

impl AssistantMessageState {
    pub(crate) fn new() -> Self {
        Self {
            text: String::new(),
            reasoning: String::new(),
            usage: None,
            stop_reason: "stop".into(),
            error_message: None,
            checkpoint: None,
            semantic_blocks: Vec::new(),
            order: Vec::new(),
            thinking_index: None,
            next_thinking_index: 0,
            thinking_started_at: None,
        }
    }

    pub(crate) fn apply_gateway_event(&mut self, event: &InferenceEvent) {
        self.apply_gateway_event_at(event, Instant::now());
    }

    /// Apply one gateway event with an explicit monotonic observation time.
    /// Production callers use [`Self::apply_gateway_event`]; the timestamped
    /// form keeps duration tests deterministic without using wall-clock time.
    pub(crate) fn apply_gateway_event_at(&mut self, event: &InferenceEvent, now: Instant) {
        match event {
            InferenceEvent::Cursor(_) => {}
            InferenceEvent::TextDelta { delta, .. }
            | InferenceEvent::RefusalDelta { delta, .. } => {
                self.close_thinking_run_at(now);
                self.text.push_str(delta);
                append_text_block(&mut self.order, delta);
            }
            InferenceEvent::ReasoningDelta { delta, .. } => {
                if !delta.is_empty() && self.thinking_index.is_none() {
                    self.thinking_index = Some(self.next_thinking_index);
                    self.next_thinking_index = self.next_thinking_index.saturating_add(1);
                    self.thinking_started_at = Some(now);
                }
                self.reasoning.push_str(delta);
                append_thinking_block(&mut self.order, delta);
            }
            InferenceEvent::Usage(usage) => self.usage = Some(usage.clone()),
            InferenceEvent::Completed(status) => {
                self.close_thinking_run_at(now);
                if !matches!(status, FinishReason::Completed { .. }) {
                    self.checkpoint = None;
                }
                self.stop_reason = match status {
                    FinishReason::Completed { reason } => reason.clone(),
                    FinishReason::Incomplete { reason } => {
                        reason.clone().unwrap_or_else(|| "incomplete".into())
                    }
                    FinishReason::Failed { message } => {
                        self.error_message = Some(message.clone());
                        "error".into()
                    }
                    FinishReason::Cancelled => "abort".into(),
                };
            }
            InferenceEvent::Error(error) => {
                self.close_thinking_run_at(now);
                tracing::error!("Stream error: {error}");
                self.checkpoint = None;
                self.stop_reason = "error".into();
                self.error_message = Some(error.to_string());
            }
            InferenceEvent::Checkpoint(checkpoint) => self.checkpoint = Some(checkpoint.clone()),
            InferenceEvent::ToolCallDelta { .. } => self.close_thinking_run_at(now),
            InferenceEvent::UpstreamActivity(activity) => {
                self.close_thinking_run_at(now);
                let block = ContentBlock::UpstreamToolActivity {
                    activity_id: activity.activity_id.clone(),
                    tool_name: activity.tool_name.clone(),
                    kind: activity.kind.as_str().to_owned(),
                    arguments: activity.arguments.clone(),
                    action: activity.action.clone(),
                    status: match activity.status {
                        piko_llmd::tools::UpstreamActivityStatus::Started => {
                            piko_protocol::messages::UpstreamActivityStatus::Started
                        }
                        piko_llmd::tools::UpstreamActivityStatus::InProgress => {
                            piko_protocol::messages::UpstreamActivityStatus::InProgress
                        }
                        piko_llmd::tools::UpstreamActivityStatus::Completed => {
                            piko_protocol::messages::UpstreamActivityStatus::Completed
                        }
                        piko_llmd::tools::UpstreamActivityStatus::Failed => {
                            piko_protocol::messages::UpstreamActivityStatus::Failed
                        }
                    },
                };
                push_semantic(&mut self.semantic_blocks, &mut self.order, block);
            }
            InferenceEvent::ApprovalRequired(approval) => {
                self.close_thinking_run_at(now);
                let block = ContentBlock::UpstreamToolApproval {
                    approval_id: approval.approval_id.clone(),
                    tool_name: approval.tool_name.clone(),
                    summary: approval.summary.clone(),
                };
                push_semantic(&mut self.semantic_blocks, &mut self.order, block);
            }
            InferenceEvent::Source(source) => {
                self.close_thinking_run_at(now);
                let block = ContentBlock::Source {
                    source_id: source.source_id.clone(),
                    title: source.title.clone(),
                    uri: source.uri.clone(),
                };
                push_semantic(&mut self.semantic_blocks, &mut self.order, block);
            }
            InferenceEvent::Citation(citation) => {
                self.close_thinking_run_at(now);
                let block = ContentBlock::Citation {
                    source_id: citation.source_id.clone(),
                    output_item_id: citation.output_item_id.0.clone(),
                    start: citation.start,
                    end: citation.end,
                };
                push_semantic(&mut self.semantic_blocks, &mut self.order, block);
            }
            InferenceEvent::Artifact(artifact) => {
                self.close_thinking_run_at(now);
                let block = ContentBlock::Artifact {
                    artifact_id: artifact.artifact_id.clone(),
                    media_type: artifact.media_type.clone(),
                    namespace: artifact.resource.namespace.clone(),
                    resource: artifact.resource.resource.clone(),
                };
                push_semantic(&mut self.semantic_blocks, &mut self.order, block);
            }
        }
    }

    pub(crate) fn current_thinking_index(&self) -> Option<u32> {
        self.thinking_index
    }

    pub(crate) fn close_thinking_run(&mut self) {
        self.close_thinking_run_at(Instant::now());
    }

    fn close_thinking_run_at(&mut self, now: Instant) {
        let Some(started_at) = self.thinking_started_at.take() else {
            self.thinking_index = None;
            return;
        };
        let duration_ms = now
            .saturating_duration_since(started_at)
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        if let Some(ContentBlock::Thinking {
            duration_ms: stored,
            ..
        }) = self
            .order
            .iter_mut()
            .rev()
            .find(|block| matches!(block, ContentBlock::Thinking { .. }))
        {
            *stored = Some(duration_ms);
        }
        self.thinking_index = None;
    }

    pub(crate) fn build_message(&self, model: &ModelSpec) -> Message {
        // Preserve arrival order of thinking/text/semantic blocks so the
        // committed timeline interleaves an upstream tool card between the
        // text runs (text-before → card → text-after).
        let mut blocks = self.order.clone();
        if let Some(started_at) = self.thinking_started_at {
            let duration_ms = Instant::now()
                .saturating_duration_since(started_at)
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX);
            if let Some(ContentBlock::Thinking {
                duration_ms: stored,
                ..
            }) = blocks
                .iter_mut()
                .rev()
                .find(|block| matches!(block, ContentBlock::Thinking { .. }))
            {
                *stored = Some(duration_ms);
            }
        }
        if blocks.is_empty() {
            blocks = self.semantic_blocks.clone();
            if !self.reasoning.is_empty() {
                blocks.push(ContentBlock::Thinking {
                    thinking: self.reasoning.clone(),
                    thinking_signature: None,
                    duration_ms: None,
                });
            }
            if !self.text.is_empty() {
                blocks.push(ContentBlock::Text {
                    text: self.text.clone(),
                });
            }
        }
        if blocks.is_empty() {
            blocks.push(ContentBlock::Text {
                text: String::new(),
            });
        }
        Message::Assistant {
            content: blocks,
            checkpoint: self.checkpoint.clone().map(Box::new),
            provider: model.provider.clone(),
            model: model.id.clone(),
            usage: self.usage.clone(),
            stop_reason: Some(self.stop_reason.clone()),
            error_message: self.error_message.clone(),
            timestamp: Some(now_ms()),
        }
    }
}

pub(crate) struct RealtimeCollectingConsumer {
    collector: SharedRealtimeCollector,
    state: AssistantMessageState,
}

impl RealtimeCollectingConsumer {
    pub(crate) fn new(collector: SharedRealtimeCollector, state: AssistantMessageState) -> Self {
        Self { collector, state }
    }
}

#[async_trait]
impl StepEventConsumer for RealtimeCollectingConsumer {
    async fn on_step_started(&mut self, ctx: &AgentDispatchContext<'_>) {
        self.collector.push(RealtimeFrame::new(
            ctx.agent_instance_id.clone(),
            ctx.execution_id.clone(),
            ctx.agent_id.clone(),
            ctx.message_id.clone(),
            RealtimeDelta::MessageStarted {
                role: piko_protocol::MessageRole::Assistant,
            },
        ));
    }

    async fn on_gateway_event(&mut self, ctx: &AgentDispatchContext<'_>, event: &InferenceEvent) {
        self.state.apply_gateway_event(event);
        match event {
            InferenceEvent::TextDelta { delta, .. }
            | InferenceEvent::RefusalDelta { delta, .. } => {
                self.collector.push(RealtimeFrame::new(
                    ctx.agent_instance_id.clone(),
                    ctx.execution_id.clone(),
                    ctx.agent_id.clone(),
                    ctx.message_id.clone(),
                    RealtimeDelta::Text {
                        // Text chunks belong to one stable content segment;
                        // this is a segment id, not a byte offset.
                        content_index: 0,
                        delta: delta.clone(),
                    },
                ));
            }
            InferenceEvent::ReasoningDelta { delta, .. } => {
                self.collector.push(RealtimeFrame::new(
                    ctx.agent_instance_id.clone(),
                    ctx.execution_id.clone(),
                    ctx.agent_id.clone(),
                    ctx.message_id.clone(),
                    RealtimeDelta::Thinking {
                        // Thought and text are distinct stream item kinds, so
                        // each kind owns its own segment namespace.
                        content_index: self.state.current_thinking_index().unwrap_or(0),
                        delta: delta.clone(),
                    },
                ));
            }
            InferenceEvent::UpstreamActivity(activity) => {
                self.collector.push(RealtimeFrame::new(
                    ctx.agent_instance_id.clone(),
                    ctx.execution_id.clone(),
                    ctx.agent_id.clone(),
                    ctx.message_id.clone(),
                    piko_protocol::agent_runtime::RealtimeDelta::UpstreamActivity {
                        activity_id: activity.activity_id.clone(),
                        tool_name: activity.tool_name.clone(),
                        kind: activity.kind.as_str().to_owned(),
                        status: protocol_upstream_status(activity.status),
                        arguments: activity.arguments.clone(),
                        action: activity.action.clone(),
                    },
                ));
            }
            InferenceEvent::ApprovalRequired(approval) => {
                self.collector.push(RealtimeFrame::new(
                    ctx.agent_instance_id.clone(),
                    ctx.execution_id.clone(),
                    ctx.agent_id.clone(),
                    ctx.message_id.clone(),
                    piko_protocol::agent_runtime::RealtimeDelta::UpstreamApproval {
                        approval_id: approval.approval_id.clone(),
                        tool_name: approval.tool_name.clone(),
                        summary: approval.summary.clone(),
                    },
                ));
            }
            _ => {}
        }
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        self.state.close_thinking_run();
        let assistant_message = self
            .state
            .build_message(ctx.model.expect("step dispatch model missing"));
        self.collector.push(RealtimeFrame::new(
            ctx.agent_instance_id.clone(),
            ctx.execution_id.clone(),
            ctx.agent_id.clone(),
            ctx.message_id.clone(),
            RealtimeDelta::MessageEnded {
                stop_reason: match &assistant_message {
                    Message::Assistant { stop_reason, .. } => stop_reason.clone(),
                    _ => None,
                },
                error_message: match &assistant_message {
                    Message::Assistant { error_message, .. } => error_message.clone(),
                    _ => None,
                },
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use piko_llmd::gateway::{
        GeneratedArtifact, InferenceCitation, InferenceEvent, InferenceSource, OutputItemId,
        SemanticResourceRef, UpstreamApprovalRequest, UpstreamToolActivity,
    };
    use piko_llmd::tools::UpstreamActivityStatus;
    use piko_protocol::Message;

    use super::*;

    #[test]
    fn llmd_checkpoint_is_persisted_without_interpretation() {
        let mut state = AssistantMessageState::new();
        let checkpoint: piko_protocol::OpaqueModelCheckpoint =
            serde_json::from_value(serde_json::json!("opaque-token")).unwrap();
        state.apply_gateway_event(&InferenceEvent::Checkpoint(checkpoint.clone()));

        let message = state.build_message(&ModelSpec {
            id: "gpt-test".into(),
            name: "GPT Test".into(),
            provider: "openai".into(),
        });

        assert!(matches!(
            message,
            Message::Assistant {
                checkpoint: Some(persisted),
                ..
            } if persisted.as_ref() == &checkpoint
        ));
    }

    #[test]
    fn stateless_terminal_requires_no_checkpoint() {
        let mut state = AssistantMessageState::new();
        state.apply_gateway_event(&InferenceEvent::completed("stop"));

        let message = state.build_message(&ModelSpec {
            id: "gpt-test".into(),
            name: "GPT Test".into(),
            provider: "openai".into(),
        });
        assert!(matches!(
            message,
            Message::Assistant {
                checkpoint: None,
                stop_reason: Some(reason),
                error_message: None,
                ..
            } if reason == "stop"
        ));
    }

    #[test]
    fn incomplete_terminal_discards_a_pending_checkpoint() {
        let mut state = AssistantMessageState::new();
        let checkpoint = serde_json::from_value(serde_json::json!("opaque-token")).unwrap();
        state.apply_gateway_event(&InferenceEvent::Checkpoint(checkpoint));
        state.apply_gateway_event(&InferenceEvent::Completed(FinishReason::Cancelled));
        let message = state.build_message(&ModelSpec {
            id: "gpt-test".into(),
            name: "GPT Test".into(),
            provider: "openai".into(),
        });
        assert!(matches!(
            message,
            Message::Assistant {
                checkpoint: None,
                ..
            }
        ));
    }

    #[test]
    fn thinking_runs_record_monotonic_durations_and_ordered_indices() {
        let start = Instant::now();
        let mut state = AssistantMessageState::new();
        state.apply_gateway_event_at(
            &InferenceEvent::ReasoningDelta {
                item_id: OutputItemId("reasoning-1".into()),
                delta: "first".into(),
            },
            start,
        );
        state.apply_gateway_event_at(
            &InferenceEvent::ReasoningDelta {
                item_id: OutputItemId("reasoning-1".into()),
                delta: " thought".into(),
            },
            start + std::time::Duration::from_millis(100),
        );
        assert_eq!(state.current_thinking_index(), Some(0));
        state.apply_gateway_event_at(
            &InferenceEvent::TextDelta {
                item_id: OutputItemId("text-1".into()),
                delta: "answer".into(),
            },
            start + std::time::Duration::from_millis(250),
        );
        assert_eq!(state.current_thinking_index(), None);
        state.apply_gateway_event_at(
            &InferenceEvent::ReasoningDelta {
                item_id: OutputItemId("reasoning-2".into()),
                delta: "second".into(),
            },
            start + std::time::Duration::from_millis(500),
        );
        assert_eq!(state.current_thinking_index(), Some(1));
        state.apply_gateway_event_at(
            &InferenceEvent::ToolCallDelta {
                call_id: piko_llmd::gateway::ToolCallId("call-1".into()),
                name: "search".into(),
                arguments_delta: "{}".into(),
            },
            start + std::time::Duration::from_millis(900),
        );
        state.apply_gateway_event_at(
            &InferenceEvent::Completed(FinishReason::Completed {
                reason: "stop".into(),
            }),
            start + std::time::Duration::from_millis(1000),
        );

        let Message::Assistant { content, .. } = state.build_message(&ModelSpec {
            id: "gpt-test".into(),
            name: "GPT Test".into(),
            provider: "openai".into(),
        }) else {
            panic!("expected assistant message");
        };
        assert!(matches!(
            &content[0],
            ContentBlock::Thinking {
                thinking,
                duration_ms: Some(250),
                ..
            } if thinking == "first thought"
        ));
        assert!(matches!(&content[1], ContentBlock::Text { text } if text == "answer"));
        assert!(matches!(
            &content[2],
            ContentBlock::Thinking {
                thinking,
                duration_ms: Some(400),
                ..
            } if thinking == "second"
        ));
    }

    #[test]
    fn cancellation_finalizes_an_open_thinking_run() {
        let start = Instant::now();
        let mut state = AssistantMessageState::new();
        state.apply_gateway_event_at(
            &InferenceEvent::ReasoningDelta {
                item_id: OutputItemId("reasoning-1".into()),
                delta: "cancelled thought".into(),
            },
            start,
        );
        state.apply_gateway_event_at(
            &InferenceEvent::Completed(FinishReason::Cancelled),
            start + std::time::Duration::from_millis(123),
        );
        let Message::Assistant { content, .. } = state.build_message(&ModelSpec {
            id: "gpt-test".into(),
            name: "GPT Test".into(),
            provider: "openai".into(),
        }) else {
            panic!("expected assistant message");
        };
        assert!(matches!(
            &content[0],
            ContentBlock::Thinking {
                duration_ms: Some(123),
                ..
            }
        ));
    }

    #[test]
    fn upstream_observations_are_projected_without_orchd_policy_decisions() {
        let mut state = AssistantMessageState::new();
        for event in [
            InferenceEvent::UpstreamActivity(UpstreamToolActivity {
                activity_id: "activity-1".into(),
                tool_name: "search".into(),
                kind: piko_llmd::capabilities::UpstreamToolKind::new("search").unwrap(),
                status: UpstreamActivityStatus::InProgress,
                arguments: None,
                action: None,
            }),
            InferenceEvent::ApprovalRequired(UpstreamApprovalRequest {
                approval_id: "approval-1".into(),
                tool_name: "search".into(),
                summary: "search the web".into(),
            }),
            InferenceEvent::Source(InferenceSource {
                source_id: "source-1".into(),
                title: Some("Source".into()),
                uri: Some("https://example.test".into()),
            }),
            InferenceEvent::Citation(InferenceCitation {
                source_id: "source-1".into(),
                output_item_id: OutputItemId("out_semantic".into()),
                start: Some(0),
                end: Some(4),
            }),
            InferenceEvent::Artifact(GeneratedArtifact {
                artifact_id: "artifact-1".into(),
                media_type: "image/png".into(),
                resource: SemanticResourceRef {
                    namespace: "session".into(),
                    resource: "artifact-1".into(),
                },
            }),
        ] {
            state.apply_gateway_event(&event);
        }

        assert_eq!(state.semantic_blocks.len(), 5);
        assert!(state.error_message.is_none());
        assert!(matches!(
            &state.semantic_blocks[1],
            ContentBlock::UpstreamToolApproval { approval_id, .. } if approval_id == "approval-1"
        ));
    }

    #[test]
    fn upstream_lifecycle_collapses_to_single_block_by_activity_id() {
        let mut state = AssistantMessageState::new();
        let activity = |status: UpstreamActivityStatus| {
            InferenceEvent::UpstreamActivity(UpstreamToolActivity {
                activity_id: "ws_1".into(),
                tool_name: "web_search".into(),
                kind: piko_llmd::capabilities::UpstreamToolKind::new("search").unwrap(),
                status,
                arguments: if matches!(status, UpstreamActivityStatus::Completed) {
                    Some(serde_json::json!({ "query": "USD CNY" }))
                } else {
                    None
                },
                action: None,
            })
        };
        state.apply_gateway_event(&activity(UpstreamActivityStatus::InProgress));
        state.apply_gateway_event(&activity(UpstreamActivityStatus::Completed));

        // One block per activity_id (latest state wins), not one per event.
        let upstream_blocks = state
            .semantic_blocks
            .iter()
            .filter(|b| matches!(b, ContentBlock::UpstreamToolActivity { .. }))
            .count();
        assert_eq!(upstream_blocks, 1, "lifecycle events collapse to one block");

        let message = state.build_message(&ModelSpec {
            id: "gpt-test".into(),
            name: "GPT Test".into(),
            provider: "openai".into(),
        });
        let Message::Assistant { content, .. } = &message else {
            panic!("expected assistant message");
        };
        let upstream_blocks = content
            .iter()
            .filter(|b| matches!(b, ContentBlock::UpstreamToolActivity { .. }))
            .count();
        assert_eq!(upstream_blocks, 1, "committed content collapses upstream");
        assert!(content.iter().any(|b| matches!(
            b,
            ContentBlock::UpstreamToolActivity {
                status: piko_protocol::messages::UpstreamActivityStatus::Completed,
                arguments: Some(arguments),
                ..
            } if arguments.get("query").and_then(|q| q.as_str()) == Some("USD CNY")
        )));
    }
}
