use std::collections::VecDeque;

use piko_comms::BroadcastSender;
use piko_comms::contracts::{SessionRealtimeObservation, SessionReliableObservation};
use tokio_stream::Stream;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use piko_protocol::agent_runtime::{
    RealtimeDeltaEnvelope, SessionCursor, SessionEventEnvelope, SessionOutput,
    SessionOutputEnvelope,
};

use crate::api::{SessionStreamError, SnapshotRequiredReason};

#[derive(Debug, Clone, thiserror::Error)]
#[error("event sink closed")]
pub struct SendError;

pub struct SessionOutputHub {
    session_id: String,
    epoch: String,
    reliable_tx: BroadcastSender<SessionReliableObservation, SessionEventEnvelope>,
    delta_tx: BroadcastSender<SessionRealtimeObservation, RealtimeDeltaEnvelope>,
    /// Sequence allocation, retention append, and subscription snapshots are
    /// serialized on this lock so cursors stay monotonic and replay/live
    /// handoff is exact.
    retention: tokio::sync::Mutex<RetentionState>,
    /// Mirror of `RetentionState.cursor_seq` for lock-free `cursor()` reads.
    cursor_seq: std::sync::atomic::AtomicU64,
}

struct RetentionState {
    cursor_seq: u64,
    events: VecDeque<SessionEventEnvelope>,
    limit: usize,
}

