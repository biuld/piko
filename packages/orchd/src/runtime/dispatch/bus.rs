use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;

use piko_protocol::SessionId;

use super::{DisplayEvent, LifecycleEvent, PersistEvent};

#[derive(Debug, Clone)]
pub struct ChannelConfig {
    pub persist_buffer: usize,
    pub display_buffer: usize,
    pub lifecycle_buffer: usize,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            persist_buffer: 64,
            display_buffer: 256,
            lifecycle_buffer: 64,
        }
    }
}

#[async_trait]
pub trait Dispatch: Send {
    fn name(&self) -> &str;

    async fn run(
        &mut self,
        persist_tx: mpsc::Sender<Arc<PersistEvent>>,
        display_tx: mpsc::Sender<Arc<DisplayEvent>>,
        lifecycle_tx: Option<mpsc::Sender<Arc<LifecycleEvent>>>,
    );
}

pub struct SessionChannels {
    persist_tx: mpsc::Sender<Arc<PersistEvent>>,
    persist_rx: Option<mpsc::Receiver<Arc<PersistEvent>>>,
    display_tx: mpsc::Sender<Arc<DisplayEvent>>,
    display_rx: Option<mpsc::Receiver<Arc<DisplayEvent>>>,
    lifecycle_input_tx: mpsc::UnboundedSender<LifecycleEvent>,
    lifecycle_input_rx: Option<mpsc::UnboundedReceiver<LifecycleEvent>>,
    lifecycle_tx: mpsc::Sender<Arc<LifecycleEvent>>,
    lifecycle_rx: Option<mpsc::Receiver<Arc<LifecycleEvent>>>,
}

#[derive(Clone, Debug)]
pub struct DispatchSenders {
    pub persist: mpsc::Sender<Arc<PersistEvent>>,
    pub display: mpsc::Sender<Arc<DisplayEvent>>,
    pub lifecycle: mpsc::UnboundedSender<LifecycleEvent>,
}

impl SessionChannels {
    pub fn new(config: ChannelConfig) -> Self {
        let (persist_tx, persist_rx) = mpsc::channel(config.persist_buffer);
        let (display_tx, display_rx) = mpsc::channel(config.display_buffer);
        let (lifecycle_input_tx, lifecycle_input_rx) = mpsc::unbounded_channel();
        let (lifecycle_tx, lifecycle_rx) = mpsc::channel(config.lifecycle_buffer);
        Self {
            persist_tx,
            persist_rx: Some(persist_rx),
            display_tx,
            display_rx: Some(display_rx),
            lifecycle_input_tx,
            lifecycle_input_rx: Some(lifecycle_input_rx),
            lifecycle_tx,
            lifecycle_rx: Some(lifecycle_rx),
        }
    }

    pub fn spawn_lifecycle_dispatch(&mut self, session_id: SessionId) -> JoinHandle<()> {
        let rx = self
            .lifecycle_input_rx
            .take()
            .expect("lifecycle dispatch already spawned");
        self.spawn_dispatch(
            super::LifecycleDispatch::new(session_id.clone(), rx),
            session_id,
        )
    }

    pub fn spawn_dispatch<D>(&self, mut dispatch: D, _session_id: SessionId) -> JoinHandle<()>
    where
        D: Dispatch + 'static,
    {
        let persist_tx = self.persist_tx.clone();
        let display_tx = self.display_tx.clone();
        let lifecycle_tx = self.lifecycle_tx.clone();
        tokio::spawn(async move {
            dispatch
                .run(persist_tx, display_tx, Some(lifecycle_tx))
                .await;
        })
    }

    pub fn persist_stream(&mut self) -> Option<ReceiverStream<Arc<PersistEvent>>> {
        self.persist_rx.take().map(ReceiverStream::new)
    }

    pub fn display_stream(&mut self) -> Option<ReceiverStream<Arc<DisplayEvent>>> {
        self.display_rx.take().map(ReceiverStream::new)
    }

    pub fn lifecycle_stream(&mut self) -> Option<ReceiverStream<Arc<LifecycleEvent>>> {
        self.lifecycle_rx.take().map(ReceiverStream::new)
    }

    pub fn set_lifecycle_stream(&mut self, rx: mpsc::Receiver<Arc<LifecycleEvent>>) {
        self.lifecycle_rx = Some(rx);
    }

    pub fn persist_sender(&self) -> mpsc::Sender<Arc<PersistEvent>> {
        self.persist_tx.clone()
    }

    pub fn display_sender(&self) -> mpsc::Sender<Arc<DisplayEvent>> {
        self.display_tx.clone()
    }

    pub fn lifecycle_sender(&self) -> mpsc::Sender<Arc<LifecycleEvent>> {
        self.lifecycle_tx.clone()
    }

    pub fn senders(&self) -> DispatchSenders {
        DispatchSenders {
            persist: self.persist_tx.clone(),
            display: self.display_tx.clone(),
            lifecycle: self.lifecycle_input_tx.clone(),
        }
    }
}
