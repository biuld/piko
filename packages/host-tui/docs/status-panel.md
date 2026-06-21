# StatusPanel

`StatusPanel` is the composition root for the **status slot** — the region
between timeline and editor. It does not own data, does not subscribe to Host
or Orchestrator events, and does not mutate TUI state. It composes two
presentation units — `AgentPanel` and the notification line — from two
separate upstream sources: the `OrchState` snapshot (agent activity) and the
`StatusContract` selector (system-level state).

```
  ┌──────────────────────────────────────────────┐
  │  Timeline                                    │
  ───────────────────────────────────────────────  ← border.muted top border (StatusPanel-owned)
  │  StatusPanel  ┌─ AgentPanel (agent 1)        │
  │               ├─ AgentPanel (agent 2)        │  ← status slot
  │               ├─ ...                         │
  │               └─ notification line (if any)  │
  ├──────────────────────────────────────────────┤
  │  Editor                                      │
  └──────────────────────────────────────────────┘
```

## Data boundary

StatusPanel receives two independent inputs; it never reads `TuiState` directly.

| Input | Source | Type | When present |
|---|---|---|---|
| `snapshot` | `host.getOrchestratorSnapshot()` | `OrchState \| undefined` | Team mode; refreshed every 100ms |
| `status` | `selectStatus(state, now)` | `StatusContract` | Always |

The component emits only two intents upward:

```ts
onViewedAgentChange(agentId: string): void
onToggleExpand(): void
```

which the parent (`SlotRenderer`) converts into `{ type: "viewed_agent_changed", agentId }`
and `{ type: "agent_expansion_toggled" }` respectively.

## StatusContract

`StatusContract` (in `renderer/opentui/status/types.ts`) is the system-level
state blob derived from `TuiState` by `selectStatus()`.

```ts
interface StatusContract {
  state: "idle" | "working" | "compacting";
  label?: string;            // override for "working" label
  queue?: StatusQueueContract;
  notification?: StatusNotification;
}
```

### Derivation rules (`selectStatus` in `state/selectors.ts`)

| Condition | `state` | `notification` | `queue` |
|---|---|---|---|
| stream running | `"working"` | — | — |
| idle + unread unexpired notification | `"idle"` | latest unread | queue if non-empty |
| idle + queue only | `"idle"` | — | queue |
| idle + nothing | `"idle"` | — | — |

- **`state`**: `"working"` when `stream.status === "running"`, otherwise `"idle"`.
  `"compacting"` is reserved but not yet wired.
- **`notification`**: First unread, unexpired notification from `state.notifications`.
  Expiry is checked via `isNotificationExpired()` against the caller-supplied `now`
  timestamp (driven by a `statusClock` signal in `App.tsx`).
- **`queue`**: `state.stream.queue` if it has any steering, follow-up, or next-turn messages.

### TTL-driven notification expiry

`App.tsx` maintains a `statusClock` signal and a `notificationExpiryTimer`.
When the earliest unexpired TTL among unread notifications is reached, the
timer fires, bumps `statusClock`, and `selectStatus` re-evaluates — causing the
notification line to disappear.

## Agent projection (`projectAgents`)

`StatusPanel.projectAgents()` converts `OrchState` + `StatusContract` into
`AgentPanelViewModel[]` — the presentation-ready list consumed by `AgentPanel`.

```ts
function projectAgents(
  snapshot: OrchState | undefined,
  status: StatusContract,
  currentAgentId: string,
): AgentPanelViewModel[]
```

### With snapshot (team mode on)

1. Iterates `snapshot.agents`.
2. Resolves each agent's active task from `snapshot.tasks[agent.activeTaskId]`.
3. Parses the task plan into `AgentPlanStepViewModel[]` (validates step/status fields).
4. Attaches queue to the agent matching `currentAgentId` only (steering/follow-up/next-turn from `StatusContract.queue`).

### Without snapshot (team mode off)

Returns a single fallback agent:

```ts
{
  id: currentAgentId,       // "main"
  name: currentAgentId,     // "main"
  status: status.state === "working" || status.state === "compacting" ? "running" : "idle",
  queue: projectQueue(status),
}
```

The fallback agent uses `StatusContract.state` to infer `AgentPanelStatus`.
When the stream is running, the agent shows the spinner. When idle, it shows
the hollow dot.

### Queue projection

`projectQueue()` converts `StatusQueueContract` into `AgentQueueItemViewModel[]`:

| Source | `kind` | `preview` |
|---|---|---|
| `queue.steering[i]` | `"steering"` | `item.preview` |
| `queue.followUp[i]` | `"follow_up"` | `item.preview` |
| `queue.nextTurnCount > 0` | `"next_turn"` | `"N next-turn messages"` |

## AgentPanel integration

