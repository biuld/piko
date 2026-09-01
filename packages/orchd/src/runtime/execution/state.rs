use std::collections::VecDeque;

use piko_orchd_api::{ExecutionStatus, SteerExecutionRequest};
use piko_protocol::Usage;

use crate::domain::transcript::TranscriptManager;

pub(crate) struct ExecutionState {
    pub status: ExecutionStatus,
    pub transcript: TranscriptManager,
    pub model_step_index: u32,
    pub steering: VecDeque<SteerExecutionRequest>,
    /// Set when a steered user message was committed; the next model step
    /// must answer it in text without tools (F-35 / ADR-021).
    pub respond_after_steer: bool,
    pub usage: Usage,
    pub head_message_id: Option<String>,
    pub error: Option<String>,
}
