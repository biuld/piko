use std::collections::VecDeque;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;
use tokio_stream::Stream;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use piko_protocol::agent_runtime::{
    RealtimeDeltaEnvelope, SessionCursor, SessionEventEnvelope, SessionOutput,
    SessionOutputEnvelope,
};

use crate::api::{SessionStreamError, SnapshotRequiredReason};

#[async_trait]
pub trait EventSink<T>: Send + Sync {
    async fn send(&self, event: T) -> Result<(), SendError>;
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("event sink closed")]
pub struct SendError;

pub struct SessionOutputHub {
    session_id: String,
    epoch: String,
    reliable_tx: broadcast::Sender<SessionEventEnvelope>,
    delta_tx: broadcast::Sender<RealtimeDeltaEnvelope>,
    cursor_seq: std::sync::atomic::AtomicU64,
    reliable_retention: tokio::sync::Mutex<VecDeque<SessionEventEnvelope>>,
    retention_limit: usize,
}

impl SessionOutputHub {
    pub fn new(session_id: String, epoch: String, buffer: usize) -> Self {
        let (reliable_tx, _) = broadcast::channel(buffer);
        let (delta_tx, _) = broadcast::channel(buffer * 4);
        Self {
            session_id,
            epoch,
            reliable_tx,
            delta_tx,
            cursor_seq: std::sync::atomic::AtomicU64::new(0),
            reliable_retention: tokio::sync::Mutex::new(VecDeque::with_capacity(buffer)),
            retention_limit: buffer,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn cursor(&self) -> SessionCursor {
        SessionCursor {
            epoch: self.epoch.clone(),
            seq: self.cursor_seq.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    pub async fn publish_event(&self, mut envelope: SessionEventEnvelope) -> Result<(), SendError> {
        let seq = self
            .cursor_seq
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        envelope.cursor = SessionCursor {
            epoch: self.epoch.clone(),
            seq,
        };
        let mut retention = self.reliable_retention.lock().await;
        retention.push_back(envelope.clone());
        while retention.len() > self.retention_limit {
            retention.pop_front();
        }
        drop(retention);
        // The retained reliable lane is authoritative for observation replay;
        // having no live subscribers must never fail or block the runtime.
        let _ = self.reliable_tx.send(envelope);
        Ok(())
    }

    pub async fn publish_delta(&self, envelope: RealtimeDeltaEnvelope) -> Result<(), SendError> {
        // Realtime deltas are best effort and may be dropped without subscribers.
        let _ = self.delta_tx.send(envelope);
        Ok(())
    }

    pub fn try_publish_delta(&self, envelope: RealtimeDeltaEnvelope) {
        let _ = self.delta_tx.send(envelope);
    }

    pub async fn subscribe(
        &self,
        after: &SessionCursor,
    ) -> Result<SessionHubSubscription, SnapshotRequiredReason> {
        if after.epoch != self.epoch {
            return Err(SnapshotRequiredReason::EpochChanged);
        }
        let reliable = self.reliable_tx.subscribe();
        let delta = self.delta_tx.subscribe();
        let retention = self.reliable_retention.lock().await;
        let current = self.cursor_seq.load(std::sync::atomic::Ordering::Relaxed);
        if after.seq > current {
            return Err(SnapshotRequiredReason::CursorUnknown);
        }
        if let Some(first) = retention.front()
            && after.seq.saturating_add(1) < first.cursor.seq
        {
            return Err(SnapshotRequiredReason::CursorExpired);
        }
        let replay = retention
            .iter()
            .filter(|event| event.cursor.seq > after.seq)
            .cloned()
            .collect();
        Ok(SessionHubSubscription {
            session_id: self.session_id.clone(),
            reliable: BroadcastStream::new(reliable),
            delta: BroadcastStream::new(delta),
            replay,
        })
    }
}

pub struct SessionHubSubscription {
    session_id: String,
    reliable: BroadcastStream<SessionEventEnvelope>,
    delta: BroadcastStream<RealtimeDeltaEnvelope>,
    replay: VecDeque<SessionEventEnvelope>,
}

pub fn merged_output_stream(
    mut subscription: SessionHubSubscription,
    cursor: SessionCursor,
) -> std::pin::Pin<Box<dyn Stream<Item = Result<SessionOutputEnvelope, SessionStreamError>> + Send>>
{
    let session_id = subscription.session_id.clone();
    let after_epoch = cursor.epoch.clone();
    let after_seq = cursor.seq;
    Box::pin(async_stream::stream! {
        while let Some(envelope) = subscription.replay.pop_front() {
            yield Ok(SessionOutputEnvelope {
                session_id: session_id.clone(),
                emitted_at: crate::ports::clock::now_ms(),
                output: SessionOutput::Event(envelope),
            });
        }
        loop {
            tokio::select! {
                event = subscription.reliable.next() => {
                    match event {
                        Some(Ok(envelope)) => {
                            if envelope.cursor.epoch != after_epoch {
                                yield Err(SessionStreamError::SnapshotRequired {
                                    reason: SnapshotRequiredReason::EpochChanged,
                                });
                                break;
                            }
                            if envelope.cursor.seq <= after_seq {
                                continue;
                            }
                            yield Ok(SessionOutputEnvelope {
                                session_id: session_id.clone(),
                                emitted_at: crate::ports::clock::now_ms(),
                                output: SessionOutput::Event(envelope),
                            });
                        }
                        Some(Err(_)) => {
                            yield Err(SessionStreamError::SnapshotRequired {
                                reason: SnapshotRequiredReason::CursorExpired,
                            });
                            break;
                        }
                        None => break,
                    }
                }
                delta = subscription.delta.next() => {
                    match delta {
                        Some(Ok(envelope)) => {
                            yield Ok(SessionOutputEnvelope {
                                session_id: session_id.clone(),
                                emitted_at: crate::ports::clock::now_ms(),
                                output: SessionOutput::Delta(envelope),
                            });
                        }
                        Some(Err(_)) => continue,
                        None => break,
                    }
                }
            }
        }
    })
}

pub type SharedSessionOutputHub = Arc<SessionOutputHub>;

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;
    use piko_protocol::MessageRole;
    use piko_protocol::agent_runtime::SessionEvent;

    use super::*;

    fn event(message_id: &str) -> SessionEventEnvelope {
        SessionEventEnvelope {
            agent_instance_id: "root".into(),
            agent_id: "agent".into(),
            transcript_seq: 1,
            cursor: SessionCursor {
                epoch: String::new(),
                seq: 0,
            },
            event: SessionEvent::MessageCommitted {
                message_id: message_id.into(),
                work_id: "turn".into(),
                role: MessageRole::Assistant,
            },
        }
    }

    #[tokio::test]
    async fn replays_reliable_events_after_cursor() {
        let hub = SessionOutputHub::new("session".into(), "epoch".into(), 4);
        let cursor = hub.cursor();
        hub.publish_event(event("exec-1")).await.unwrap();

        let subscription = hub.subscribe(&cursor).await.unwrap();
        let mut stream = merged_output_stream(subscription, cursor);
        let output = stream.next().await.unwrap().unwrap();
        let SessionOutput::Event(output) = output.output else {
            panic!("expected reliable event");
        };
        assert!(matches!(
            output.event,
            SessionEvent::MessageCommitted { .. }
        ));
    }

    #[tokio::test]
    async fn rejects_expired_cursor_and_replays_session_events() {
        let hub = SessionOutputHub::new("session".into(), "epoch".into(), 1);
        let original = hub.cursor();
        hub.publish_event(event("exec-1")).await.unwrap();
        let after_first = hub.cursor();
        hub.publish_event(event("exec-2")).await.unwrap();

        assert!(matches!(
            hub.subscribe(&original).await,
            Err(SnapshotRequiredReason::CursorExpired)
        ));

        let subscription = hub.subscribe(&after_first).await.unwrap();
        let mut stream = merged_output_stream(subscription, after_first);
        let output = stream.next().await.unwrap().unwrap();
        let SessionOutput::Event(output) = output.output else {
            panic!("expected reliable event");
        };
        assert!(matches!(
            output.event,
            SessionEvent::MessageCommitted { .. }
        ));
    }
}
