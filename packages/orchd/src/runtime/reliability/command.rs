use piko_comms::{ReplyContract, ReplySender};

struct ReplyGuard<C: ReplyContract, T> {
    sender: Option<ReplySender<C, T>>,
    fallback: Option<T>,
}

/// A narrow mailbox-command context manager. It owns only the reply obligation;
/// it never commits durable state or infers a successful business result.
pub(crate) struct ActorCommandScope<C: ReplyContract, T> {
    reply: ReplyGuard<C, T>,
}

impl<C: ReplyContract, T> ActorCommandScope<C, T> {
    pub fn new(sender: ReplySender<C, T>, fallback: T) -> Self {
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
}

impl<C: ReplyContract, T> Drop for ReplyGuard<C, T> {
    fn drop(&mut self) {
        if let (Some(sender), Some(fallback)) = (self.sender.take(), self.fallback.take()) {
            let _ = sender.send(fallback);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_comms::contracts::AgentCommandReply;

    #[tokio::test]
    async fn dropped_scope_completes_with_fallback() {
        let (sender, receiver) = piko_comms::reply::<AgentCommandReply, _>();
        drop(ActorCommandScope::new(sender, "aborted"));
        assert_eq!(receiver.await.unwrap(), "aborted");
    }

    #[tokio::test]
    async fn completed_scope_sends_value_exactly_once() {
        let (sender, receiver) = piko_comms::reply::<AgentCommandReply, _>();
        ActorCommandScope::new(sender, "aborted").complete("complete");
        assert_eq!(receiver.await.unwrap(), "complete");
    }
}
