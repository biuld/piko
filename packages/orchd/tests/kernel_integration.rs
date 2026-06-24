// Phase 2 integration test: verify tokio_actors-based ActorSystem semantics.

use tokio_actors::{
    ActorResult, StopReason,
    actor::{Actor, ActorExt, context::ActorContext},
};

use tokio_actors::ActorSystem;

// ---- Test actor that echoes messages ----

#[derive(Default)]
struct EchoActor {
    received: Vec<serde_json::Value>,
}

impl Actor for EchoActor {
    type Message = serde_json::Value;
    type Response = serde_json::Value;

    async fn handle(
        &mut self,
        msg: serde_json::Value,
        _ctx: &mut ActorContext<Self>,
    ) -> ActorResult<serde_json::Value> {
        self.received.push(msg.clone());
        Ok(serde_json::json!({"echo": msg, "from": "echo"}))
    }
}

#[tokio::test]
async fn test_spawn_send_and_reply() {
    let _system = ActorSystem::default();

    // Spawn echo actor
    let handle = EchoActor::default()
        .spawn()
        .named("echo")
        .await
        .expect("spawn");

    // Test send (request-response)
    let reply = handle
        .send(serde_json::json!({"question": "ping"}))
        .await
        .expect("send");
    assert_eq!(reply["echo"]["question"], "ping");

    // Test notify (fire-and-forget) + send
    handle
        .notify(serde_json::json!({"hello": "world"}))
        .await
        .expect("notify");
    let reply2 = handle
        .send(serde_json::json!({"second": "msg"}))
        .await
        .expect("send");
    assert!(reply2["echo"]["second"].is_string());

    // Stop the actor
    handle.stop(StopReason::Graceful).await.expect("stop");

    // Send to stopped actor should fail
    let result = handle.send(serde_json::json!({"x": 1})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_send_to_unknown_actor() {
    let system = ActorSystem::default();
    // Getting a handle to an actor that doesn't exist returns None
    let handle = system.get::<EchoActor>("nonexistent");
    assert!(handle.is_none());
}

#[tokio::test]
async fn test_multiple_actors() {
    let _system = ActorSystem::default();

    let h1 = EchoActor::default()
        .spawn()
        .named("a")
        .await
        .expect("spawn a");
    let h2 = EchoActor::default()
        .spawn()
        .named("b")
        .await
        .expect("spawn b");

    let r1 = h1
        .send(serde_json::json!({"to": "a"}))
        .await
        .expect("send a");
    let r2 = h2
        .send(serde_json::json!({"to": "b"}))
        .await
        .expect("send b");

    assert_eq!(r1["from"], "echo");
    assert_eq!(r2["from"], "echo");

    // Stop both
    h1.stop(StopReason::Graceful).await.expect("stop a");
    h2.stop(StopReason::Graceful).await.expect("stop b");
}
