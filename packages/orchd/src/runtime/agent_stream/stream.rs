use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use futures_core::Stream;
use futures_util::stream::{SelectAll, StreamExt};

use crate::domain::events::event::Event;

tokio::task_local! {
    pub(crate) static RUN_AGENT_EVENTS: AgentEventBuffer;
    pub(crate) static RUN_TASK_SCHEDULER: RunTaskScheduler;
}

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
    pub(crate) fn new<F>(run: F) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let events = AgentEventBuffer::new();
        let scheduler = RunTaskScheduler::new();
        let events_for_run = events.clone();
        let events_for_finish = events.clone();
        let scheduler_for_run = scheduler.clone();
        events.stream(scheduler.clone(), async move {
            RUN_TASK_SCHEDULER
                .scope(scheduler_for_run, async move {
                    RUN_AGENT_EVENTS.scope(events_for_run, run).await;
                })
                .await;
            events_for_finish.finish();
        })
    }
}

pub(crate) fn current_agent_events() -> Option<AgentEventBuffer> {
    RUN_AGENT_EVENTS.try_with(Clone::clone).ok()
}

pub(crate) fn current_task_scheduler() -> Option<RunTaskScheduler> {
    RUN_TASK_SCHEDULER.try_with(Clone::clone).ok()
}

pub(crate) async fn scope_agent_events<F, T>(events: AgentEventBuffer, run: F) -> T
where
    F: Future<Output = T>,
{
    RUN_AGENT_EVENTS.scope(events, run).await
}

pub(crate) fn emit_agent_event(event: Event) {
    if let Some(events) = current_agent_events() {
        events.push(event);
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
