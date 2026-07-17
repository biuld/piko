use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::sync::{broadcast as tokio_broadcast, mpsc, oneshot, watch};
use tokio_stream::wrappers::BroadcastStream;

use crate::{
    BroadcastContract, CapacityPolicy, CommunicationKind, LatestContract, MailboxContract,
    ReplyContract, ThreadBridgeContract,
};

pub struct MailboxSender<C, T> {
    inner: mpsc::Sender<T>,
    marker: PhantomData<fn() -> C>,
}

impl<C, T> Clone for MailboxSender<C, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            marker: PhantomData,
        }
    }
}

impl<C: MailboxContract, T> MailboxSender<C, T> {
    pub async fn send(&self, value: T) -> Result<(), mpsc::error::SendError<T>> {
        let started = std::time::Instant::now();
        let result = self.inner.send(value).await;
        tracing::trace!(
            communication_id = C::SPEC.id,
            wait_micros = started.elapsed().as_micros() as u64,
            remaining_capacity = self.inner.capacity(),
            success = result.is_ok(),
            "communication mailbox send"
        );
        result
    }

    pub fn try_send(&self, value: T) -> Result<(), mpsc::error::TrySendError<T>> {
        let result = self.inner.try_send(value);
        if let Err(error) = &result {
            tracing::debug!(
                communication_id = C::SPEC.id,
                remaining_capacity = self.inner.capacity(),
                error = %error,
                "communication mailbox rejected send"
            );
        }
        result
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    pub fn max_capacity(&self) -> usize {
        self.inner.max_capacity()
    }

    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
}

pub struct MailboxReceiver<C, T> {
    inner: mpsc::Receiver<T>,
    marker: PhantomData<fn() -> C>,
}

impl<C: MailboxContract, T> MailboxReceiver<C, T> {
    pub async fn recv(&mut self) -> Option<T> {
        let value = self.inner.recv().await;
        if value.is_none() {
            tracing::debug!(
                communication_id = C::SPEC.id,
                "communication mailbox closed"
            );
        }
        value
    }

    pub fn try_recv(&mut self) -> Result<T, mpsc::error::TryRecvError> {
        self.inner.try_recv()
    }

    pub fn close(&mut self) {
        self.inner.close();
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

pub fn mailbox<C: MailboxContract, T>() -> (MailboxSender<C, T>, MailboxReceiver<C, T>) {
    assert!(matches!(
        C::SPEC.kind,
        CommunicationKind::Mailbox | CommunicationKind::ClientOutput
    ));
    let CapacityPolicy::Bounded(capacity) = C::SPEC.capacity else {
        panic!("mailbox contract {} must be bounded", C::SPEC.id);
    };
    let (sender, receiver) = mpsc::channel(capacity);
    (
        MailboxSender {
            inner: sender,
            marker: PhantomData,
        },
        MailboxReceiver {
            inner: receiver,
            marker: PhantomData,
        },
    )
}

pub struct ReplySender<C, T> {
    inner: Option<oneshot::Sender<T>>,
    marker: PhantomData<fn() -> C>,
}

impl<C: ReplyContract, T> ReplySender<C, T> {
    pub fn send(mut self, value: T) -> Result<(), T> {
        let sender = self.inner.take().expect("reply sender may only send once");
        let result = sender.send(value);
        tracing::trace!(
            communication_id = C::SPEC.id,
            success = result.is_ok(),
            "communication reply send"
        );
        result
    }

    pub fn is_closed(&self) -> bool {
        self.inner
            .as_ref()
            .is_none_or(tokio::sync::oneshot::Sender::is_closed)
    }
}

pub struct ReplyReceiver<C, T> {
    inner: oneshot::Receiver<T>,
    marker: PhantomData<fn() -> C>,
}

impl<C: ReplyContract, T> ReplyReceiver<C, T> {
    pub fn close(&mut self) {
        self.inner.close();
    }

    pub fn try_recv(&mut self) -> Result<T, oneshot::error::TryRecvError> {
        self.inner.try_recv()
    }
}

impl<C: ReplyContract, T> Future for ReplyReceiver<C, T> {
    type Output = Result<T, oneshot::error::RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.inner).poll(cx)
    }
}

pub fn reply<C: ReplyContract, T>() -> (ReplySender<C, T>, ReplyReceiver<C, T>) {
    assert_eq!(C::SPEC.kind, CommunicationKind::Reply);
    let (sender, receiver) = oneshot::channel();
    (
        ReplySender {
            inner: Some(sender),
            marker: PhantomData,
        },
        ReplyReceiver {
            inner: receiver,
            marker: PhantomData,
        },
    )
}

pub struct LatestSender<C, T> {
    inner: watch::Sender<T>,
    marker: PhantomData<fn() -> C>,
}

impl<C: LatestContract, T> LatestSender<C, T> {
    pub fn send(&self, value: T) -> Result<(), watch::error::SendError<T>> {
        self.inner.send(value)
    }

    pub fn send_replace(&self, value: T) -> T {
        self.inner.send_replace(value)
    }
}

pub struct LatestReceiver<C, T> {
    inner: watch::Receiver<T>,
    marker: PhantomData<fn() -> C>,
}

impl<C, T> Clone for LatestReceiver<C, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            marker: PhantomData,
        }
    }
}

