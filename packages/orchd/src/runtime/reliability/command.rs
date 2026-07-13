use tokio::sync::oneshot;

struct ReplyGuard<T> {
    sender: Option<oneshot::Sender<T>>,
    fallback: Option<T>,
}

/// A narrow mailbox-command context manager. It owns only the reply obligation;
/// it never commits durable state or infers a successful business result.
pub(crate) struct ActorCommandScope<T> {
    reply: ReplyGuard<T>,
}

impl<T> ActorCommandScope<T> {
    pub fn new(sender: oneshot::Sender<T>, fallback: T) -> Self {
        Self {
            reply: ReplyGuard {
                sender: Some(sender),
                fallback: Some(fallback),
            },
        }
    }

    pub fn complete(mut self, value: T) {
        self.reply.fallback.take();
        if let Some(sender) = self.reply.sender.take() {
            let _ = sender.send(value);
        }
    }

    /// Transfer the reply obligation into a longer-lived protocol such as an
    /// Agent run waiter or durable follow-up queue.
    pub fn transfer(mut self) -> oneshot::Sender<T> {
        self.reply.fallback.take();
        self.reply
            .sender
            .take()
            .expect("reply obligation may only be transferred once")
    }
}

impl<T> Drop for ReplyGuard<T> {
    fn drop(&mut self) {
        if let (Some(sender), Some(fallback)) = (self.sender.take(), self.fallback.take()) {
            let _ = sender.send(fallback);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dropped_scope_completes_with_fallback() {
        let (sender, receiver) = oneshot::channel();
        drop(ActorCommandScope::new(sender, "aborted"));
        assert_eq!(receiver.await.unwrap(), "aborted");
    }

    #[tokio::test]
    async fn completed_scope_sends_value_exactly_once() {
        let (sender, receiver) = oneshot::channel();
        ActorCommandScope::new(sender, "aborted").complete("complete");
        assert_eq!(receiver.await.unwrap(), "complete");
    }

    #[tokio::test]
    async fn transfer_moves_the_reply_obligation() {
        let (sender, receiver) = oneshot::channel();
        let transferred = ActorCommandScope::new(sender, "aborted").transfer();
        transferred.send("complete").unwrap();
        assert_eq!(receiver.await.unwrap(), "complete");
    }
}
