mod support;

use std::sync::Arc;

use async_trait::async_trait;
use piko_hostd::api::{Command, ServerMessage as Event};
use piko_hostd::domain::config::{CompactionSettings, HostSettings};
use piko_hostd::infra::storage::JsonlSessionRepository;
use piko_hostd::infra::storage::session_store::SessionStore;
use piko_hostd::ports::{AgentRunHandle, AgentRunInput, AgentRunRunner};
use piko_hostd::protocol::HostServer;
use piko_llmd::gateway::{
    InferenceError, InferenceEvent, InferenceExecution, InferenceGateway, InferenceRequest,
};
use piko_protocol::agent_runtime::SessionEvent;
use piko_protocol::messages::Message;
use piko_protocol::{ContentBlock, MessageContent, MessageRole};
use support::{MockSessionPublisher, execution_running, execution_succeeded, successful_turn_run};
use tokio_util::sync::CancellationToken;

fn text_execution(text: impl Into<String>) -> InferenceExecution {
    InferenceExecution {
        events: Box::pin(tokio_stream::iter(vec![
            InferenceEvent::text(text),
            InferenceEvent::completed("stop"),
        ])),
        handle: None,
    }
}

include!("compaction_reconcile_cases/harness.rs");
include!("compaction_reconcile_cases/summarize.rs");
include!("compaction_reconcile_cases/new_window.rs");
include!("compaction_reconcile_cases/concurrency.rs");
include!("compaction_reconcile_cases/world_state.rs");
