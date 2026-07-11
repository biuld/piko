//! Multi-agent spawn and join behavior.

use std::sync::Arc;

use piko_protocol::agents::HostTaskContext;

use crate::faux_provider::FauxProvider;
use crate::runtime::{test_agent_spec, test_config, test_supervisor, wait_for_task_report};

#[tokio::test]
async fn test_task_control_spawn_and_join() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("sub-task result").await;

    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    let sub_spec = test_agent_spec("worker");
    core.register_agent(sub_spec).await;

    let task_id = core
        .spawn_detached(
            "worker",
            "do delegated work",
            None,
            None,
            HostTaskContext::new("s1"),
        )
        .await;
    assert!(!task_id.is_empty());

    let result = wait_for_task_report(&core, &task_id).await;
    assert_eq!(result.text, "sub-task result");
}
