# AgentPanel

`AgentPanel` is the embeddable presentation unit for one agent. It does not
subscribe to Host or Orchestrator events and does not decide where it is
mounted. A parent surface supplies a presentation-ready `AgentPanelViewModel`
and owns borders, expansion state, height budgets, and multi-agent ordering.

## Modes

- `collapsed`: one row containing agent status, active plan position, and the
  active step. Without a plan it falls back to the active task title.
- `expanded`: one task header row followed by an optional failure row and every
  plan step with its status and `x/y` position.

Running agents use an animated spinner in the agent header. Plan steps keep
static status markers so animation communicates agent activity only.

The default multi-agent composition will render the current agent in the
surface's selected mode and every other active agent in `collapsed` mode.

## Data boundary

The component consumes `AgentPanelViewModel`; it does not consume
`OrchState`, `HostEvent`, or TUI stream state. Host per-agent projection will
be responsible for producing this view model in the later integration phase.

`selected` only controls the active-agent highlight. Mouse activation emits an
`AgentPanelSelectEvent` through `onSelect`; the parent converts that intent into
`viewed_agent_changed`. AgentPanel never mutates `viewedAgentId` and never
controls timeline state. Keyboard selection remains owned by the parent surface
and uses the same semantic event path.
