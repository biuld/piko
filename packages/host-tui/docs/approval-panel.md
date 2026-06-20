# ApprovalPanel

`ApprovalPanel` is the runtime-owned **tool approval surface** — opened
automatically via the `PanelSurfaceRequest` system (like `/model`) when an
agent requests permission to execute a tool under the configured approval
policy. It replaces the editor slot while keeping the status slot visible,
presents one pending request at a time, and provides keyboard controls for
**scoped authorization**: accept once, or authorize the tool for the session,
workspace, or permanently.

```
  ┌──────────────────────────────────────────────┐
  │  Timeline                                    │
  ├──────────────────────────────────────────────┤
  │  StatusPanel  ┌─ AgentPanel (main)           │ ← status stays visible
  ╞══════════════════════════════════════════════╪ border.accent top
  │  Authorize                                   │ ← title (chrome)
  │                                              │
  │  Permission required        2 more queued    │ ← body: row 1
  │  bash                                        │ ← body: row 2
  │  bun run test                                │ ← body: row 3
  │                                              │
  │  [Enter] accept  [A] session  [W] workspace  │ ← hints bar (chrome)
  │  [P] permanent  [Esc] decline                │
  ╞══════════════════════════════════════════════╡ border.accent bottom
  │  Bottom bar                                  │
  └──────────────────────────────────────────────┘
```

## Architecture

The approval dialog is a **proper surface**, not a hardcoded render slot.
It uses the same `PanelSession` / `PanelBody` / `PanelRenderer` system as
`/model`, `/settings`, and other user-triggered panels.

```
ActionService.approvalHandler(request)
  → ApprovalStore.isApproved(toolName)?        ← auto-accept if already authorized
  → dispatch approval_needed
  → onOpenApprovalSurface() callback
    → createToolApprovalPanelSession()
    → SurfaceManager.openPanel({ placement: "partial", inputPolicy: "capture" })
    → TuiController.setSurfaceController(handleKey: Enter/A/W/P/Esc)
    → render plan: [timeline, status, approval surface, bottom-bar]
```

### Component roles