impl<C: LatestContract, T> LatestReceiver<C, T> {
    pub fn borrow(&self) -> watch::Ref<'_, T> {
        self.inner.borrow()
    }

    pub fn borrow_and_update(&mut self) -> watch::Ref<'_, T> {
        self.inner.borrow_and_update()
    }

    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        self.inner.changed().await
    }
}

pub fn latest<C: LatestContract, T>(value: T) -> (LatestSender<C, T>, LatestReceiver<C, T>) {
    assert_eq!(C::SPEC.kind, CommunicationKind::LatestState);
    let (sender, receiver) = watch::channel(value);
    (
        LatestSender {
            inner: sender,
            marker: PhantomData,
        },
        LatestReceiver {
            inner: receiver,
            marker: PhantomData,
        },
    )
}

pub struct BroadcastSender<C, T> {
    inner: tokio_broadcast::Sender<T>,
    marker: PhantomData<fn() -> C>,
}

impl<C, T> Clone for BroadcastSender<C, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            marker: PhantomData,
        }
    }
}

impl<C: BroadcastContract, T: Clone> BroadcastSender<C, T> {
    pub fn send(&self, value: T) -> Result<usize, tokio_broadcast::error::SendError<T>> {
        self.inner.send(value)
    }

    pub fn subscribe(&self) -> BroadcastReceiver<C, T> {
        BroadcastReceiver {
            inner: Some(self.inner.subscribe()),
            marker: PhantomData,
        }
    }

    pub fn receiver_count(&self) -> usize {
        self.inner.receiver_count()
    }
}

pub struct BroadcastReceiver<C, T> {
    inner: Option<tokio_broadcast::Receiver<T>>,
    marker: PhantomData<fn() -> C>,
}

impl<C: BroadcastContract, T: Clone> BroadcastReceiver<C, T> {
    pub async fn recv(&mut self) -> Result<T, tokio_broadcast::error::RecvError> {
        self.inner
            .as_mut()
            .expect("broadcast receiver already converted into stream")
            .recv()
            .await
    }

    pub fn into_stream(mut self) -> BroadcastStream<T>
    where
        T: Send + 'static,
    {
        BroadcastStream::new(
            self.inner
                .take()
                .expect("broadcast receiver already converted into stream"),
        )
    }
}

pub fn broadcast<C: BroadcastContract, T: Clone>()
-> (BroadcastSender<C, T>, BroadcastReceiver<C, T>) {
    assert_eq!(C::SPEC.kind, CommunicationKind::Observation);
    let CapacityPolicy::Bounded(capacity) = C::SPEC.capacity else {
        panic!("broadcast contract {} must be bounded", C::SPEC.id);
    };
    let (sender, receiver) = tokio_broadcast::channel(capacity);
    (
        BroadcastSender {
            inner: sender,
            marker: PhantomData,
        },
        BroadcastReceiver {
            inner: Some(receiver),
            marker: PhantomData,
        },
    )
}

pub struct ThreadBridgeSender<C, T> {
    inner: std::sync::mpsc::Sender<T>,
    marker: PhantomData<fn() -> C>,
}

impl<C, T> Clone for ThreadBridgeSender<C, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            marker: PhantomData,
        }
    }
}

impl<C: ThreadBridgeContract, T> ThreadBridgeSender<C, T> {
    pub fn send(&self, value: T) -> Result<(), std::sync::mpsc::SendError<T>> {
        self.inner.send(value)
    }
}

pub struct ThreadBridgeReceiver<C, T> {
    inner: std::sync::mpsc::Receiver<T>,
    marker: PhantomData<fn() -> C>,
}

impl<C: ThreadBridgeContract, T> ThreadBridgeReceiver<C, T> {
    pub fn recv(&self) -> Result<T, std::sync::mpsc::RecvError> {
        self.inner.recv()
    }

    pub fn try_recv(&self) -> Result<T, std::sync::mpsc::TryRecvError> {
        self.inner.try_recv()
    }
}

pub fn thread_bridge<C: ThreadBridgeContract, T>()
-> (ThreadBridgeSender<C, T>, ThreadBridgeReceiver<C, T>) {
    assert_eq!(C::SPEC.kind, CommunicationKind::ThreadBridge);
    let (sender, receiver) = std::sync::mpsc::channel();
    (
        ThreadBridgeSender {
            inner: sender,
            marker: PhantomData,
        },
        ThreadBridgeReceiver {
            inner: receiver,
            marker: PhantomData,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{AgentCommandReply, AgentCommands, AgentSnapshot};

    #[tokio::test]
    async fn typed_mailbox_and_reply_round_trip() {
        let (commands, mut mailbox) = mailbox::<AgentCommands, u32>();
        let (reply_tx, reply_rx) = reply::<AgentCommandReply, u32>();
        commands.send(41).await.unwrap();
        assert_eq!(mailbox.recv().await, Some(41));
        reply_tx.send(42).unwrap();
        assert_eq!(reply_rx.await.unwrap(), 42);
    }

    #[test]
    fn typed_latest_state_replaces_value() {
        let (tx, rx) = latest::<AgentSnapshot, u32>(1);
        tx.send(2).unwrap();
        assert_eq!(*rx.borrow(), 2);
    }
}