impl SessionOutputHub {
    pub fn new(session_id: String, epoch: String, buffer: usize) -> Self {
        let (reliable_tx, _) =
            piko_comms::broadcast::<SessionReliableObservation, SessionEventEnvelope>();
        let (delta_tx, _) =
            piko_comms::broadcast::<SessionRealtimeObservation, RealtimeDeltaEnvelope>();
        Self {
            session_id,
            epoch,
            reliable_tx,
            delta_tx,
            retention: tokio::sync::Mutex::new(RetentionState {
                cursor_seq: 0,
                events: VecDeque::with_capacity(buffer),
                limit: buffer,
            }),
            cursor_seq: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn cursor(&self) -> SessionCursor {
        SessionCursor {
            epoch: self.epoch.clone(),
            seq: self.cursor_seq.load(std::sync::atomic::Ordering::Acquire),
        }
    }

    pub async fn publish_event(&self, mut envelope: SessionEventEnvelope) -> Result<(), SendError> {
        // The retained reliable lane is authoritative for observation replay;
        // having no live subscribers must never fail or block the runtime.
        // Sequence allocation, retention append, and the broadcast happen
        // under one lock so live delivery order always equals cursor order.
        let mut retention = self.retention.lock().await;
        let seq = retention.cursor_seq + 1;
        retention.cursor_seq = seq;
        envelope.cursor = SessionCursor {
            epoch: self.epoch.clone(),
            seq,
        };
        retention.events.push_back(envelope.clone());
        while retention.events.len() > retention.limit {
            retention.events.pop_front();
        }
        self.cursor_seq
            .store(seq, std::sync::atomic::Ordering::Release);
        let _ = self.reliable_tx.send(envelope);
        drop(retention);
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
        // Create the live receiver while the retention lock is held. Publishers
        // use the same lock, so every event is either in this replay snapshot
        // or sent after the receiver exists (duplicates are filtered by seq).
        let (replay, reliable) = {
            let retention = self.retention.lock().await;
            if after.seq > retention.cursor_seq {
                return Err(SnapshotRequiredReason::CursorUnknown);
            }
            if let Some(first) = retention.events.front()
                && after.seq.saturating_add(1) < first.cursor.seq
            {
                return Err(SnapshotRequiredReason::CursorExpired);
            }
            let reliable = self.reliable_tx.subscribe();
            let replay = retention
                .events
                .iter()
                .filter(|event| event.cursor.seq > after.seq)
                .cloned()
                .collect::<VecDeque<_>>();
            (replay, reliable)
        };
        let delta = self.delta_tx.subscribe();
        Ok(SessionHubSubscription {
            session_id: self.session_id.clone(),
            reliable: reliable.into_stream(),
            delta: delta.into_stream(),
            replay,
            last_emitted_seq: after.seq,
        })
    }
}

pub struct SessionHubSubscription {
    session_id: String,
    reliable: BroadcastStream<SessionEventEnvelope>,
    delta: BroadcastStream<RealtimeDeltaEnvelope>,
    replay: VecDeque<SessionEventEnvelope>,
    /// Highest sequence emitted so far. Live events at or below it duplicate
    /// replay entries and are skipped.
    last_emitted_seq: u64,
}

impl SessionHubSubscription {
    fn accept_event(&mut self, envelope: &SessionEventEnvelope) -> bool {
        if envelope.cursor.seq <= self.last_emitted_seq {
            return false;
        }
        self.last_emitted_seq = envelope.cursor.seq;
        true
    }
}

pub fn merged_output_stream(
    mut subscription: SessionHubSubscription,
    cursor: SessionCursor,
) -> std::pin::Pin<Box<dyn Stream<Item = Result<SessionOutputEnvelope, SessionStreamError>> + Send>>
{
    let session_id = subscription.session_id.clone();
    let after_epoch = cursor.epoch.clone();
    Box::pin(async_stream::stream! {
        while let Some(envelope) = subscription.replay.pop_front() {
            if !subscription.accept_event(&envelope) {
                continue;
            }
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
                            if !subscription.accept_event(&envelope) {
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
            cursor: SessionCursor {
                epoch: String::new(),
                seq: 0,
            },
            event: SessionEvent::MessageCommitted {
                transcript_seq: 1,
                message_id: message_id.into(),
                root_input_id: "turn".into(),
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

    #[tokio::test]
    async fn replayed_event_is_not_duplicated_by_live_lane() {
        let hub = SessionOutputHub::new("session".into(), "epoch".into(), 4);
        let cursor = hub.cursor();
        hub.publish_event(event("exec-1")).await.unwrap();
        // A publish after the retention snapshot but before the broadcast
        // subscription lands in both replay and the live receiver buffer.
        let subscription = hub.subscribe(&cursor).await.unwrap();
        hub.publish_event(event("exec-1")).await.unwrap();

        let mut stream = merged_output_stream(subscription, cursor);
        let first = stream.next().await.unwrap().unwrap();
        let SessionOutput::Event(first) = first.output else {
            panic!("expected reliable event");
        };
        let second = stream.next().await.unwrap().unwrap();
        let SessionOutput::Event(second) = second.output else {
            panic!("expected reliable event");
        };
        assert_eq!(first.cursor.seq, 1);
        assert_eq!(second.cursor.seq, 2);
    }

    #[tokio::test]
    async fn concurrent_publishers_keep_cursor_monotonic() {
        let hub = std::sync::Arc::new(SessionOutputHub::new("session".into(), "epoch".into(), 128));
        let mut publishers = Vec::new();
        for publisher in 0..8_u32 {
            let hub = std::sync::Arc::clone(&hub);
            publishers.push(tokio::spawn(async move {
                for index in 0..32_u32 {
                    let mut envelope = event("concurrent");
                    envelope.event = SessionEvent::MessageCommitted {
                        transcript_seq: u64::from(publisher * 32 + index),
                        message_id: format!("m-{publisher}-{index}"),
                        root_input_id: "turn".into(),
                        role: MessageRole::Assistant,
                    };
                    hub.publish_event(envelope).await.unwrap();
                }
            }));
        }
        for publisher in publishers {
            publisher.await.unwrap();
        }
        let retention = hub.retention.lock().await;
        let seqs: Vec<u64> = retention.events.iter().map(|e| e.cursor.seq).collect();
        assert_eq!(seqs.len(), 128);
        for pair in seqs.windows(2) {
            assert_eq!(pair[1], pair[0] + 1);
        }
        assert_eq!(retention.cursor_seq, 256);
    }
}
