# ApprovalPanel

`ApprovalPanel` is the runtime-owned **tool approval dialog** — the widget
that replaces the status and editor slots when an agent requests permission
to execute a tool under the configured approval policy. It presents one
pending request at a time, shows the number of queued requests behind it,
and provides keyboard-only accept/decline controls.

```
  ┌──────────────────────────────────────────────┐
  │  Timeline                                    │
  ╞══════════════════════════════════════════════╪ ← border.accent (top + bottom)
  │ ┌─ Tool Approval ──────────────────────────┐ │
  │ │ Permission required        2 more queued │ │
  │ │ bash                                     │ │
  │ │ bun run test                             │ │
  │ └──────────────────────────────────────────┘ │
  │         Enter accept    Esc decline          │ ← hints bar (text.dim)
  ╞══════════════════════════════════════════════╡
  │  Bottom bar                                  │
  └──────────────────────────────────────────────┘
```

## Data boundary

`ApprovalPanel` receives a single prop — the current `TuiApprovalState`
snapshot from `TuiState.approval`. It is a pure presentation component;
it does not subscribe to Host or Orchestrator events, does not mutate
TUI state, and delegates all side effects to the parent `ActionService`.

```ts
interface ApprovalPanelProps {
  approval: TuiApprovalState;
}
```

### State model

```ts
interface TuiApprovalRequest {
  callId: string;
  toolName: string;
  toolArgs: unknown;
}

interface TuiApprovalState {
  /** Approval currently presented to the user. */
  pending?: TuiApprovalRequest;
  /** FIFO approvals waiting behind the presented request. */
  queue: TuiApprovalRequest[];
}
```

The component reads from the parent via `SlotRenderer`:

```tsx
case "approval":
  return <ApprovalPanel approval={s().approval} />;
```

## Layout

`ApprovalPanel` is wrapped in a `PartialShell` container that provides:

- Fixed height: **8** rows (content area + hints bar + border).
- Accent-colored top and bottom borders (`border.accent`).
- A floating title overlay: `"Tool Approval"` (positioned at `top=-1, left=2`).
- Hints bar: `"Enter accept"` and `"Esc decline"` in dim text.

### Content rows

The content area is a `flexDirection="column"` box with `paddingLeft={2}`, `paddingRight={2}`, `paddingTop={1}`, and `gap={1}`:

| Row | Content | Color token |
|-----|---------|-------------|
| 1 | `"Permission required"` label | `text.warning` |
| 1 (right) | Queue count (e.g. `"2 more queued"`) | `text.dim` |
| 2 | Tool name (e.g. `"bash"`) | `text.primary` |
| 3 | Summarized tool arguments, truncated to 1 line | `text.muted` |

Row 1 is a horizontal flex box with `justifyContent="space-between"`.
The queue count is only shown when `queue.length > 0`.

Row 3 uses `overflow="hidden"` and `height={1}` to constrain the tool
argument display to a single line.

### Argument summarization

`summarizeArgs(toolName, args)` produces a compact, single-line preview:

| Tool(s) | Preferred key | Fallback |
|---------|---------------|----------|
| `bash` | `args.command` | `JSON.stringify(args)` |
| `edit`, `write`, `read` | `args.path` | `JSON.stringify(args)` |
| other | — | `JSON.stringify(args)` |

Whitespace is collapsed (`/\s+/g` → `" "`) and the string is trimmed to
**220 characters** (`…` appended if truncated). If `args` is not an object
or is unserializable, an empty string or `"[unserializable arguments]"`
is shown.

## Render plan integration

The approval slot is a **runtime-owned capture panel**. When
`state.approval.pending` is truthy, `computeRenderPlan()` injects the
`"approval"` slot after the timeline slot and skips the status and editor
slots entirely:

```
pending?  →  [timeline, approval, bottom-bar]
no pending →  [timeline, status, editor, bottom-bar]
```

This takes priority over all user-opened surfaces — even a capture panel
with `inputPolicy: "capture"` is displaced so parallel tool approvals
cannot be hidden behind another dialog.

### Full panel coexistence

When a `placement: "full"` surface replaces the timeline, the approval
panel still renders between that surface and the bottom bar:

```
[full-surface | approval | bottom-bar]
```

## Approval lifecycle

### 1. Request arrives

When an agent's tool call hits the Host's approval gateway, a
`ToolApprovalRequest` flows through the **ApprovalBridge** → **ActionService**
→ dispatched as an `approval_needed` event.

```
Agent tool call
  → Orchestrator ApprovalGateway
  → Host approval handler
  → ApprovalBridge.handler(request, signal)
  → bridge delivers to ActionService.setApprovalBridge listener
  → ActionService stores in pendingApprovals Map
  → dispatch({ type: "approval_needed", callId, toolName, toolArgs })
  → reducer: approval_needed
    → if no pending request: set as pending, stream → awaiting_approval
    → if pending exists: append to queue
  → render plan sees pending → renders approval slot
```

### 2. Queue semantics (FIFO)

The `approval_needed` reducer maintains a **staged FIFO**:

```ts
approval_needed: (state, event) => {
  // Skip if the request is already pending or queued
  if (
    approval.pending?.callId === request.callId ||
    approval.queue.some((item) => item.callId === request.callId)
  ) return state;

  if (!approval.pending) {
    // First request: set as pending
    return { ...state, approval: { pending: request, queue: approval.queue } };
  }
  // Subsequent requests: append to queue
  return { ...state, approval: { ...approval, queue: [...approval.queue, request] } };
}
```

### 3. User decides

Keyboard handling is in `tui-controller.ts`'s **global handler**. When
`state.approval.pending` exists, only two keys are processed:

