# AgentPanel

`AgentPanel` is the embeddable, presentation-only unit for displaying agent
activity. It does not subscribe to Host or Orchestrator events, does not
control where it is mounted, and does not mutate TUI state. A parent surface
provides a presentation-ready `AgentPanelViewModel` and owns borders, expansion
state, height budgets, and multi-agent ordering.

## View Model

The component consumes `AgentPanelViewModel` from `packages/host-tui/src/agents/types.ts`:

```ts
type AgentPanelMode = "collapsed" | "expanded";

type AgentPanelStatus = "idle" | "running" | "failed" | "stopped";

type AgentPlanStepStatus = "pending" | "in_progress" | "completed";

interface AgentPlanStepViewModel {
  id?: string;
  step: string;
  status: AgentPlanStepStatus;
}

interface AgentTaskViewModel {
  id: string;
  title: string;
  plan: AgentPlanStepViewModel[];
  error?: string;
}

type AgentQueueKind = "steering" | "follow_up" | "next_turn";

interface AgentQueueItemViewModel {
  id?: string;
  kind: AgentQueueKind;
  preview: string;
}

interface AgentPanelViewModel {
  id: string;
  name: string;
  status: AgentPanelStatus;
  activeTask?: AgentTaskViewModel;
  queue?: AgentQueueItemViewModel[];
}
```

It does not consume `OrchState`, `HostEvent`, or TUI stream state. Host
per-agent projection is responsible for producing this view model.

## Modes

- **`collapsed`**: Single row containing agent status marker + name + active
  plan position/detail + queue count. If the agent has no task, only the icon
  and name are shown (no detail or progress). If the agent has a task but no
  plan steps, same — icon and name only. On failure with an error message,
  shows the error as the detail and stops the spinner.
- **`expanded`**: Agent header row (name + icon only) followed by plan steps
  (with `x/y` position and status markers), optional error row, and queued
  items. The header does **not** repeat the task title or queue count — those
  are only shown in collapsed mode.

Running agents show an animated spinner in the header. Plan steps use static
status markers: `●` (in_progress), `✓` (completed), `○` (pending).

## Queue Rendering

Queue items appear in both modes:

- **Collapsed**: Shows a queue count in the queue column (e.g. `"1 queued"`).
- **Expanded**: No queue count in the header. Queue items render as
  individual rows (icon `↳`, kind, preview).

The queue column is only rendered when the terminal width is ≥ 64 characters.

## Collapsed vs Expanded — full examples

The following examples show how a single agent renders in each mode,
then how multiple agents compose in the StatusPanel.

### Single agent: collapsed

One row. Icon, name, plan position, active step.

```text
  ◌     main     2/3       Design AgentPanel          1 queued
  ───   ──────   ─────     ──────────────────         ──────────
  gap   marker   gap       name  gap progress  gap    detail    gap  queue
```

### Single agent: expanded

Header row + plan step rows + queue items.

```text
  ●     main
    ✓   1/3                 Inspect architecture
    ●   2/3                 Design AgentPanel
    ○   3/3                 Verify behavior
    ↳   follow-up           Run focused tests
```

- Header: agent name only (no task title, no queue count in expanded mode).
- Step rows: indented (no name), markers reflect step status.
- Queue row: icon `↳`, kind in progress column, preview in detail.

### Multi-agent composition

The StatusPanel renders all agents from the orchestrator snapshot.
One agent is **viewed** (`selected`, see § Marker states) and may be
**expanded** (`Ctrl+E` or click). All others render collapsed.

```text
  ◌     main
    ✓   1/3                 Inspect architecture
    ●   2/3                 Design AgentPanel
    ○   3/3                 Verify behavior
    ↳   follow-up           Run focused tests
  ○     worker                                       3 queued
  ■     reviewer
  !     builder             Model request failed
```

- **main**: viewed + expanded (accent header, plan steps visible).
- **worker**: background, collapsed (dim `○`, queue count in queue column).
- **reviewer**: stopped (`■`), collapsed, no task.
- **builder**: failed (`!`), collapsed, error in detail.

### Expansion toggle (`Ctrl+E`)

Pressing `Ctrl+E` dispatches `agent_expansion_toggled`. The reducer flips
`expandedAgentId` between `viewedAgentId` and `undefined`. Switching viewed
agent (click or `/agent` command) always resets `expandedAgentId` to
`undefined`.

See [status-panel.md](status-panel.md) for the StatusPanel composition layer
that owns mode distribution and expansion state.

## Data Boundary

The component emits only one intent:

```ts
interface AgentPanelSelectEvent {
  type: "agent_selected";
  agentId: string;
}
```

