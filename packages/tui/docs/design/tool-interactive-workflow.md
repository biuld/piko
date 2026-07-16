# Tool Interactive Workflow Design

## Selected Feature

This design implements the `Tool Interactive Workflow` feature contract in
`packages/tui/docs/features/tool-interactive-workflow.md`.

The user-visible feature is a focused workflow panel that appears when an agent
tool needs structured input from the user before the running turn can continue.

## Responsibilities

`orchd` owns:

- exposing user-interaction tools to the model
- pausing the tool call while waiting for host/user input
- converting a completed or cancelled user interaction into a tool result

`hostd` owns:

- pending interaction state for active turns
- generating interaction ids
- emitting interaction request and resolution events to the TUI
- receiving TUI responses and unblocking the waiting tool callback
- including pending interactions in snapshots/resume
- serializing all user-facing prompts from orchd, including tool approvals and
  user-input requests

`protocol` owns:

- serializable request, answer, status, event, command, and snapshot DTOs
- stable ids for questions, choices, answers, and pending interactions

`tui` owns:

- rendering the focused workflow panel
- local interaction state while the panel is active
- focus/input routing for workflow navigation and text entry
- sending structured answers or cancellation back to hostd
- serializing display and input so only one approval or user-input prompt is
  active at a time

`InteractiveWorkflow` remains a reusable low-level component. It should not know
about tool calls, session ids, hostd commands, or orchd callbacks.

## Product Boundary

Tool Interactive Workflow is not Tool Approval.

Approval answers the question "may this action run?" and supports scoped grants
such as session, workspace, and permanent acceptance. Interactive Workflow
answers "what should the agent do next?" and returns selected values and text
input to a user-interaction tool.

The initial tool surface should support:

- a plain `ask_user` path for one text answer
- a structured `request_user_input` path for one or more choice-based questions

`request_approval` may continue to use the existing approval system unless a
future feature explicitly asks for non-persistent yes/no questions inside the
workflow UI.

## Shared Component Boundary

`InteractiveWorkflow` is the shared low-level component for focused,
choice-based prompts. It owns only local prompt state, rendering, and primitive
navigation/editing behavior. It does not own feature semantics.

Feature panels wrap the component and adapt their own domain objects into the
component model:

```text
ApprovalPanel
        |
        v
InteractiveWorkflow
        ^
        |
ToolInteractiveWorkflowPanel
```

Approval and Tool Interactive Workflow should therefore share the same component
but remain separate features above it.

ApprovalPanel maps a pending approval into a single-question workflow with
approval choices such as accept, accept for session, accept for workspace,
accept permanently, and decline. Submitting still sends
`Command::ApprovalRespond`, and hostd/orchd keep using the approval gateway and
approval persistence rules.

ToolInteractiveWorkflowPanel maps a user-interaction request into one or more
workflow questions. Submitting sends `Command::UserInteractionRespond`, and
hostd/orchd treat the answer as ordinary tool input, not as permission to run a
sensitive action.

This keeps the UI behavior consistent while preserving the security boundary:
approval remains a tool-execution gate, and interactive workflow remains a
structured input collection surface.

## Protocol Shape

Add protocol ids:

```rust
pub type InteractionId = String;
pub type InteractionQuestionId = String;
pub type InteractionChoiceId = String;
```

Add shared DTOs:

```rust
pub struct InteractionQuestion {
    pub id: InteractionQuestionId,
    pub header: String,
    pub prompt: String,
    pub choices: Vec<InteractionChoice>,
    pub required: bool,
}

pub struct InteractionChoice {
    pub id: InteractionChoiceId,
    pub label: String,
    pub value: serde_json::Value,
    pub input: Option<InteractionInput>,
}

pub struct InteractionInput {
    pub prompt: String,
    pub placeholder: Option<String>,
}

pub struct InteractionAnswer {
    pub question_id: InteractionQuestionId,
    pub choice_id: InteractionChoiceId,
    pub value: serde_json::Value,
    pub input: Option<String>,
}
```

Add events:

```rust
Event::UserInteractionRequested {
    task_id,
    agent_id,
    interaction_id,
    tool_call_id,
    title,
    questions,
    require_confirm,
    auto_resolution_ms,
}

Event::UserInteractionResolved {
    task_id,
    agent_id,
    interaction_id,
    status,
}
```

Add command:

```rust
Command::UserInteractionRespond {
    command_id,
    session_id,
    interaction_id,
    response,
}
```

Response shape:

```rust
pub enum UserInteractionResponse {
    Submit { answers: Vec<InteractionAnswer> },
    Cancel { reason: Option<String> },
}
```

Snapshots should include pending interactions next to pending approvals:

```rust
pub struct UserInteractionSnapshot {
    pub interaction_id: InteractionId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub tool_call_id: ToolCallId,
    pub status: UserInteractionStatus,
    pub title: Option<String>,
    pub questions: Vec<InteractionQuestion>,
    pub require_confirm: bool,
    pub auto_resolution_ms: Option<u64>,
}
```

## Orchestrator Flow

`UserInteractionProvider` currently exposes stubbed callbacks for `ask_user`
and `request_approval`. It should be extended to support a structured callback:

```rust
request_user_input(request) -> Future<Output = UserInteractionResponse>
```

