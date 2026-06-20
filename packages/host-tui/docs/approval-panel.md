# ApprovalPanel

`ApprovalPanel` is the runtime-owned **tool approval surface** — opened
automatically via the `PanelSurfaceRequest` system (like `/model`) when an
agent requests permission to execute a tool under the configured approval
policy. It replaces the editor slot while keeping the status slot visible,
presents one pending request at a time, shows the number of queued requests,
and provides keyboard-only accept/decline controls.

```
  ┌──────────────────────────────────────────────┐
  │  Timeline                                    │
  ├──────────────────────────────────────────────┤
  │  StatusPanel  ┌─ AgentPanel (main)           │  ← status stays visible
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

## Architecture

The approval dialog is a **proper surface**, not a hardcoded render slot.
It uses the same `PanelSession` / `PanelBody` / `PanelRenderer` system as
`/model`, `/settings`, and other user-triggered panels.

```
ActionService.approvalHandler()
  → dispatch approval_needed
  → onOpenApprovalSurface() callback
    → createToolApprovalPanelSession()
    → SurfaceManager.openPanel({ placement: "partial", inputPolicy: "capture" })
    → TuiController.setSurfaceController(handleKey: Enter→accept, Esc→decline)
    → render plan: [timeline, status, approval surface, bottom-bar]
```

### Component roles

| Component | Responsibility |
|-----------|---------------|
| `TuiApprovalState` | State model: `pending` request + FIFO `queue` |
| `ActionService.approvalHandler` | Host gateway → Promise that awaits user decision |
| `ActionService.onOpenApprovalSurface` | Callback that opens the surface |
| `TuiController.setActionService` | Wires the callback to SurfaceManager |
| `createToolApprovalPanelSession()` | Factory: creates `PanelSession` with `body.type: "tool-approval"` |
| `PanelRenderer` | Renders `PartialShell` (border, title, hints) + `PanelBody` |
| `ToolApprovalBody` | Presents tool name, args summary, queue count |
| Surface controller | Keyboard: Enter → accept, Esc → decline, all others blocked |
| `computeRenderPlan` | Detects `tool-approval` surface, keeps status visible |

## Data boundary

`ToolApprovalBody` reads from the `TuiStore` (via `props.store.state().approval`)
— the same pattern used by other `PanelBody` implementations. It does not
subscribe to Host or Orchestrator events and does not mutate state.

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

## PanelSession factory

```ts
import { createToolApprovalPanelSession } from "../panels/panel-factories.js";

// Returns:
{
  id: "tool-approval-N",
  stack: [{
    id: "tool-approval.main",
    chrome: {
      title: "Tool Approval",
      hints: ["Enter accept", "Esc decline"],
      height: 9,                          // override default PartialShell height (14)
    },
    interaction: "passive",
    capabilities: [],
    body: { type: "tool-approval", payload: {} },
  }],
  state: {},
}
```

The `height: 9` field in `PanelChrome` is an optional override — when absent,
`PanelRenderer` defaults to 14 for partial panels.

## Layout

Rendered via `PartialShell` with custom height **9**:

| Row | Content | Source |
|-----|---------|--------|
| 1 | ═══ accent top border | `PartialShell` |
| 2–7 | Content area (padding + 3 text rows + 2 gaps) | `ToolApprovalBody` |
| 8 | `"Enter accept    Esc decline"` hints | `PanelChrome.hints` |
| 9 | ═══ accent bottom border | `PartialShell` |

### Content rows (ToolApprovalBody)

| Row | Content | Color token |
|-----|---------|-------------|
| 1 | `"Permission required"` label | `text.warning` |
| 1 (right) | Queue count (e.g. `"2 more queued"`) | `text.dim` |
| 2 | Tool name (e.g. `"bash"`) | `text.primary` |
| 3 | Summarized tool arguments, truncated to 1 line | `text.muted` |

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

## Keyboard handling

The surface controller (registered via `TuiController.setSurfaceController`)
maps:

| Key | Action |
|-----|--------|
| `Enter` / `Return` | `resolveApproval(callId, "accept")` → close surface |
| `Escape` | `resolveApproval(callId, "decline")` → close surface |
| Any other key | Blocked (`{ type: "handled" }`) |

The surface has `inputPolicy: "capture"` so it receives exclusive keyboard
focus while open.

## Render plan integration

The approval surface is detected by `computeRenderPlan` via its body type
(`"tool-approval"`). When present:

```
presence  →  [timeline, status, approval surface, bottom-bar]
absent    →  [timeline, status, editor, bottom-bar]
```

The status slot stays visible (unlike other capture panels that hide both
status and editor). The approval surface takes precedence over all other
user-opened surfaces — even a `inputPolicy: "capture"` panel is displaced.

### Full panel coexistence

When a `placement: "full"` surface replaces the timeline, the approval
surface still renders after status and before the bottom bar:

```
[full-surface | status | approval surface | bottom-bar]
```

## Approval lifecycle

### 1. Request arrives

```
Agent tool call
  → Orchestrator ApprovalGateway
  → Host approval handler
  → ApprovalBridge.handler(request, signal)
  → ActionService.approvalHandler
    → stores in pendingApprovals Map
    → dispatch({ type: "approval_needed" })
    → onOpenApprovalSurface()
      → SurfaceManager.openPanel()
      → registerOwner + setSurfaceController