`selected` only controls the active-agent highlight (accent text color on the
agent row). Mouse activation emits `AgentPanelSelectEvent` through `onSelect`;
the parent converts that intent into `viewed_agent_changed`. AgentPanel never
mutates `viewedAgentId` and never controls timeline state. Keyboard selection
remains owned by the parent surface and uses the same semantic event path.

## Row Model

Rows are produced by `buildAgentPanelRows()` in `agent-panel-model.ts`:

```ts
interface AgentPanelRow {
  key: string;
  kind: "agent" | "plan" | "queue" | "error";
  icon: string;
  spinner?: boolean;       // animate marker instead of static icon
  name?: string;            // agent name
  progress?: string;        // step position or status
  detail?: string;          // step description / task title / error
  queue?: string;           // queue count or item detail
  tone: AgentPanelRowTone;  // normal | muted | accent | success | error
}
```

Tones map to theme tokens: `accent` → `text.accent`, `success` → `text.success`,
`error` → `text.error`, `muted` → `text.dim`, `normal` → `text.primary`.

## Layout

### Container

Every AgentPanel row lives in a `flexDirection="row"` box of `width={props.width}`
with `paddingLeft={1}` and `overflow="hidden"`. Column widths are calculated
against `props.width - 1`.

### Column allocation algorithm

`getAgentPanelColumns(width)` in `agent-panel-layout.ts` distributes available
width across content columns separated by `gap`:

```ts
function getAgentPanelColumns(width: number): AgentPanelColumns {
  const available = Math.max(20, width);
  const marker   = 1;
  const gap      = 1;
  const progress = available < 34 ? 5 : 7;
  const queue    = available >= 64 ? 12 : 0;
  const name     = Math.min(16, Math.max(8, Math.floor(available * 0.22)));
  const gapCount = queue > 0 ? 5 : 4;
  const detail   = Math.max(1, available - marker - gap * gapCount - name - progress - queue);
  return { marker, gap, name, progress, detail, queue };
}
```

Width breakpoints and thresholds:

| Available width | marker | gap | name | progress | queue | detail |
|---|---:|---:|---:|---:|---:|---:|
| 20 (minimum) | 1 | 1 | 8 | 5 | 0 | 4 |
| 34 | 1 | 1 | 8 | 7 | 0 | 16 |
| 40 | 1 | 1 | 8 | 7 | 0 | 22 |
| 64 | 1 | 1 | 14 | 7 | 12 | 28 |
| 80 | 1 | 1 | 16 | 7 | 12 | 42 |
| 100 | 1 | 1 | 16 | 7 | 12 | 62 |
| 120 | 1 | 1 | 16 | 7 | 12 | 82 |

Rules:
- **name** is capped at 16 and floored at 8. Between these limits it scales
  proportionally (22% of available width).
- **gap** appears 5 times when queue is visible, 4 times otherwise.
- **detail** is the remainder after all fixed columns and gaps. Guaranteed at least 1.
- **queue** appears only when the terminal is 65+ chars wide (available ≥ 64).

### Row structure

Every gap between columns is an empty `<box width={1}>`, including before the
marker column (left margin):

```text
  gap   marker  gap   name      gap  progress  gap  detail              gap  queue
┌─────┬──────┬─────┬──────────┬────┬─────────┬────┬───────────────────┬────┬──────────┐
│     │  ●   │     │  main    │    │  2/3    │    │ Design AgentPanel │    │ 1 queued │
└─────┴──────┴─────┴──────────┴────┴─────────┴────┴───────────────────┴────┴──────────┘
```

| Column | Width | Content |
|---|---|---|
| `gap` | 1 | Empty `<box>`, left margin |
| `marker` | 1 | Icon or spinner — see marker states below |
| `gap` | 1 | Empty `<box>` |
| `name` | 8–16 | Agent name, truncated via `truncateToWidth()` |
| `gap` | 1 | Empty `<box>` |
| `progress` | 5 / 7 | Step position (`2/3`) or status label |
| `gap` | 1 | Empty `<box>` |
| `detail` | remainder | Description / title / error message |
| `gap` | 1 | Empty `<box>` (only when `queue > 0`) |
| `queue` | 12 | Queue count (only when width ≥ 65) |

#### Marker states

The marker column renders differently based on agent run state and selection:

| Condition | Rendered | Notes |
|---|---|---|
| `spinner === true` | `<Spinner>` animation | Braille frame cycle, ~80ms/frame, replaces icon |
| `selected && icon === "○"` | `●` (solid dot) | Viewed agent, accent color |
| otherwise | `row.icon` as-is | `○` background agent, `!` failed, `■` stopped |

- Only the currently-outputting agent (`status: "running"`) has `spinner: true`
- When the viewed agent is idle, it shows a solid dot `●` to distinguish it
  from background agents which show a hollow `○`