Each agent in the projected list is rendered as an `AgentPanel` component.
StatusPanel owns:

- **Mode per agent**: `"expanded"` for the agent with `expandedAgentId === agent.id`, `"collapsed"` otherwise.
- **Selection**: `selected` is true when `viewedAgentId === agent.id`.
- **Spinner frame**: Forwarded from `App.tsx`'s 80ms frame clock.
- **Width**: Full viewport width.

For AgentPanel's own layout, column model, and row building, see
[agent-panel.md](agent-panel.md).

### Expansion behavior

Clicking an agent panel triggers `onSelect`:

```ts
onSelect = ({ agentId }) => {
  if (viewedAgentId === agentId) {
    // Toggle expansion for the currently-viewed agent
    onToggleExpand();
  } else {
    // Switch viewed agent (collapses previously-expanded agent)
    onViewedAgentChange(agentId);
  }
}
```

- Clicking a **different** agent sets `viewedAgentId` and clears `expandedAgentId`.
- Clicking the **same** agent dispatches `agent_expansion_toggled` which
  toggles `expandedAgentId` between `viewedAgentId` and `undefined`.

### Keyboard expansion (`Ctrl+E`)

Keybinding `app.agent.toggleExpand` (default `Ctrl+E`) dispatches
`agent_expansion_toggled` — the same event as clicking the currently-viewed
agent. No new interaction mode is required; the command runs in global scope.

The reducer:

```ts
agent_expansion_toggled: (state) => ({
  ...state,
  expandedAgentId: state.expandedAgentId ? undefined : state.viewedAgentId,
}),

viewed_agent_changed: (state, event) => ({
  ...state,
  viewedAgentId: event.agentId,
  expandedAgentId: undefined,  // reset expansion on agent switch
}),
```

## Notification line

Below all agent panels, a single notification row is rendered when
`status.notification` is present.

### Gutter alignment

The notification line must share the same gutter as `AgentPanel` rows.
Both use the identical prefix:

```
paddingLeft(1) + gap(1) + marker(1) + gap(1)
```

This ensures the notification text starts at the same column as agent names,
not at the box edge.

### Agent ↔ notification separator

The marker column distinguishes the notification row from agent rows.
Instead of an agent icon (`●`/`○`/`!`/`■`), the marker renders `│`
(vertical bar). This acts as a visual prefix that says "content below the
agent area, not another agent."

The `│` is colored by severity (same as the label text), reinforcing the
severity association at the first character the user sees.

### Layout

```text
  ●  main   2/3   Design AgentPanel          ← agent row
  ○  worker                                  ← background agent
  │  Something went wrong                    ← notification row (│ + message, severity color)
  ── ── ───────────────────────────────────
  🅰  🅱  🅲     🅳

  🅰 paddingLeft  (1 char)
  🅱 gap          (1 char, before marker)
  🅲 gap          (1 char, after marker)
  🅳 content area (remaining width)
```

Key rules:
- The `│` marker and the message text share the same severity color.
- No severity label — the color alone distinguishes error / warning / success / info.
- The message is a single string rendered after the gutter, truncated via
  `truncateToWidth()` to the available content width.
- Color tokens:
  - `"error"` → `text.error`
  - `"warning"` → `text.warning`
  - `"success"` → `text.success`
  - `"info"` → `text.accent`

### Display conditions

The notification line shows only when **all** of these hold:

1. `StatusContract.state === "idle"` — never during streaming.
2. `StatusContract.notification` is defined — there is an unread, unexpired notification.
3. No capture panel is active (the status slot is present in the render plan).

When any condition fails, the notification line does not render. Expiry is
driven by the TTL clock in `App.tsx` — once expired, `selectStatus` returns
`notification: undefined` and the line disappears on the next render.

## What StatusPanel does NOT render

These concerns are explicitly outside StatusPanel's scope:

| Concern | Where it belongs | Status |
|---|---|---|
| Working/compacting spinner + label | (design gap) | Not rendered. When `state === "working"`, agent panels show the spinner via their own `AgentPanelStatus === "running"`. No dedicated "Working..." line exists. |
| Queue items (steering/follow-up) as standalone rows | (design gap) | Queue is projected onto the current agent only (`AgentPanelViewModel.queue`), rendered inside `AgentPanel`. The old `StatusLine` rendered queue as separate rows — this has been removed. |
| Session rule (`─── session ───`) | (removed) | Was in the old `StatusLine`, now deleted. |
| "Ready" placeholder | (removed) | Was in the old `StatusLine`. |
| "Interrupted" / "Compacting..." messages | NotificationCenter | These are produced as notifications via `ctrl.notifications.notify()`. |
| Tool call progress | Timeline items | Tool calls are timeline entries, not status entries. |
| Thinking text | Timeline items | Rendered inside assistant messages in the timeline. |

