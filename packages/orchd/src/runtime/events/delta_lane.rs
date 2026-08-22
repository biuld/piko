use async_trait::async_trait;
use piko_llmd::gateway::{FinishReason, InferenceEvent};

use crate::domain::model::step::ModelSpec;
use crate::domain::transcript::{ContentBlock, MessageUsage};
use crate::ports::clock::now_ms;
use piko_protocol::Message;
use piko_protocol::agent_runtime::RealtimeDelta;

use crate::domain::RealtimeFrame;
use crate::runtime::events::collector::SharedRealtimeCollector;
use crate::runtime::events::identity::{AgentDispatchContext, StepEventConsumer};

#[derive(Clone)]
pub(crate) struct AssistantMessageState {
    pub(crate) text: String,
    pub(crate) reasoning: String,
    pub(crate) usage: Option<MessageUsage>,
    pub(crate) stop_reason: String,
    pub(crate) error_message: Option<String>,
    pub(crate) checkpoint: Option<piko_protocol::OpaqueModelCheckpoint>,
    pub(crate) semantic_blocks: Vec<ContentBlock>,
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
        }
    }

    pub(crate) fn apply_gateway_event(&mut self, event: &InferenceEvent) {
        match event {
            InferenceEvent::Cursor(_) => {}
            InferenceEvent::TextDelta { delta, .. }
            | InferenceEvent::RefusalDelta { delta, .. } => {
                self.text.push_str(delta);
            }
            InferenceEvent::ReasoningDelta { delta, .. } => {
                self.reasoning.push_str(delta);
            }
            InferenceEvent::Usage(usage) => self.usage = Some(usage.clone()),
            InferenceEvent::Completed(status) => {
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
                tracing::error!("Stream error: {error}");
                self.checkpoint = None;
                self.stop_reason = "error".into();
                self.error_message = Some(error.to_string());
            }
            InferenceEvent::Checkpoint(checkpoint) => self.checkpoint = Some(checkpoint.clone()),
            InferenceEvent::ToolCallDelta { .. } => {}
            InferenceEvent::UpstreamActivity(activity) => {
                self.semantic_blocks
                    .push(ContentBlock::UpstreamToolActivity {
                        activity_id: activity.activity_id.clone(),
                        tool_name: activity.tool_name.clone(),
                        kind: activity.kind.as_str().to_owned(),
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
                    });
            }
            InferenceEvent::ApprovalRequired(approval) => {
                self.semantic_blocks
                    .push(ContentBlock::UpstreamToolApproval {
                        approval_id: approval.approval_id.clone(),
                        tool_name: approval.tool_name.clone(),
                        summary: approval.summary.clone(),
                    });
            }
            InferenceEvent::Source(source) => self.semantic_blocks.push(ContentBlock::Source {
                source_id: source.source_id.clone(),
                title: source.title.clone(),
                uri: source.uri.clone(),
            }),
            InferenceEvent::Citation(citation) => {
                self.semantic_blocks.push(ContentBlock::Citation {
                    source_id: citation.source_id.clone(),
                    output_item_id: citation.output_item_id.0.clone(),
                    start: citation.start,
                    end: citation.end,
                });
            }
            InferenceEvent::Artifact(artifact) => {
                self.semantic_blocks.push(ContentBlock::Artifact {
                    artifact_id: artifact.artifact_id.clone(),
                    media_type: artifact.media_type.clone(),
                    namespace: artifact.resource.namespace.clone(),
                    resource: artifact.resource.resource.clone(),
                });
            }
        }
    }

    pub(crate) fn build_message(&self, model: &ModelSpec) -> Message {
        let mut blocks = self.semantic_blocks.clone();
        if !self.reasoning.is_empty() {
            blocks.push(ContentBlock::Thinking {
                thinking: self.reasoning.clone(),
                thinking_signature: None,
            });
        }
        if !self.text.is_empty() {
            blocks.push(ContentBlock::Text {
                text: self.text.clone(),
            });
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
                        // each kind owns its own segment-zero namespace.
                        content_index: 0,
                        delta: delta.clone(),
                    },
                ));
            }
            _ => {}
        }
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
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
    fn upstream_observations_are_projected_without_orchd_policy_decisions() {
        let mut state = AssistantMessageState::new();
        for event in [
            InferenceEvent::UpstreamActivity(UpstreamToolActivity {
                activity_id: "activity-1".into(),
                tool_name: "search".into(),
                kind: piko_llmd::capabilities::UpstreamToolKind::new("search").unwrap(),
                status: UpstreamActivityStatus::InProgress,
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
}
