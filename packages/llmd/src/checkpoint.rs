use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use piko_protocol::OpaqueModelCheckpoint;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::gateway::{Conversation, ErrorClass, InferenceError};
use crate::target::ModelTarget;

const FORMAT_VERSION: u8 = 1;
const MAX_TOKEN_BYTES: usize = 128 * 1024;
const MAX_DECODED_BYTES: usize = 96 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Envelope {
    version: u8,
    target: String,
    anchor: Option<String>,
    prefix_digest: String,
    carrier_digest: String,
    payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub(crate) struct AdapterCheckpoint {
    pub payload: serde_json::Value,
}

#[derive(Debug)]
pub(crate) enum ConversationPlan<'a> {
    FullReplay {
        items: &'a [crate::gateway::ConversationItem],
    },
    Resume {
        checkpoint: AdapterCheckpoint,
        suffix: &'a [crate::gateway::ConversationItem],
    },
    OpaqueReplay {
        checkpoint: AdapterCheckpoint,
        items: &'a [crate::gateway::ConversationItem],
    },
}

pub(crate) fn encode(
    target: &ModelTarget,
    conversation: &Conversation,
    carrier_digest: String,
    payload: serde_json::Value,
) -> Result<OpaqueModelCheckpoint, InferenceError> {
    let anchor = conversation.items.last().map(|item| item.id.0.clone());
    let envelope = Envelope {
        version: FORMAT_VERSION,
        target: target_fingerprint(target),
        anchor,
        prefix_digest: prefix_digest(conversation, conversation.items.len()),
        carrier_digest,
        payload,
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|_| rejected(target))?;
    if bytes.len() > MAX_DECODED_BYTES {
        return Err(rejected(target));
    }
    carrier_from_token(URL_SAFE_NO_PAD.encode(bytes)).map_err(|_| rejected(target))
}

pub(crate) fn plan<'a>(
    target: &ModelTarget,
    conversation: &'a Conversation,
) -> Result<ConversationPlan<'a>, InferenceError> {
    for (checkpoint_index, item) in conversation.items.iter().enumerate().rev() {
        let Some(checkpoint) = item.checkpoint.as_ref() else {
            continue;
        };
        let envelope = match decode(checkpoint, target) {
            Ok(envelope) => envelope,
            Err(_) => {
                tracing::debug!(
                    target_id = %target.id,
                    "checkpoint rejected; considering safe full replay"
                );
                continue;
            }
        };
        let anchor_len = match envelope.anchor.as_deref() {
            Some(anchor) => conversation
                .items
                .iter()
                .position(|item| item.id.0 == anchor)
                .map(|index| index + 1),
            None => Some(0),
        };
        let Some(anchor_len) = anchor_len else {
            continue;
        };
        if anchor_len != checkpoint_index
            || !matches!(
                &item.kind,
                crate::gateway::ConversationItemKind::Assistant { .. }
            )
            || prefix_digest(conversation, anchor_len) != envelope.prefix_digest
            || assistant_item_digest(&item.kind).as_deref() != Some(&envelope.carrier_digest)
        {
            tracing::debug!(
                target_id = %target.id,
                "checkpoint history boundary rejected; considering safe full replay"
            );
            continue;
        }
        let checkpoint = AdapterCheckpoint {
            payload: envelope.payload,
        };
        return Ok(match target.responses_continuation() {
            Some(crate::modeling::ResponsesContinuationPolicy::PreviousResponseId) => {
                ConversationPlan::Resume {
                    checkpoint,
                    suffix: &conversation.items[checkpoint_index + 1..],
                }
            }
            Some(crate::modeling::ResponsesContinuationPolicy::EncryptedReasoning) => {
                ConversationPlan::OpaqueReplay {
                    checkpoint,
                    items: &conversation.items,
                }
            }
            _ => ConversationPlan::FullReplay {
                items: &conversation.items,
            },
        });
    }
    if !target.capabilities.replay_safe {
        return Err(InferenceError::new(
            ErrorClass::ContinuationUnavailable,
            &target.id,
            "checkpoint",
            "required continuation state is unavailable",
        ));
    }
    Ok(ConversationPlan::FullReplay {
        items: &conversation.items,
    })
}

pub(crate) fn assistant_output_digest(reasoning: &str, text: &str) -> String {
    let mut digest = Sha256::new();
    digest.update((reasoning.len() as u64).to_be_bytes());
    digest.update(reasoning.as_bytes());
    digest.update((text.len() as u64).to_be_bytes());
    digest.update(text.as_bytes());
    format!("{:x}", digest.finalize())
}

fn assistant_item_digest(kind: &crate::gateway::ConversationItemKind) -> Option<String> {
    let crate::gateway::ConversationItemKind::Assistant { content } = kind else {
        return None;
    };
    let mut reasoning = String::new();
    let mut text = String::new();
    for block in content {
        match block {
            piko_protocol::ContentBlock::Thinking { thinking, .. } => reasoning.push_str(thinking),
            piko_protocol::ContentBlock::Text { text: value } => text.push_str(value),
            _ => {}
        }
    }
    Some(assistant_output_digest(&reasoning, &text))
}