### Known gaps

1. **No dedicated "Working..." indicator**: When the stream is running and there
   are no agents (team mode off), the fallback agent shows a spinner, which
   implicitly indicates "working". But if team mode is on and no agent is
   running, there's no explicit "Working..." line. The old `StatusLine` had
   this.

2. **Queue not rendered independently**: Steering and follow-up messages are
   projected onto the current agent's `AgentPanelViewModel.queue` and rendered
   as expanded queue rows inside `AgentPanel`. There is no standalone queue
   display (the old `StatusLine` had this). Whether StatusPanel should render
   queue items directly is an open design decision.

3. **No session rule / divider**: ~~The old `StatusLine` rendered a decorative
   `─── session name ───` line.~~ Resolved: StatusPanel now renders a
   `border.muted` top border (`border={["top"]}`) that separates it from
   the timeline above.

## Render plan integration

The status slot is rendered by `SlotRenderer` in `App.tsx`:

```tsx
case "status":
  return (
    <StatusPanel
      status={ctx.statusContract()}
      snapshot={ctx.orchestratorSnapshot()}
      currentAgentId={s().currentAgentId}
      viewedAgentId={s().viewedAgentId}
      width={ctx.layout().viewport.width}
      spinnerFrame={ctx.spinnerFrame()}
      onViewedAgentChange={(agentId) =>
        ctx.store.dispatch({ type: "viewed_agent_changed", agentId })
      }
    />
  );
```

The status slot appears in the render plan (from `computeRenderPlan()`) between
timeline and editor, **unless** a capture panel is active (`inputPolicy !== "passive"`),
in which case both status and editor are skipped.

### Orchestrator snapshot refresh

In `App.tsx`, when `host.teamMode` is true:

```ts
snapshotTimer = setInterval(refreshSnapshot, 100);
```

Every 100ms, `OrchState` is compared by JSON serialization. Only when the
snapshot changes is the SolidJS signal updated, triggering a StatusPanel
re-render.

## Component tree

```
StatusPanel
├── AgentPanel (agent 1)      ← from projectAgents()
│   ├── row: agent header
│   ├── row: plan step 1
│   ├── row: plan step 2
│   └── row: queue item
├── AgentPanel (agent 2)
│   └── row: agent header
└── notification line         ← from status.notification
```

Each `AgentPanel` is a `flexDirection="column"` box with height equal to its
row count. `StatusPanel` itself is `flexShrink={0}` — it takes only the space
its content needs, leaving remaining height to the timeline above and editor
below.

## Notification → StatusPanel data flow

```
NotificationCenter.notify(input)
  → emit "notification_added"
  → TuiController.onEvent
  → store.dispatch({ type: "notification_added", notification })
  → reducer: handleNotificationAdded → state.notifications.unshift(n)
  → SolidJS signal update
  → selectStatus(state, statusClock()) reads state.notifications
  → StatusContract.notification = { severity, message }
  → StatusPanel re-renders notification line
```

Read / clear events follow the same path:
- `markRead(id)` → `notification_read` → notification gets `readAt` → `selectStatus` skips it
- `clear(id)` → `notification_cleared` → notification removed from array
- TTL expiry → `statusClock` bump → `selectStatus` called with new `now` → expired notifications skipped

## Multi-agent considerations

When team mode is on and multiple agents exist:

- All agents from `snapshot.agents` are rendered.
- The **viewed** agent (`viewedAgentId`) is rendered with `selected={true}` (accent color, solid dot).
- The **expanded** agent (`expandedAgentId`) gets `mode="expanded"` — all other agents get `mode="collapsed"`.
- Only the **current** agent (`currentAgentId`) gets queue items projected onto its `AgentPanelViewModel`.
- Background (non-current, non-viewed) agents render in collapsed mode with dim color.

### Fallback when team mode is off

```ts
function fallbackAgent(status: StatusContract, agentId: string): AgentPanelViewModel {
  return {
    id: agentId,
    name: agentId,
    status: status.state === "working" || status.state === "compacting" ? "running" : "idle",
    queue: projectQueue(status),
  };
}
```

A single agent "main" is rendered with status derived from `StatusContract.state`
(running → spinner, idle → hollow dot).

## Testing

StatusPanel-specific tests should cover:

- Agent list projection from `OrchState` (with and without tasks/plans)
- Fallback agent when snapshot is undefined
- Fallback agent status when stream is running
- Queue projection onto current agent
- Notification line visibility (present only when idle + notification defined)
- Expansion toggle (same agent click) vs view switch (different agent click)
- Notification TTL expiry causing line disappearance
- Capture panel hiding the entire status slot

AgentPanel-specific tests are in `packages/host-tui/test/agent-panel-model.test.ts`.
Notification-specific tests are in `packages/host-tui/test/` (notification center).
