use super::step::PendingToolExecution;

/// Next unit of work for the task runtime state machine.
pub(super) enum TaskAction {
    StopCancelled,
    CommitInput(InputAwaitReason),
    ApplyControls,
    RunStep,
    ExecuteTools(PendingToolExecution),
}

/// Why the runtime is waiting on mailbox input before continuing.
pub(super) enum InputAwaitReason {
    Initial,
    WhileClosed,
    NextTurn { summary: String },
}