```

### 2. Queue semantics (FIFO)

The `approval_needed` reducer maintains a **staged FIFO**:

```ts
approval_needed: (state, event) => {
  // Skip if already pending or queued (dedup by callId)
  if (
    approval.pending?.callId === request.callId ||
    approval.queue.some((item) => item.callId === request.callId)
  ) return state;

  if (!approval.pending) {
    // First request: set as pending
    return { ...state, approval: { pending: request, queue: [] } };
  }
  // Subsequent requests: append to queue
  return { ...state, approval: { ...approval, queue: [...approval.queue, request] } };
}
```

### 3. User decides

The surface controller resolves via `ActionService.resolveApproval()`:

```ts
resolveApproval(callId, decision):
  → delete from pendingApprovals Map
  → dispatch({ type: "approval_resolved", callId, decision })
  → entry.resolve(decision)  // fulfills the Promise from approvalHandler
```

### 4. Resolution reducer

```ts
approval_resolved: (state, event) => {
  // If resolved callId != pending: remove from queue
  if (approval.pending?.callId !== event.callId) {
    const queue = approval.queue.filter((item) => item.callId !== event.callId);
    return queue.length === approval.queue.length
      ? state
      : { ...state, approval: { ...approval, queue } };
  }
  // Dequeue next (if any)
  const [pending, ...queue] = approval.queue;
  return {
    ...state,
    approval: { pending, queue },
    stream: {
      ...state.stream,
      status: pending ? "awaiting_approval" : "running",
    },
  };
}
```

- If queue still has items after resolve → next request becomes `pending`
- If queue is empty → stream returns to `"running"`
- A new approval surface is opened automatically for the next pending request

### 5. Settled / aborted

- `stream_settled` → resets `approval` to `{ queue: [] }`
- Abort signal → bridge settles with `"decline"`, dispatches `approval_resolved`

## ApprovalBridge

The **ApprovalBridge** (`src/approval-bridge.ts`) is a lossless bridge
between the Host approval gateway and the mounted TUI. See the source
for full buffer semantics and debug tracing.

## Stream status interaction

| Before | During approval | After resolve (queue empty) | After resolve (more pending) |
|--------|----------------|----------------------------|------------------------------|
| `"running"` | `"awaiting_approval"` | `"running"` | `"awaiting_approval"` |

While `status === "awaiting_approval"`, the editor is disabled.

## Theme tokens

| Token | Usage |
|-------|-------|
| `border.accent` | Top and bottom borders (`PartialShell`) |
| `text.accent` | Title overlay text |
| `text.warning` | `"Permission required"` label |
| `text.dim` | Queue count, hints bar |
| `text.primary` | Tool name |
| `text.muted` | Tool argument summary |

## Timeline integration

Approval requests also appear as timeline items with `kind: "approval"`.
`TimelineItemView` renders them as:

```
  [approval] Approve `bash`?
```

## Testing

| Test file | Coverage |
|-----------|----------|
| `render-plan.test.ts` | Approval surface slot order, priority over capture panels |
| `approval-panel.test.ts` | Renders tool name, args, queue count |
| `approval-panel-theme.test.ts` | Theme token resolution |