| Key | Decision | Effect |
|-----|----------|--------|
| `Enter` / `Return` | `"accept"` | Tool proceeds |
| `Escape` | `"decline"` | Tool blocked |

All other keys return `true` (event consumed, no action).

Both paths call `actionSvc.resolveApproval(callId, decision)`:

```ts
resolveApproval(callId: string, decision: ToolApprovalDecision): void {
  const entry = this.pendingApprovals.get(callId);
  if (!entry) return;
  this.pendingApprovals.delete(callId);
  this.dispatch({ type: "approval_resolved", callId, decision });
  entry.resolve(decision);
}
```

### 4. Resolution

`approval_resolved` reducer processes the decision:

```ts
approval_resolved: (state, event) => {
  // If the resolved callId doesn't match pending (queued item), remove from queue
  if (approval.pending?.callId !== event.callId) {
    const queue = approval.queue.filter((item) => item.callId !== event.callId);
    return queue.length === approval.queue.length
      ? state
      : { ...state, approval: { ...approval, queue } };
  }
  // Dequeue next (if any) and continue
  const [pending, ...queue] = approval.queue;
  return {
    ...state,
    approval: { pending, queue },
    stream: {
      ...state.stream,
      status: pending ? "awaiting_approval" : "running",
      currentToolCallId: pending?.callId,
    },
  };
}
```

- If the resolved `callId` matches `pending`: the next queued request (if any)
  is promoted to `pending`. If the queue is empty, the stream returns to `"running"`.
- If the resolved `callId` does **not** match `pending`: it is removed from the
  queue (e.g., an aborted request that never became pending).

### 5. Settled / aborted

When the stream settles (`stream_settled` reducer): `approval` is reset
to `{ queue: [] }`, clearing all approval state.

When an abort signal fires: the `AbortSignal` listener from
`ActionService.approvalHandler` fires, removes the entry from
`pendingApprovals`, dispatches `approval_resolved` with `decision: "decline"`,
and the promise resolves as `"decline"`.

## ApprovalBridge

The **ApprovalBridge** (`src/approval-bridge.ts`) is a lossless bridge
between the Host approval gateway and the mounted TUI. It solves the
temporal ordering problem: the Host is created with the approval handler
wired in, but the TUI (and its ActionService) may not be mounted yet.

### Architecture

```
createApprovalBridge()
  → { handler, onPending }
```

`handler` is passed to `createHost(…, { approvalHandler: bridge.handler })`
as the ToolApprovalHandler. `onPending` registers the TUI-side listener after
mount (via `ActionService.setApprovalBridge()`).

### Buffer semantics

- If `onPending` has not been registered yet, incoming requests are **buffered**
  and delivered once a listener is attached.
- Each `PendingApproval` carries `{ resolve, request, signal }`.
- Abort signals are wired to `settle("decline")` — settling is idempotent
  (the `settled` flag prevents double-resolve).

### Debug tracing

The bridge emits debug spans in `piko-orchestrator-protocol` format:

| Stage | Emitted when |
|-------|-------------|
| `approval.bridge.requested` | A new request enters the handler |
| `approval.bridge.buffered` | Request buffered (TUI not yet mounted) |
| `approval.bridge.delivery_error` | TUI-side callback throws |
| `approval.bridge.resolved` | Promise settled (accept/decline/aborted) |

`ActionService` adds its own spans:

| Stage | Emitted when |
|-------|-------------|
| `approval.tui.received` | Request enters ActionService via bridge |
| `approval.tui.resolved` | User accepts or declines |

## Stream status interaction

Approval state affects `TuiStreamState.status`:

| Before | During approval | After resolve (queue empty) | After resolve (more pending) |
|--------|----------------|----------------------------|------------------------------|
| `"running"` | `"awaiting_approval"` | `"running"` | `"awaiting_approval"` |

While `status === "awaiting_approval"`, the **input editor is disabled**
(`disabled` prop on the input surface). This prevents the user from
submitting new prompts before the approval is decided.

## Theme tokens

The component uses these theme color tokens:

| Token | Usage |
|-------|-------|
| `border.accent` | Top and bottom borders (`PartialShell`) |
| `text.accent` | Title overlay text |
| `text.warning` | `"Permission required"` label |
| `text.dim` | Queue count (`"N more queued"`), hints bar |
| `text.primary` | Tool name |
| `text.muted` | Tool argument summary |

## What ApprovalPanel does NOT render

| Concern | Where it belongs |
|---------|-----------------|
| Accept/decline history | Timeline items (via `TimelineItemView`, `kind === "approval"`) |
| Multi-step approval workflows | Orchestrator policy engine (out of scope for the panel) |
| Tool result display | Timeline items (tool call / tool result) |
| Policy configuration | Settings / `.piko/settings.json` |

## Timeline integration

Approval requests also appear as timeline items with `kind: "approval"`.
`TimelineItemView` renders them as:

```
  [approval] Approve `bash`?
```

The timeline entry is informational — the user interacts exclusively
through the `ApprovalPanel` key handlers. Once resolved, the timeline
item transitions to the corresponding tool call entry.

## Testing

Tests live in `packages/host-tui/test/approval-panel.test.ts` and
`packages/host-tui/test/approval-panel-theme.test.ts`:

| Test | Coverage |
|------|----------|
| `renders the active request and queued count` | Title, tool name, summarized args, queue count, hints bar |
| `uses tokens present in the resolved theme` | All five color tokens resolve in the default theme |

Tests for render plan integration live in `packages/host-tui/test/render-plan.test.ts`:

| Test | Coverage |
|------|----------|
| `replaces status and editor with the approval panel` | Approval slot injected when pending |
| `keeps approval visible ahead of an existing partial surface` | Approval takes priority over capture surfaces |
