# D-34: Client agent projection lifecycle

> Status: draft
> Implements: [F-22](../features/F-22-client-agent-projection.md)
> Decisions: [ADR-003](../decisions/ADR-003-protocol-modeling-acp-reference.md)

## Goal

Define how hostd **projects** agent work to clients so TUI, GUI, and
client-core share one lifecycle: foreground state, stream item identity rules,
submit vs complete, and usage (`used`/`size`/cost). Land incremental wire
changes without adopting ACP transport.

This design does **not** re-specify orchd turn execution (F-01 / D-01). It
maps runtime facts to client DTO shapes and bootstrap duties.

## Constraints and non-goals

- hostd remains authoritative (AGENTS.md / ADR-003).
- Session storage schema stays v3 unless a later decision expands it; prefer
  wire-only fields derived from existing durable facts.
- ACP JSON-RPC, method names, and client-side tool execution are non-goals.
- Full stream-item redesign of the timeline is phased; first slices are
  **usage correctness** and **foreground state**, then progressive stream id
  discipline.
- File size / module ceilings unchanged; avoid a single mega event enum dump.

## Proposed design

### 1. Projection layers

```text
orchd runtime events / durable SessionTreeEntry
        │
        ▼
hostd application projections
        │  - Turn lifecycle (existing)
        │  - AgentInstance foreground state (new/explicit)
        │  - Stream identity patches (phased)
        │  - Usage summary (new/extended)
        ▼
piko-protocol ServerMessage / CommandResult / Snapshot
        │
        ▼
client-core / TUI / GUI projectors
```

### 2. AgentInstance foreground state

**Source of truth (in-process, hostd):** for each live session agent instance:

| Field | Derivation |
|---|---|
| `foreground` | `idle` \| `running` \| `requires_action` |
| `active_turn_id` | if running or requires_action |
| `stop_reason` | last terminal, cleared when next work starts |

Derivation rules (initial):

- `running` — instance has an active non-terminal interaction turn **and** no
  blocking approval/interaction for that turn.
- `requires_action` — open approval or user-interaction prompt attributed to
  that instance (or its turn).
- `idle` — otherwise.

Emit:

- On every transition: `AgentEvent` / `TurnEvent` side-channel **or** a
  dedicated `AgentInstanceEvent::ForegroundChanged` (prefer explicit event to
  avoid overload). Exact type name is implementation detail; protocol must
  carry `agent_instance_id`, `foreground`, optional `active_turn_id`, optional
  `stop_reason`.

Snapshot:

- Extend active agent instance payload (or session agents list) with current
  `foreground` + optional `active_turn_id`.

Session chrome:

- Clients choose focus-based display; host may later add a session summary
  but is not required for F-22 v1.

### 3. Submit acceptance vs completion

Keep existing commands (`ChatSubmit`, queue/steer commands, `TurnCancel`).

Clarify in protocol docs / client-core:

1. **Disposition** — command result or turn-started/queued events (F-01).
2. **Idle** — when `foreground` returns to `idle` with `stop_reason`.

Do not invent an ACP `session/prompt` RPC. Optionally add a structured
`SubmitDisposition` in command results if clients currently only infer from
timeline (implementation slice).

### 4. Stream identity (phased)

**Phase A (document + enforce where cheap):**

- Require clients and host to treat durable entry / message / tool ids as
  upsert keys when replaying snapshots.
- Forbid generating new random client-side ids for host-committed items.

**Phase B (wire):**

Introduce a small patch DTO family (illustrative):

```text
StreamItemPatch {
  agent_instance_id?: AgentInstanceId,
  item_id: String,
  kind: StreamItemKind,
  op: upsert | append_chunk | replace_content | clear_content,
  ... kind-specific fields
}
```

Migrate high-traffic paths onto `ServerMessage::StreamItem` (assistant
chunks, tool call content, thought). Specialized realtime/tool
ServerMessage variants are not retained as parallel wire forms.

**Phase C:** plan and system markers use the same envelope.

### 5. Usage projection

Extend client-visible usage summary (snapshot field + optional live event):

```text
ContextUsageProjection {
  used: u64,              // host estimate of context fill
  size: Option<u64>,      // model context window / budget when known
  cost: Option<Cost>,     // or rely on cumulative_usage for cost only
}
```

`used` initial formula (keep stable until F-04 live estimate ships):

- Prefer latest terminal turn (or last assistant message with usage) prompt
  side: `input + cache_read` (matches current TUI `context_tokens_from_usage`).
- Document limitations in F-22 open questions; improve when dedicated budget
  tooling lands.

`size`:

1. Prefer explicit value set by host from `resolved_model_context_window()` at
   turn boundaries and on model change.
2. Clients may fill missing `size` from `ModelListed` catalog **only as a
   fallback**; host should push size so cold UI does not depend on opening
   the model selector.

Bootstrap duties (TUI/GUI/client-core):

- On connection: `ModelList` (silent) **or** guarantee `size` on
  `ModelEvent::ConfigChanged` / usage projection. Prefer both: catalog for
  selectors, size on model/usage for chrome.

