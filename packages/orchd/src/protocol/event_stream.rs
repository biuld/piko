// ---- Protocol: event_stream — async push-based event stream ----
//
// In Rust, we use native `Stream` trait from `futures-core` instead of a custom EventStream class.
// The TS `EventStream<T, R>` pattern (stream of T events + final R result) is expressed as:
//   - `Pin<Box<dyn Stream<Item = T> + Send>>` for the stream part
//   - A `tokio::sync::oneshot` channel for the final result
//
// This module provides convenience wrappers.

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use tokio::sync::oneshot;

/// A stream of events of type T with a deferred final result of type R.
pub struct EventStream<T, R> {
    inner: Pin<Box<dyn Stream<Item = T> + Send>>,
    result_rx: oneshot::Receiver<R>,
}

impl<T, R> EventStream<T, R> {
    /// Create a new EventStream from a raw stream and a oneshot receiver for the final result.
    pub fn new(
        inner: Pin<Box<dyn Stream<Item = T> + Send>>,
        result_rx: oneshot::Receiver<R>,
    ) -> Self {
        Self { inner, result_rx }
    }

    /// Await the final result. Consumes the stream.
    pub async fn result(self) -> Result<R, oneshot::error::RecvError> {
        self.result_rx.await
    }
}

impl<T, R> Stream for EventStream<T, R> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

/// Create a push-based event stream (like TS EventStream) using a channel.
///
/// Returns (sender, EventStream) tuple.
/// Call `sender.send(event)` to push events, `sender.finalize(result)` to end with result.
pub fn create_event_stream<T: Send + 'static, R: Send + 'static>(
    buffer: usize,
) -> (EventStreamSender<T, R>, EventStream<T, R>) {
    let (event_tx, event_rx) = tokio::sync::mpsc::channel(buffer);
    let (result_tx, result_rx) = oneshot::channel();

    let sender = EventStreamSender {
        event_tx,
        result_tx: Some(result_tx),
    };

    let stream = EventStream {
        inner: Box::pin(tokio_stream::wrappers::ReceiverStream::new(event_rx)),
        result_rx,
    };

    (sender, stream)
}

pub struct EventStreamSender<T, R> {
    event_tx: tokio::sync::mpsc::Sender<T>,
    result_tx: Option<oneshot::Sender<R>>,
}

impl<T, R> EventStreamSender<T, R> {
    /// Push an event into the stream.
    pub async fn send(&self, event: T) -> Result<(), tokio::sync::mpsc::error::SendError<T>> {
        self.event_tx.send(event).await
    }

    /// End the stream with a final result.
    pub fn finalize(mut self, result: R) -> Result<(), R> {
        if let Some(tx) = self.result_tx.take() {
            tx.send(result)
        } else {
            Err(result)
        }
    }
}