fn decode(
    checkpoint: &OpaqueModelCheckpoint,
    target: &ModelTarget,
) -> Result<Envelope, InferenceError> {
    let token = token_from_carrier(checkpoint).ok_or_else(|| rejected(target))?;
    if token.len() > MAX_TOKEN_BYTES {
        return Err(rejected(target));
    }
    let estimated = token.len().saturating_mul(3) / 4 + 3;
    if estimated > MAX_DECODED_BYTES {
        return Err(rejected(target));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| rejected(target))?;
    if bytes.len() > MAX_DECODED_BYTES {
        return Err(rejected(target));
    }
    let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|_| rejected(target))?;
    if envelope.version != FORMAT_VERSION || envelope.target != target_fingerprint(target) {
        return Err(rejected(target));
    }
    Ok(envelope)
}

fn carrier_from_token(token: String) -> Result<OpaqueModelCheckpoint, serde_json::Error> {
    serde_json::from_value(serde_json::Value::String(token))
}

fn token_from_carrier(checkpoint: &OpaqueModelCheckpoint) -> Option<String> {
    serde_json::to_value(checkpoint)
        .ok()?
        .as_str()
        .map(str::to_owned)
}

fn target_fingerprint(target: &ModelTarget) -> String {
    let mut digest = Sha256::new();
    digest.update(target.id.as_bytes());
    digest.update([0]);
    digest.update(target.model.as_bytes());
    digest.update([0]);
    digest.update(target.api_surface.as_bytes());
    digest.update([0]);
    digest.update(serde_json::to_vec(&target.protocol).unwrap_or_default());
    format!("{:x}", digest.finalize())
}

fn prefix_digest(conversation: &Conversation, len: usize) -> String {
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(&conversation.instructions).unwrap_or_default());
    digest.update([0xfe]);
    for item in conversation.items.iter().take(len) {
        digest.update(serde_json::to_vec(&item.id).unwrap_or_default());
        digest.update([0]);
        digest.update(serde_json::to_vec(&item.kind).unwrap_or_default());
        digest.update([0xff]);
    }
    format!("{:x}", digest.finalize())
}