Backfill TUI: request catalog during `bootstrap()` without opening the Models
panel focus (today ModelList reply forces Models mode — fix that side effect
as part of the slice).

### 6. Approvals ↔ requires_action

When an approval/interaction opens for an instance:

- set `requires_action`;
- keep existing Approval events;
- clear to `running` or `idle` per F-07 resolution outcomes.

### 7. Multi-agent

All new projection fields that describe work are optionally scoped by
`agent_instance_id`. Parent state stays `idle` while a child runs unless the
user is viewing the child.

### 8. Compatibility

- Additive fields with `#[serde(default)]` / optional.
- Old clients ignore unknown events/fields.
- New clients must not require Phase B stream envelope until shipped.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Foreground state DTOs; optional usage projection fields; optional stream patch types (phased); docs comments |
| `piko-hostd` | Compute/emit foreground transitions; set `size` on usage projection; keep ledgers (D-29) |
| `piko-orchd` | Minimal: ensure existing commits expose stable ids; no transport change |
| `piko-client-core` | Project foreground + usage; silent ModelList bootstrap |
| `piko-tui` | Bootstrap catalog without modal; BottomBar uses host `size`; render state from projection |
| `piko-gui` | StatusBar usage uses same projection (beyond cumulative-only when fields exist) |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Loss of connection: client marks unknown connection state; on rehydrate,
  snapshot overwrites projection.
- Cancelled turns: `foreground → idle` with stop reason `cancelled`; tool
  projections follow F-01 aborted results.
- Overload/duplicate: disposition only; no foreground transition.

## Verification

- Protocol type round-trip tests for new DTOs.
- hostd tests: foreground transitions on start/terminal/approval.
- hostd tests: `size` present when model window resolves.
- TUI: `bootstrap` requests models without entering Models mode; BottomBar
  formats `used/size` when catalog/window known.
- client-core: projection of multi-agent idle parent / running child.
- Manual: deepseek (or any catalog model) shows `Nk/Mk` not only `Nk/—`.

## Alternatives considered

| Alternative | Why not (now) |
|---|---|
| Full ACP Agent transport on hostd | Rejected by ADR-003; loses domain fidelity or forces extension heap |
| Only fix TUI catalog bootstrap | Fixes one bug; does not standardize agent projection semantics |
| Immediate full stream envelope rewrite | High risk; phase after state + usage slices |

## Rollout

### Slice 0 — Docs (this change)

- ADR-003 accepted, F-22 + D-34 draft reviewed.

### Slice 1 — Usage size + bootstrap (landed)

1. hostd: include `contextWindow` on `ModelEvent::ConfigChanged` from registry.
2. TUI: silent `ModelList` on bootstrap; does not force Models focus.
3. BottomBar size from host window field (fallback catalog).
4. client-core stores `context_window` and exposes `active_context_window()`.

### Slice 1b — Live usage projection for client-core / GUI (landed)

1. client-core rolls terminal turn `usage` into `last_context_tokens` +
   `cumulative_usage` (parity with TUI BottomBar between reconciles).
2. Snapshot rebuilds `last_context_tokens` from turn/message usage.
3. GUI StatusBar renders `used/size` (and optional cost) via shared formatters.

### Slice 1c — Dedicated `Usage` / `usage_update` event (landed)

1. Protocol `ServerMessage::Usage(UsageEvent::Updated)` with `used`, optional
   `size`, optional authoritative `cumulative` ledger, optional turn scope.
2. hostd emits Usage immediately after terminal `TurnLifecycle` (complete /
   fail / cancel), including queued-cancel and session-reopen finalization.
3. Clients apply Usage as the **sole** chrome authority (`used` / `size` /
   replace cumulative). Turn lifecycle `usage` fields are not projected into
   client chrome.

### Slice 2 — Foreground state (landed)

1. Protocol `AgentForeground` enum (ACP-aligned names).
2. client-core pure projection + approval/interaction turn status sync.
3. TUI AgentPanel uses per-agent foreground (no session-global is_running).
4. GUI agent tree labels use the same projection.

### Slice 3 — Stream item envelope (landed — sole stream transport)

1. Protocol `StreamItemKind` / `StreamItemOp` / `StreamItemPatch` plus mapping
   helpers from `RealtimeDelta` and internal `ToolExecutionEvent` fixtures.
2. Sole wire transport for live stream: `ServerMessage::StreamItem` (no
   `RealtimeMessage` / `ToolExecution` ServerMessage variants).
3. hostd emits StreamItem for assistant deltas and tool upserts (including
   hydrate/replay).
4. client-core / TUI / GUI apply StreamItem only; tool `tool_call_id` is the
   upsert key for arg chunks.
5. `ToolExecutionEvent` remains a non-wire helper for building tool patches
   (tests + host mapping), not a client-facing message.

### Slice 4 — Optional ACP adapter sketch

- Separate PRD when product prioritizes third-party editors; map F-22
  projections to ACP v1/v2 updates (subset).
