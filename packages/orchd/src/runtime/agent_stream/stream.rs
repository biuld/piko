// ---- AgentStream — poll-driven event delivery ----

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use futures_core::Stream;
use futures_util::stream::{SelectAll, StreamExt};

use crate::domain::events::event::Event;

#[derive(Clone)]
pub(crate) struct AgentEventBuffer {
    inner: Arc<Mutex<AgentEventBufferState>>,
}

pub struct AgentStream {
    events: AgentEventBuffer,
    scheduler: RunTaskScheduler,
    run: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

#[derive(Clone)]
pub(crate) struct RunTaskScheduler {
    inner: Arc<Mutex<SelectAll<AgentStream>>>,
}

struct AgentEventBufferState {
    queue: VecDeque<Event>,
    done: bool,
    waker: Option<Waker>,
}

impl AgentEventBuffer {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AgentEventBufferState {
                queue: VecDeque::new(),
                done: false,
                waker: None,
            })),
        }
    }

    pub(crate) fn stream<F>(&self, scheduler: RunTaskScheduler, run: F) -> AgentStream
    where
        F: Future<Output = ()> + Send + 'static,
    {
        AgentStream {
            events: self.clone(),
            scheduler,
            run: Some(Box::pin(run)),
        }
    }

    pub(crate) fn push(&self, event: Event) {
        let waker = {
            let mut state = self.inner.lock().expect("event stream mutex poisoned");
            if state.done {
                return;
            }
            state.queue.push_back(event);
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    pub(crate) fn finish(&self) {
        let waker = {
            let mut state = self.inner.lock().expect("event stream mutex poisoned");
            state.done = true;
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl AgentStream {
    /// Create an AgentStream with pre-created event buffer and scheduler.
    /// The run future will execute, pushing events into the buffer.
    /// When the future completes, the buffer is marked done.
    pub(crate) fn with_buffers<F>(
        events: AgentEventBuffer,
        scheduler: RunTaskScheduler,
        run: F,
    ) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let finish_events = events.clone();
        events.stream(scheduler, async move {
            run.await;
            finish_events.finish();
        })
    }
}

impl RunTaskScheduler {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SelectAll::new())),
        }
    }

    pub(crate) fn push(&self, stream: AgentStream) {
        self.inner
            .lock()
            .expect("run task scheduler mutex poisoned")
            .push(stream);
    }

    fn is_empty(&self) -> bool {
        self.inner
            .lock()
            .expect("run task scheduler mutex poisoned")
            .is_empty()
    }

    fn poll_next_child(&self, cx: &mut Context<'_>) -> Poll<Option<Event>> {
        let mut guard = self
            .inner
            .lock()
            .expect("run task scheduler mutex poisoned");
        guard.poll_next_unpin(cx)
    }
}

impl Stream for AgentStream {
    type Item = Event;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if let Some(event) = this.events.pop() {
            return Poll::Ready(Some(event));
        }

        if let Some(run) = this.run.as_mut() {
            if run.as_mut().poll(cx).is_ready() {
                this.run = None;
            }
        }

        if let Poll::Ready(Some(event)) = this.scheduler.poll_next_child(cx) {
            return Poll::Ready(Some(event));
        }

        if let Some(event) = this.events.pop() {
            return Poll::Ready(Some(event));
        }

        if this.events.is_done() && this.scheduler.is_empty() {
            return Poll::Ready(None);
        }

        this.events.register_waker(cx.waker().clone());

        if let Some(event) = this.events.pop() {
            return Poll::Ready(Some(event));
        }

        if this.events.is_done() && this.scheduler.is_empty() {
            return Poll::Ready(None);
        }

        Poll::Pending
    }
}

impl AgentEventBuffer {
    fn pop(&self) -> Option<Event> {
        let mut state = self.inner.lock().expect("event stream mutex poisoned");
        state.queue.pop_front()
    }

    fn is_done(&self) -> bool {
        let state = self.inner.lock().expect("event stream mutex poisoned");
        state.done
    }

    fn register_waker(&self, waker: Waker) {
        let mut state = self.inner.lock().expect("event stream mutex poisoned");
        state.waker = Some(waker);
    }
}