fn rejected(target: &ModelTarget) -> InferenceError {
    InferenceError::new(
        ErrorClass::CheckpointRejected,
        &target.id,
        "checkpoint",
        "checkpoint rejected",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modeling::{ProtocolProfile, ResponsesContinuationPolicy};
    use crate::target::ModelTargetConfig;

    fn target(policy: ResponsesContinuationPolicy) -> ModelTarget {
        let mut config = ModelTargetConfig::new(
            "fixture/gpt@responses",
            "responses",
            piko_protocol::model::ProviderAuthMethod::ApiKey,
            ProtocolProfile::Responses {
                continuation: policy,
            },
        );
        config.base_url = Some("https://example.test/v1".into());
        ModelTarget::resolve("fixture/gpt", "gpt", &config, None).unwrap()
    }

    fn conversation_with_checkpoint(target: &ModelTarget) -> Conversation {
        let mut conversation = crate::protocols::tests_support::semantic_request().conversation;
        let prefix = Conversation {
            instructions: conversation.instructions.clone(),
            items: conversation.items[..1].to_vec(),
        };
        conversation.items[1].checkpoint = Some(
            encode(
                target,
                &prefix,
                assistant_item_digest(&conversation.items[1].kind).unwrap(),
                serde_json::json!({"response_id":"resp_private"}),
            )
            .unwrap(),
        );
        conversation
    }

    #[test]
    fn arbitrary_tokens_do_not_leak_in_debug() {
        let checkpoint = carrier_from_token("secret-response-id".into()).unwrap();
        let debug = format!("{checkpoint:?}");
        assert!(!debug.contains("secret-response-id"));
    }

    #[test]
    fn malformed_wrong_version_and_history_mismatch_fall_back_safely() {
        let target = target(ResponsesContinuationPolicy::PreviousResponseId);
        let mut conversation = conversation_with_checkpoint(&target);

        conversation.items[1].checkpoint = Some(carrier_from_token("%%%".into()).unwrap());
        assert!(matches!(
            plan(&target, &conversation).unwrap(),
            ConversationPlan::FullReplay { .. }
        ));

        let mut conversation = conversation_with_checkpoint(&target);
        conversation.instructions.blocks[0].content = "changed instructions".into();
        assert!(matches!(
            plan(&target, &conversation).unwrap(),
            ConversationPlan::FullReplay { .. }
        ));

        let mut conversation = conversation_with_checkpoint(&target);
        let checkpoint = conversation.items[1].checkpoint.as_ref().unwrap();
        let mut envelope = decode(checkpoint, &target).unwrap();
        envelope.version = FORMAT_VERSION + 1;
        let token = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&envelope).unwrap());
        conversation.items[1].checkpoint = Some(carrier_from_token(token).unwrap());
        assert!(matches!(
            plan(&target, &conversation).unwrap(),
            ConversationPlan::FullReplay { .. }
        ));

        let mut conversation = conversation_with_checkpoint(&target);
        conversation.items[0].kind = crate::gateway::ConversationItemKind::User {
            content: piko_protocol::MessageContent::String("changed".into()),
        };
        assert!(matches!(
            plan(&target, &conversation).unwrap(),
            ConversationPlan::FullReplay { .. }
        ));

        let mut conversation = conversation_with_checkpoint(&target);
        conversation.items[1].kind = crate::gateway::ConversationItemKind::Assistant {
            content: vec![piko_protocol::ContentBlock::Text {
                text: "tampered output".into(),
            }],
        };
        assert!(matches!(
            plan(&target, &conversation).unwrap(),
            ConversationPlan::FullReplay { .. }
        ));
    }

    #[test]
    fn target_and_continuation_configuration_are_checkpoint_bound() {
        let original = target(ResponsesContinuationPolicy::PreviousResponseId);
        let conversation = conversation_with_checkpoint(&original);

        let changed_policy = target(ResponsesContinuationPolicy::EncryptedReasoning);
        assert!(matches!(
            plan(&changed_policy, &conversation).unwrap(),
            ConversationPlan::FullReplay { .. }
        ));

        let mut changed_protocol = target(ResponsesContinuationPolicy::PreviousResponseId);
        changed_protocol.protocol = ProtocolProfile::ChatCompletions;
        assert!(matches!(
            plan(&changed_protocol, &conversation).unwrap(),
            ConversationPlan::FullReplay { .. }
        ));

        let mut changed_target = target(ResponsesContinuationPolicy::PreviousResponseId);
        changed_target.api_surface = "other-surface".into();
        assert!(matches!(
            plan(&changed_target, &conversation).unwrap(),
            ConversationPlan::FullReplay { .. }
        ));

        let mut changed_model = target(ResponsesContinuationPolicy::PreviousResponseId);
        changed_model.model = "other-model".into();
        assert!(matches!(
            plan(&changed_model, &conversation).unwrap(),
            ConversationPlan::FullReplay { .. }
        ));

        let mut changed_provider_target = target(ResponsesContinuationPolicy::PreviousResponseId);
        changed_provider_target.id = "other-provider/gpt@responses".into();
        assert!(matches!(
            plan(&changed_provider_target, &conversation).unwrap(),
            ConversationPlan::FullReplay { .. }
        ));
    }

    #[test]
    fn newest_compatible_checkpoint_wins_over_newer_malformed_token() {
        let target = target(ResponsesContinuationPolicy::PreviousResponseId);
        let mut conversation = conversation_with_checkpoint(&target);
        conversation.items.last_mut().unwrap().checkpoint =
            Some(carrier_from_token("malformed".into()).unwrap());
        let ConversationPlan::Resume { suffix, .. } = plan(&target, &conversation).unwrap() else {
            panic!("expected the older compatible checkpoint");
        };
        assert_eq!(suffix.len(), conversation.items.len() - 2);
    }

    #[test]
    fn oversized_and_arbitrary_tokens_are_bounded_and_redacted() {
        let target = target(ResponsesContinuationPolicy::PreviousResponseId);
        let oversized = carrier_from_token("x".repeat(MAX_TOKEN_BYTES)).unwrap();
        let error = decode(&oversized, &target).unwrap_err();
        assert_eq!(error.class, ErrorClass::CheckpointRejected);
        assert!(!error.to_string().contains(&"x".repeat(64)));

        for length in [0, 1, 2, 3, 31, 127, 1_024, MAX_TOKEN_BYTES] {
            let token = (0..length)
                .map(|index| match index % 4 {
                    0 => 'A',
                    1 => '/',
                    2 => '_',
                    _ => '%',
                })
                .collect();
            let checkpoint = carrier_from_token(token).unwrap();
            let _ = decode(&checkpoint, &target);
        }
    }

    #[test]
    fn non_replayable_target_returns_typed_continuation_failure() {
        let mut target = target(ResponsesContinuationPolicy::PreviousResponseId);
        target.capabilities.replay_safe = false;
        let conversation = crate::protocols::tests_support::semantic_request().conversation;
        assert_eq!(
            plan(&target, &conversation).unwrap_err().class,
            ErrorClass::ContinuationUnavailable
        );
    }

    #[test]
    fn compaction_that_removes_the_anchor_invalidates_checkpoint() {
        let target = target(ResponsesContinuationPolicy::PreviousResponseId);
        let mut conversation = conversation_with_checkpoint(&target);
        conversation.items.remove(0);
        assert!(matches!(
            plan(&target, &conversation).unwrap(),
            ConversationPlan::FullReplay { .. }
        ));
    }
}