Execution flow:

```text
model emits user-interaction tool call
        |
        v
UserInteractionProvider::execute
        |
        v
host callback creates pending interaction
        |
        v
tool future waits on oneshot
        |
        v
TUI responds through hostd command
        |
        v
callback resolves and tool result is returned to model
```

The callback must be cancellable with the active turn cancellation token. If the
turn is cancelled while waiting for user input, the callback resolves as cancel
and hostd emits `UserInteractionResolved`.

## Hostd Flow

`OrchAgentRunRunner` should hold pending interaction senders similarly to pending
approval senders:

```rust
pending_interactions:
    Arc<Mutex<HashMap<InteractionId, oneshot::Sender<UserInteractionResponse>>>>
```

`OrchAgentRunRunner` also owns a shared prompt gate used by both approval requests
and user-interaction requests:

```rust
prompt_gate: Arc<tokio::sync::Mutex<()>>
```

When orchd calls the host callback, hostd:

1. Acquires the prompt gate.
2. Allocates an `interaction_id`.
3. Stores the oneshot sender.
4. Emits `UserInteractionRequested`.
5. Waits for `Command::UserInteractionRespond`.
6. Removes the pending sender.
7. Emits `UserInteractionResolved`.
8. Releases the prompt gate.
9. Returns the response to the tool provider.

Tool approvals use the same prompt gate:

1. Acquires the prompt gate.
2. Emits `ApprovalRequested`.
3. Waits for `Command::ApprovalRespond`.
4. Emits `ApprovalResolved`.
5. Releases the prompt gate.
6. Returns the approval decision to orchd.

This means orchd may execute tools concurrently, but hostd exposes only one
user-facing prompt to the TUI at a time.

Unlike approvals, interaction responses are not stored in approval files and do
not support session/workspace/permanent grants.

State snapshots should include any pending interactions so a TUI reconnect or
resume can restore the visible workflow instead of leaving the tool blocked
without UI.

## TUI State And Placement

Add a workflow panel state owned by `AppState`, for example:

```rust
pub interactions: ToolInteractionPanel
```

The panel stores a queue of pending interaction view models. The active item is
the oldest unresolved request.

Add `AppMode::ToolInteraction`. It uses the same partial-overlay placement as
Approval mode:

- it is a partial overlay that replaces the Editor slot
- Timeline and AgentPanel remain visible above it
- Editor draft state is preserved while the panel is active
- focus is pushed when the first interaction arrives
- focus is popped when the queue becomes empty

Approval should use the same partial-overlay placement. Approval and Tool
Interactive Workflow both represent "the TUI is waiting for a user decision",
so both replace the Editor instead of inserting an additional panel above it.
If approvals and interactions are both pending, Approval should win because it
controls security-sensitive tool execution. After approvals resolve, the
workflow panel can become active.

TUI keeps its own queues as a defensive layer. Even if multiple prompt events
arrive during reconnect or from older hostd versions, it displays and routes
input to only one active prompt at a time.

## Component Refactor

`InteractiveWorkflow` should stay under `ui/components` and remain free of
hostd/tool concepts.

Required component changes:

- add stable question and choice ids
- expose a method that returns structured answers
- expose a method that reports whether the current state can submit
- move Tree-specific `target_entry_id` out of the component into Tree state
- keep rendering data-driven and theme-driven

The Tree summary prompt can keep using the component through a small wrapper
that stores the target tree entry id outside the component.

## Input Routing

Add workflow-focused actions:

- next choice
- previous choice
- select choice by number
- next question
- previous question
- enter inline input
- save inline input
- submit
- cancel

Input priority remains:

1. Global Escape/Enter handling.
2. Active focus owner handles workflow keys.
3. Editor fallback is not reached while workflow focus is active.

Escape behavior:

- if inline input is active, leave inline input
- otherwise send cancel response for the active interaction

Enter behavior:

- if inline input is active, save inline input
- else if the selected choice has input and no input is active, enter inline
  input
- else if the workflow can submit, send submit response
- else advance to the next unanswered question

## Event Handling

On `UserInteractionRequested`, TUI should:

1. Convert protocol DTOs into an `InteractiveWorkflow` view model.
2. Push the request into the panel queue.
3. Notify the user.
4. Push `AppMode::InteractiveWorkflow` if approval mode is not active.

On `UserInteractionResolved`, TUI should:

1. Remove the matching request from the queue.
2. Clear local state for that request.
3. Pop workflow focus if the queue is empty and workflow focus is active.

If the user submits or cancels locally, the panel should stay visible in a
pending-submission state until hostd emits `UserInteractionResolved`. This
matches the existing approval flow and avoids local/host disagreement.

## Validation

Focused validation should include:

- protocol serde tests for request, answer, command, event, and snapshot DTOs
- hostd tests for pending interaction registration, response, cancellation,
  and snapshot restore
- orchd tests for `ask_user` and `request_user_input` tool execution with wired
  callbacks
- TUI tests for event handling, focus push/pop, submit/cancel dispatch, and
  component answer extraction

Run:

```text
cargo fmt --all
cargo test -p piko-protocol
cargo test -p hostd
cargo test -p orchd
cargo test -p tui
```

Run workspace clippy before merging because this feature crosses crate
boundaries.
