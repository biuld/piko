use std::collections::VecDeque;

use piko_protocol::execution::{ExecutionStatus, SteerExecutionRequest};
use piko_protocol::Usage;

use crate::domain::transcript::TranscriptManager;

pub struct ExecutionState {
    pub status: ExecutionStatus,
    pub transcript: TranscriptManager,
    pub model_step_index: u32,
    pub steering: VecDeque<SteerExecutionRequest>,
    pub usage: Usage,
    pub head_message_id: Option<String>,
    pub error: Option<String>,
}
