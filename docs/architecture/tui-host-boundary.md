# TUI / Host Boundary

The stable product boundary is between the TUI client and the Host runtime.
The TUI never talks directly to `orchd`; `hostd` is the authority for all
user-visible runtime state.

## TUI-Owned State

The TUI may own local presentation state:

- input buffer text
- cursor position
- selected panel or tab
- expanded and collapsed sections
- scroll position
- transient notifications
- command palette state
- focus state
- render caches

The TUI must not own authoritative business state:

- session history
- active turn status
- assistant streaming completion
- pending tool calls
- pending approvals
- compaction status
- final selected model
- auth credentials
- resolved skills, prompts, or context files

## Host-Owned State

`hostd` owns:

- session lifecycle and metadata
- message timeline
- turn lifecycle
- assistant streaming text
- tool call lifecycle
- approval lifecycle
- cancellation lifecycle
- compaction lifecycle
- auth, settings, and model resolution
- skills, prompts, and context files
- event cursors
- reconnect and resume state
- orchestrator task binding

## TUI -> Host Commands

Every command has a stable `command_id`. `hostd` must answer each command with
`command.accepted` or `command.rejected`.

```ts
type HostCommand =
  | { type: "session.create"; commandId: string; cwd: string }
  | { type: "session.open"; commandId: string; sessionId: string }
  | { type: "session.list"; commandId: string }
  | { type: "turn.submit"; commandId: string; sessionId: string; text: string }
  | { type: "turn.cancel"; commandId: string; sessionId: string; turnId: string }
  | { type: "approval.respond"; commandId: string; sessionId: string; approvalId: string; decision: "approve" | "reject"; note?: string }
  | { type: "state.snapshot"; commandId: string; sessionId: string }
  | { type: "events.resume"; commandId: string; sessionId: string; afterSeq: number };
```

## Host -> TUI Events

All user-visible runtime updates flow through `HostEvent`.

```ts
type HostEvent =
  | { type: "command.accepted"; commandId: string }
  | { type: "command.rejected"; commandId: string; reason: string }
  | { type: "session.created"; seq: number; sessionId: string; cwd: string }
  | { type: "session.opened"; seq: number; sessionId: string; snapshot: HostSessionSnapshot }
  | { type: "session.listed"; seq: number; sessions: SessionSummary[] }
  | { type: "turn.started"; seq: number; sessionId: string; turnId: string }
  | { type: "assistant.delta"; seq: number; sessionId: string; turnId: string; text: string }
  | { type: "assistant.message.completed"; seq: number; sessionId: string; turnId: string; messageId: string }
  | { type: "tool.started"; seq: number; sessionId: string; turnId: string; toolCallId: string; name: string }
  | { type: "tool.completed"; seq: number; sessionId: string; turnId: string; toolCallId: string; result: unknown }
  | { type: "approval.requested"; seq: number; sessionId: string; approvalId: string; request: unknown }
  | { type: "approval.resolved"; seq: number; sessionId: string; approvalId: string; decision: "approve" | "reject" }
  | { type: "turn.completed"; seq: number; sessionId: string; turnId: string }
  | { type: "turn.failed"; seq: number; sessionId: string; turnId: string; error: string }
  | { type: "turn.cancelled"; seq: number; sessionId: string; turnId: string }
  | { type: "state.snapshot"; seq: number; sessionId: string; snapshot: HostSessionSnapshot };
```

## Recovery Semantics

Session events have monotonically increasing `seq` values.

On reconnect, the TUI sends:

```text
events.resume(sessionId, lastSeenSeq)
```

`hostd` returns missing events when they are still in the replay window. If the
gap cannot be replayed, `hostd` returns `state.snapshot`. Streaming state is
part of the snapshot and never depends on transport close or promise resolution.

## Runtime Dependency Rule

The allowed direction is:

```text
hostd -> orchd -> sandbox
```

`orchd` does not depend on `hostd`; `sandbox` does not depend on either product
runtime crate.