| Component | Responsibility |
|-----------|---------------|
| `TuiApprovalState` | State model: `pending` request + FIFO `queue` |
| `ApprovalStore` | Scoped authorization: check/grant/revoke at session/workspace/permanent |
| `ActionService.approvalHandler` | Checks stored approvals, then delegates to UI if needed |
| `ActionService.onOpenApprovalSurface` | Callback that opens the surface |
| `TuiController.setActionService` | Wires the callback to SurfaceManager |
| `createToolApprovalPanelSession()` | Factory: creates `PanelSession` with `body.type: "tool-approval"` |
| `PanelRenderer` | Renders `PartialShell` (border, title, hints) + `PanelBody` |
| `ToolApprovalBody` | Presents tool name, args summary, queue count. No hints — those are in chrome. |
| Surface controller | Keyboard: Enter/A/W/P → accept at scope, Esc → decline, all other keys blocked |
| `computeRenderPlan` | Detects `tool-approval` surface by body type, keeps status visible |

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
      title: "Authorize",
      hints: ["[Enter] accept  [A] session  [W] workspace  [P] permanent  [Esc] decline"],
      height: 10,
    },
    interaction: "passive",
    capabilities: [],
    body: { type: "tool-approval", payload: {} },
  }],
  state: {},
}
```

The `height: 10` field in `PanelChrome` is an optional override — when absent,
`PanelRenderer` defaults to 14 for partial panels.

## Layout

Rendered via `PartialShell` with custom height **10**:

| Row | Content | Source |
|-----|---------|--------|
| 1 | ═══ accent top border | `PartialShell` |
| 2–8 | Content area: padding + 3 text rows + 2 gaps | `ToolApprovalBody` |
| 9 | Hints bar | `PanelChrome.hints` |
| 10 | ═══ accent bottom border | `PartialShell` |

### Content rows (ToolApprovalBody)

| Row | Content | Color token |
|-----|---------|-------------|
| 1 | `"Permission required"` label | `text.warning` |
| 1 (right) | Queue count (e.g. `"2 more queued"`) | `text.dim` |
| 2 | Tool name (e.g. `"bash"`) | `text.primary` |
| 3 | Summarized tool arguments, 1 line, overflow hidden | `text.muted` |

### Argument summarization

`summarizeArgs(toolName, args)` produces a compact, single-line preview:

| Tool(s) | Preferred key | Fallback |
|---------|---------------|----------|
| `bash` | `args.command` | `JSON.stringify(args)` |
| `edit`, `write`, `read` | `args.path` | `JSON.stringify(args)` |
| other | — | `JSON.stringify(args)` |

Whitespace is collapsed (`/\s+/g` → `" "`) and the string is trimmed to
**220 characters** (`…` appended if truncated).

## Keyboard handling

The surface controller (registered via `TuiController.setSurfaceController`)
maps:

| Key | Decision | Scope | Effect |
|-----|----------|-------|--------|
| `Enter` / `Return` | `"accept"` | once | Accept this call only |
| `A` | `"accept_session"` | session | Authorize this tool for the session |
| `W` | `"accept_workspace"` | workspace | Authorize this tool for the project |
| `P` | `"accept_permanent"` | permanent | Authorize this tool globally |
| `Escape` | `"decline"` | — | Decline this call |
| Any other key | — | — | Blocked |

The surface has `inputPolicy: "capture"` so it receives exclusive keyboard
focus while open.

## Scoped approvals (ApprovalStore)

### Storage

`ApprovalStore` (`src/approval-store.ts`) manages scoped tool authorizations
across three tiers. It is created in `App.tsx` with the project's `cwd` and
injected into `ActionService` as `svc.approvalStore`.

| Scope | Storage | Path | Lifetime |
|-------|---------|------|----------|
| `session` | In-memory `Map<toolName, grantedAt>` | (none) | Cleared when TUI exits |
| `workspace` | JSON file | `.piko/approvals.json` | Per-project, persists across sessions |
| `permanent` | JSON file | `~/.piko/approvals.json` | Global, applies to all projects |

Each JSON file stores a simple map:

```json
{
  "tools": {
    "bash": { "toolName": "bash", "grantedAt": 1719000000000 },
    "edit":  { "toolName": "edit",  "grantedAt": 1719000000000 }
  }
}
```

### Auto-accept (check before UI)

Before opening the approval surface, `ActionService.approvalHandler` calls
`approvalStore.isApproved(toolName)`. If the tool is already authorized at
**any** scope:

```ts
isApproved(toolName: string): ApprovalScope | null {
  if (this.sessionApprovals.has(toolName)) return "session";
  if (this.readApprovalsFile(this.workspacePath).tools[toolName]) return "workspace";
  if (this.readApprovalsFile(this.permanentPath).tools[toolName]) return "permanent";
  return null;
}
```

The handler returns `"accept"` immediately — **no surface is opened**, no
approval UI is shown. The orchestrator proceeds with tool execution.

Priority: session > workspace > permanent.

### Grant (store after decision)

When the user presses `A` / `W` / `P`, the surface controller calls
`resolveApproval(callId, "accept_session|workspace|permanent")`. Inside
`resolveApproval`:

```ts
if (decision === "accept_session") {
  this.approvalStore?.grant(entry.request.toolName, "session");
} else if (decision === "accept_workspace") {
  this.approvalStore?.grant(entry.request.toolName, "workspace");
} else if (decision === "accept_permanent") {
  this.approvalStore?.grant(entry.request.toolName, "permanent");
}
```

`grant()` writes to the appropriate storage backend: `Map.set()` for session,
`writeFileSync()` for workspace/permanent JSON files. File writes are
best-effort — errors are silently caught so approval flow is never blocked.

### Revoke

`revoke(toolName, scope)` removes the authorization. Session approvals are
deleted from the in-memory `Map`. Workspace/permanent are removed from the
JSON file. `clearSession()` drops all session-level approvals at once.

### Type extension

`ToolApprovalDecision` (in `orchestrator-protocol`) has been extended:

```ts
export type ToolApprovalDecision =
  | "accept"             // one-time
  | "decline"
  | "accept_session"     // auto-accept for current session
  | "accept_workspace"   // auto-accept for current workspace
  | "accept_permanent";  // auto-accept globally

export function isApprovalAccepted(decision: ToolApprovalDecision): boolean {
  return decision !== "decline";
}
```

The orchestrator treats all non-`"decline"` decisions identically (proceed
with execution). The scope variants only affect `ApprovalStore` behavior on
the TUI side.

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

## Approval lifecycle

### 1. Request arrives

```
Agent tool call
  → Orchestrator ApprovalGateway
  → Host approval handler
  → ApprovalBridge.handler(request, signal)
  → ActionService.approvalHandler
    → ApprovalStore.isApproved(toolName)?
      → yes → return "accept" (auto-accept, no UI)
    → stores in pendingApprovals Map
    → dispatch({ type: "approval_needed" })
    → onOpenApprovalSurface()
      → createToolApprovalPanelSession()
      → SurfaceManager.openPanel()
      → setSurfaceController(Enter/A/W/P/Esc)
```

### 2. Queue semantics (FIFO)

The `approval_needed` reducer maintains a staged FIFO. If a request is
already pending, subsequent requests are appended to the queue. When the
pending request is resolved, the next queued request is promoted.

### 3. User decides

The surface controller calls `ActionService.resolveApproval()` with the
appropriate decision string. This fulfills the Promise that the orchestrator
is awaiting, allowing tool execution to proceed.

### 4. Resolution reducer

`approval_resolved` removes the resolved callId. If the queue has more
items, the next becomes `pending`. If the queue is empty, stream status
returns to `"running"`.

### 5. Settled / aborted

- `stream_settled` → resets `approval` to `{ queue: [] }`
- Abort signal → bridge settles with `"decline"`, dispatches `approval_resolved`

## ApprovalBridge

The **ApprovalBridge** (`src/approval-bridge.ts`) is a lossless bridge
between the Host approval gateway and the mounted TUI. It solves the
temporal ordering problem: the Host is created before the TUI is mounted.
Pending requests are buffered and delivered once the listener is attached.

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
