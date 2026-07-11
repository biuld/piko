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

use crate::api::SessionStreamError;

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
        self.reliable_tx.send(envelope).map_err(|_| SendError)?;
        Ok(())
    }

    pub async fn publish_delta(&self, envelope: RealtimeDeltaEnvelope) -> Result<(), SendError> {
        self.delta_tx.send(envelope).map_err(|_| SendError)?;
        Ok(())
    }

    pub fn subscribe(&self) -> SessionHubSubscription {
        SessionHubSubscription {
            session_id: self.session_id.clone(),
            reliable: BroadcastStream::new(self.reliable_tx.subscribe()),
            delta: BroadcastStream::new(self.delta_tx.subscribe()),
        }
    }
}

pub struct SessionHubSubscription {
    session_id: String,
    reliable: BroadcastStream<SessionEventEnvelope>,
    delta: BroadcastStream<RealtimeDeltaEnvelope>,
}

pub fn merged_output_stream(
    mut subscription: SessionHubSubscription,
    cursor: SessionCursor,
) -> std::pin::Pin<Box<dyn Stream<Item = Result<SessionOutputEnvelope, SessionStreamError>> + Send>>
{
    let session_id = subscription.session_id.clone();
    Box::pin(async_stream::stream! {
        let _ = cursor;
        loop {
            tokio::select! {
                event = subscription.reliable.next() => {
                    match event {
                        Some(Ok(envelope)) => {
                            yield Ok(SessionOutputEnvelope {
                                session_id: session_id.clone(),
                                emitted_at: crate::runtime::utils::now_ms(),
                                output: SessionOutput::Event(envelope),
                            });
                        }
                        Some(Err(_)) => continue,
                        None => break,
                    }
                }
                delta = subscription.delta.next() => {
                    match delta {
                        Some(Ok(envelope)) => {
                            yield Ok(SessionOutputEnvelope {
                                session_id: session_id.clone(),
                                emitted_at: crate::runtime::utils::now_ms(),
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