- Failed / stopped icons are not affected by `selected`

#### Queue column

Only rendered when `columns.queue > 0` (width ≥ 65), preceded by a `gap`.
Uses `text.dim` color.
- Collapsed: queue count (e.g. `"1 queued"`).
- Expanded: empty (queue items render as full rows instead).

### Visual layouts by mode

Marker column legend: `◌` = spinner (running), `●` = viewed agent (idle),
`○` = background agent (idle).

#### Collapsed — running agent with plan

```text
  ◌     main     2/3       Design AgentPanel
  ───   ──────   ─────     ──────────────────
  marker name    progress  detail
```

- 1 row. Spinner animating (braille frames).
- `progress` = `2/3` (second of three plan steps in progress).
- `detail` = active step description.

#### Collapsed — running agent without plan

```text
  ◌     main
  ───   ──────
  marker name
```

- 1 row. Spinner animating.
- `progress` and `detail` are empty — agent has a task but no plan steps yet.

#### Collapsed — viewed agent (selected, idle)

```text
  ●     main
  ───   ──────
  marker name
```

- Currently-viewed agent, solid dot `●`, accent color.
- Other columns empty when no task.

#### Collapsed — background agent (idle)

```text
  ○     main
  ───   ──────
  marker name
```

- Non-current agent, hollow dot `○`, dim color.

#### Collapsed — failed agent

```text
  !     main     Model request failed
  ───   ──────   ────────────────────
  marker name    detail
```

- Spinner stops, icon changes to `!`.
- `detail` column shows the error message.
- Entire row uses `error` tone (red).

### Expanded layouts

All expanded rows share the same column structure (each row `height={1}`):

```text
 [marker] [ name ] [progress] [       detail       ] [queue]
```

#### Expanded — running agent with plan

```text
  ●     main
    ✓   1/3                 Inspect architecture
    ●   2/3                 Design AgentPanel
    ○   3/3                 Verify behavior
```

- 4 rows (header + 3 steps).
- Header row: agent name only — no task title in expanded mode.
- Step rows: static markers. `progress` shows `x/y`.
- Completed steps use `success` tone (green), in-progress uses `accent`,
  pending uses `muted`.

#### Expanded — with error

```text
  !     main
    !   error               Model request failed
    ✓   1/3                 Inspect architecture
    ●   2/3                 Design AgentPanel
    ○   3/3                 Verify behavior
```

- Error row inserted between header and plan steps.
- Icon `!`, progress = `"error"`, detail = error text.

#### Expanded — with queue items

```text
  ●     main
    ✓   1/3                 Inspect architecture
    ●   2/3                 Design AgentPanel
    ○   3/3                 Verify behavior
    ↳   follow-up           Run focused tests
```

- Queue items render as rows with icon `↳`, kind in progress column,
  preview in detail column.
- Header row does **not** show queue count in expanded mode (unlike collapsed).

### Tone mapping

Row tones map to theme semantic color tokens:

| tone | token | Usage |
|---|---|---|
| `"normal"` | `text.primary` | Default |
| `"muted"` | `text.dim` | Idle agents, pending steps, queue items |
| `"accent"` | `text.accent` | Running agents, in-progress steps, selected agent |
| `"success"` | `text.success` | Completed plan steps |
| `"error"` | `text.error` | Failed agents, error rows |

When `selected === true`, the agent header row (`kind === "agent"`) overrides
its tone with `text.accent` regardless of agent status.

### Truncation

All text content goes through `truncateToWidth(text, columnWidth)` which:

1. Strips ANSI escape sequences for width calculation.
2. Counts CJK / wide characters as 2 columns.
3. Counts combining characters as 0 columns.
4. Cuts visible characters when they exceed the column width.
5. Preserves ANSI codes in the output string.

No ellipsis is appended; text is simply cut at the column boundary.

## Error Handling

When an agent is in `failed` status with a task error:

- **Collapsed**: Replaces the plan summary with the error message, sets tone to
  `error`, and stops the spinner.
- **Expanded**: Adds a dedicated error row (`kind: "error"`) below the agent
  header, with icon `!`, progress label `"error"`, and the error text as detail.

## Multi-Agent

The default multi-agent composition will render the current agent in the
surface's selected mode and every other active agent in `collapsed` mode. The
parent surface owns the mode per agent and the ordering.

## Testing

Agent panel model tests live in `packages/host-tui/test/agent-panel-model.test.ts`
and cover:

- Collapsed rendering with active plan
- Expanded rendering with all plan steps
- Completed and not-started plan summaries
- Empty detail/progress when no plan exists in collapsed mode
- Idle agent without an active task
- Error prioritization in collapsed mode
- Queue summary in collapsed mode, items in expanded mode
- Column width allocation (queue column only above 64 chars)
