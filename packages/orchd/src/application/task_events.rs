use futures_core::Stream;
use piko_protocol::Event;
use tokio::sync::{Mutex, mpsc};
use tokio_stream::wrappers::ReceiverStream;

#[derive(Clone)]
pub(crate) struct RuntimeEventBus {
    capacity: usize,
    subscribers: std::sync::Arc<Mutex<Vec<mpsc::Sender<Event>>>>,
}

impl RuntimeEventBus {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            subscribers: std::sync::Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(crate) async fn publish(&self, event: Event) {
        let subscribers = self.subscribers.lock().await.clone();
        for tx in subscribers {
            let _ = tx.send(event.clone()).await;
        }
        self.subscribers.lock().await.retain(|tx| !tx.is_closed());
    }

    pub(crate) async fn subscribe(&self) -> impl Stream<Item = Event> + Send + 'static {
        let (tx, rx) = mpsc::channel(self.capacity);
        self.subscribers.lock().await.push(tx);
        ReceiverStream::new(rx)
    }
}
