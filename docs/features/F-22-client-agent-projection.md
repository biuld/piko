# F-22: Client agent projection lifecycle

> Status: implemented (Slices 1–3b; Slice 4 product-gated)
> Priority: P0
> Source evidence: Agent Client Protocol v1/v2 (modeling reference, ADR-003);
> piko F-01 turn runtime, F-07 approvals, F-09 session persistence, F-10
> multi-agent, D-29 usage accounting; TUI BottomBar usage gaps

## Summary

Clients need one host-authoritative contract for what is running, what may be
submitted next, what each streamed item means under stable identity, and how
context fill and cost are reported. This feature specifies that **client-facing
agent projection lifecycle**: foreground work state, stream item model,
admission visibility, usage projection, and how those projections compose with
piko-native session trees and multi-agent instances — without requiring clients
to speak ACP, and without replacing host domain ownership.

## Problem

Runtime behavior is substantially specified (F-01), but the **client wire and
projection** still leave ambiguity:

1. **Foreground readiness** is inferred from turn events and UI heuristics
   rather than a single session- or agent-scoped state (`idle` / `running` /
   blocked on user action).
2. **Stream updates** often lack uniform upsert keys and append/replace rules,
   so chrome must special-case timeline, tools, and cancellations.
3. **Usage chrome is incomplete**: prompt-side fill may appear as `11.5k/—`
   when the model catalog is not loaded, and hosts do not always push a paired
   context **size** with **used**.
4. **Cross-feature surfaces** (approvals, multi-agent targets, session
   snapshot reconcile) are correct in isolation but not stated as one
   projection stack a second client could implement from docs alone.

ACP documents mature editor-agent patterns for several of these problems.
piko should absorb the useful semantics under host authority (ADR-003), not
trade the piko protocol for ACP.

## User journeys

1. User opens a session. Snapshot establishes transcript, agent instances,
   active approvals, cumulative usage, and per-agent **foreground state**. UI
   is never “guessing idle” while a turn is open.
2. User submits to a target AgentInstance. Host accepts the intent (or returns
   queued/duplicate/overload per F-01). Client sees user message item(s) with
   stable ids, then running state, then agent/tool/plan updates. Cancel yields
   cancelled terminal state and durable abort markers already required by F-01.
3. An approval or structured interaction is required. Foreground state becomes
   **requires action**; after resolve, work continues or ends with a documented
   stop reason.
4. User watches context and cost. Status chrome shows **used / size** when both
   are known, or partial placeholders only when truly unknown; cumulative cost
   is host ledger authoritative. After model catalog bootstrap, size matches
   the active model window (or an explicit host estimate).
5. Child agents update while the parent is idle or running. Their stream items
   are attributable to instance ids; they never invent a second authority for
   the parent transcript.
6. Client disconnects and reconnects. Snapshot + event replay restores the same
   projection identities (message/tool/plan ids, turn ids, agent instance ids).

## In scope

- Client-observable **foreground work states** per AgentInstance (and how
  session-level chrome summarizes them).
- **Stop / completion reasons** aligned with terminal turns and cancellations
  (mapping to F-01 terminal outcomes).
- **Stream item model**: kinds (user message, agent message, agent thought,
  tool call, plan, usage, system/context markers), stable ids, upsert vs
  chunk-append vs replace rules.
- **Usage projection**: context fill `used` + optional `size`, cumulative
  session cost; relationship to D-29 turn/session ledgers and model catalog
  windows.
- **Submit acceptance vs work completion** as two distinct signals (admit /
  disposition vs idle/terminal).
- Rules for multi-agent attribution, snapshot reconcile, and client bootstrap
  requirements (for example model list for size resolution).
- Fusion policy with ACP (what shapes are kept/adapted/rejected).

## Out of scope

- Replacing JSON-lines piko protocol with ACP JSON-RPC (ADR-003).
- Orchd internal actor execution design (F-01 / D-01 already own core runtime).
- Compaction algorithms (F-05); only how compaction markers appear as stream /
  transcript items when projected.
- Full permission product redesign (F-07, F-17); only how pending user action
  appears in **foreground state** and stream-level links to approval ids.
- TUI cell layout; only required data for chrome correctness.
- ACP adapter implementation (future optional surface).

## Behavior and states

### Authority

- **hostd** is the sole authority for durable transcript items, turn terminal
  outcomes, cumulative usage, approvals, and agent instance metadata.
- Clients may hold **presentation** state (selection, follow, draft) only.
- Live events never contradict a later authoritative snapshot of the same ids;
  on conflict, snapshot wins and clients rebuild projection.

### Foreground work (per AgentInstance)

| State | Meaning | Typical triggers |
|---|---|---|
| `idle` | Ready for a new interactive prompt (background updates may still arrive) | Terminal turn completed/failed/cancelled; session open with no active work |
| `running` | Foreground model/tool work in progress | Submit accepted as start/steer; resume of active turn |
| `requires_action` | Blocked on user (approval, interaction, elicitation) | F-07 approval presented; user interaction outstanding |
| `queued` (optional surface) | User intent accepted but not yet executing | F-01 follow-up disposition `queued` |

Notes:

- Parent and child instances each have their own foreground state.
- Session chrome may show the **focused** instance, or “busy if any instance
  is running/requires_action”, but must not invent a third lifecycle.
- Background multi-agent or host maintenance may emit stream updates while the
  focused instance is `idle` (ACP v2-style beyond-turn notifications,
  adapted).

### Submit vs complete

1. **Accept** — host returns submission disposition (`accepted` start/steer,
   `queued`, `duplicate`, `overload`, or error). This is **not** end of turn.
2. **Complete foreground** — durable terminal for that work stream plus client
   projection `idle` with a **stop reason** derived from F-01:
   - completed / failed / cancelled (and cancel reason classes where already
     defined).

### Stream items

Every projected content-carrying update has:

- a **kind**,
- a **stable id** (scoped under session and, where applicable, agent instance),
- an **operation** semantics set: create-or-patch (upsert), append chunk,
  replace content, clear content.

Minimum kinds:

| Kind | Id scope | Notes |
|---|---|---|
| `user_message` | message id | Includes steered submits once committed |
| `agent_message` | message id | Model-visible assistant text |
| `agent_thought` | message id | Optional; hideable by client settings |
| `tool_call` | tool call id | Status + partial content/chunks |
| `plan` | plan id | **Deferred** — enum reserved; no host emitter until plan UX ships |
| `usage` (stream kind) | — | **Not used on StreamItem**; live usage is `ServerMessage::Usage` |
| `system` / context markers | stable derived ids | **Deferred** — markers stay on transcript/other events until folded |

Chunk operations **append** to the current content for that id. Full content
on an upsert **replaces** prior content for that id (including earlier chunks).
Other omitted fields on an upsert leave prior values unchanged.

`content_index` identifies a stable content segment within its stream kind; it
is never a byte, character, or chunk offset. Repeated chunks for the same
`(kind, content_index)` append to one segment, preserving whitespace. The
projection retains segments in first-seen order across message and thought
kinds so streaming presentation converges without token-sized blocks.

`replace_content` replaces one addressed content segment without changing the
item identity or its other fields. `clear_content` clears one addressed
segment, or all textual segments when `content_index` is omitted. Both
operations are idempotent and are supported for agent messages, thoughts, and
streamed tool arguments. A patch for a committed authoritative message is
ignored; durable transcript state always wins over a live correction.

### Timeline convergence

- `piko-client-core` owns the canonical, presentation-independent Timeline
  projection. Product clients may add viewport and rendering state, but do not
  independently reinterpret host events.
- Snapshot active-branch order is authoritative. Live committed messages use
  their transcript sequence without moving durable session facts, summaries,
  or custom messages out of branch order.
- A stream sequence gap is inconsistent state and causes an authoritative
  snapshot refresh.
- Running tool items retain their source turn identity. Failed and cancelled
  terminal turns finalize unresolved tools for that turn so clients never show
  permanently running work.
- Non-message durable entries that are visible in Timeline are emitted live as
  committed session entries. Session-scoped facts are merged into every agent
  view of that active branch at the same logical position.

### Usage projection

Host pushes (or includes in snapshot) a client-ready usage summary:

| Field | Meaning |
|---|---|
| `used` | Current estimated context fill (tokens), host-authoritative |
| `size` | Context budget / window for the active model when known |
| `cost` | Optional cumulative or session cost (session cumulative remains required for chrome) |

Rules:

- When `used` is known and `size` is not, clients **may** show partial fill
  (`used/—`) **only after** bootstrap attempts have failed or catalog has no
  window; cold UI must not systematically lack size because the client never
  requested the model catalog.
- Bootstrap for chrome that needs size includes loading the host model catalog
  (or an equivalent size field on the active-model event).
- Per-turn and cumulative ledgers remain consistent with D-29; this feature
  standardizes **how clients see them**, not a second ledger.

### Multi-agent and session tree

- Stream items and foreground state are keyed by `agent_instance_id` when the
  update is agent-scoped.
- Session tree navigation (F-09) remains piko-native: projection ids on replay
  must reappear after open/resume/fork as defined by persistence contracts.
- Target selection for submit (`target_agent_instance_id`) remains required for
  multi-agent sessions; this feature does not introduce ACP flat-session
  single-agent assumptions.

### Cancellation and requires_action

- Cancel intent is acknowledged separately from terminal projection (F-01).
- While approvals wait, state is `requires_action`; deny/expire maps through
  F-07 outcomes without inventing parallel permission types here.
- After cancel, non-finished tool projections become cancelled (or aborted
  tool results appear in transcript as already committed).

### Loading, empty, error, restore

- **Loading**: session open/hydrate shows progressive readiness; foreground
  state defaults to idle only when host confirms no active turns.
- **Empty session**: idle, empty stream, zero usage.
- **Error**: failed disposition or failed terminal surfaces host error text;
  chrome does not mark idle before terminal projection.
- **Restore**: snapshot applies id-stable rebuild; subsequent live events
  only apply when they advance or patch known sequences per design.

## Acceptance criteria

- [x] Documented foreground states and transitions match what TUI/client-core
      can implement without reading hostd sources (`AgentForeground::project`).
- [x] Submit acceptance is observable separately from foreground idle
      (CommandResult / turn start vs terminal + idle projection).
- [x] Stream kinds support upsert + chunk rules for live kinds
      (agent/tool/thought); durable ids survive session re-open. Plan/System
      kinds reserved and **deferred** (see kind table).
- [x] Usage projection can render `used/size` when the active model window is
      in the host catalog; bootstrap loads catalog or size on active model;
      live chrome via `ServerMessage::Usage`.
- [x] Multi-agent updates remain instance-attributed; parent idle is allowed
      while a child runs.
- [x] F-01 admission dispositions and terminals remain the authority for
      runtime outcomes; this PRD only projects them.
- [x] ADR-003 fusion table is filled for ACP shapes used here.
- [x] Design D-34 maps protocol DTOs; Slices 1–3 landed without ACP transport.
- [x] Canonical Timeline projection is shared by client-core and TUI; snapshot
      rebuild and live replay converge for mixed messages, tools, and facts.
- [x] `replace_content` / `clear_content`, stream-gap recovery, and terminal
      tool finalization are implemented and verified.
- [x] Visible non-message durable entries are projected live as well as from
      snapshots.
- [ ] Slice 4 optional ACP adapter (product-gated; out of Slices 1–3 scope).

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Replace piko wire with ACP? | No | ADR-003; host domain deeper than ACP |
| Foreground state unit | Per AgentInstance | Matches multi-agent product |
| Prompt accept vs turn end | Distinct | Aligns F-01 and ACP v2 lifecycle insight |
| Context size source | Host catalog and/or explicit size field on usage | Fixes `xx/—` systematic miss |
| Client executes tools via fs API? | No | host/sandbox authority |
| Third-party ACP clients | Future adapter, partial view | Not this PRD’s ship criteria |

## Fusion decisions (ACP)

ACP is a modeling reference (ADR-003), not a port target.

| ACP behavior | Decision | piko landing / rationale |
|---|---|---|
| `state_update`: running / idle / requires_action | **kept (adapted)** | Per-AgentInstance via `AgentForeground::project` (sole client path) |
| `session/prompt` returns on accept; completion via updates | **kept (adapted)** | Submit disposition vs terminal/idle |
| Message / thought upsert + chunks + required ids | **kept (adapted)** | `ServerMessage::StreamItem` sole live stream |
| Unified tool call update + content chunks | **kept (adapted)** | StreamItem tool upsert/chunks |
| `usage_update` used + size + cost | **kept (adapted)** | `ServerMessage::Usage` + snapshot |
| Permission title + extensible subject | **kept (adapted)** | Link to F-07; no ACP-only permission taxonomies |
| Plan item updates | **deferred** | `StreamItemKind::Plan` reserved; no emitter until plan UX |
| Diff file states / git_patch | **deferred** | Improve when file-change UX is productized |
| Client `fs/*` / `terminal/*` execution (v1) | **rejected** | piko host/sandbox owns execution |
| Agent-owned display-only terminal chunks (v2) | **kept (adapted)** | Map to tool output projection later |
| Flat single-agent session assumption | **rejected** | AgentInstance is first-class |
| Session config options as sole mode API | **deferred** | piko has host config namespaces |
| JSON-RPC methods / ACP transport | **rejected** | Stay on piko command/event protocol |
| Beyond-turn updates while idle | **kept (adapted)** | Child agents, background tasks |
| Session list/resume shapes | **partial** | Already F-09; align naming where useful only |
| System/context stream kinds | **deferred** | `StreamItemKind::System` reserved; markers stay elsewhere |

## Implementation notes (Slices 1–3)

- **Foreground**: sole mapping is `AgentForeground::from_activity` +
  `AgentForeground::project` in `piko-protocol`. client-core and TUI both call
  `project` (no parallel activity→foreground tables). Host does not emit a
  dedicated foreground event; clients derive from turns, approvals,
  interactions, and `AgentActivity`.
- **Usage**: live chrome authority is `ServerMessage::Usage`; snapshot may
  seed `last_context_tokens` from turn/entry usage when no live event yet.
- **Stream**: sole live wire is `ServerMessage::StreamItem`. Public timeline
  apply entry is `apply_stream_item`; orchd `RealtimeDelta` remains internal
  mapping input only.
- **Activity names**: runtime keeps `AgentActivity::WaitingForApproval`;
  client chrome uses `RequiresAction` via the sole mapping table above.
- **Timeline**: `piko-client-core::AgentTimeline` is the canonical reducer;
  TUI maps normalized items into render components. `SessionEntryCommitted`
  carries live non-message durable facts, tool items retain turn attribution,
  and all four StreamItem operations have defined reducer behavior.

## Open questions

1. Should session-level chrome aggregate multi-agent busy state, or only show
   the focused instance (client-configurable)?
2. Exact formula for `used` when multiple model steps and cache reads apply —
   mirror latest terminal prompt-side sum vs live estimate service (F-04/F-05)?
3. When product prioritizes plan UX, emit `StreamItemKind::Plan` (and optionally
   fold system markers into `System`) with a dedicated PRD slice.
4. Optional host-emitted `ForegroundChanged` event if pure client projection
   proves insufficient for multi-client sync (additive, wire-compatible).

## Reference evidence

- [ADR-003](../decisions/ADR-003-protocol-modeling-acp-reference.md)
- [F-01 Turn & agent runtime](F-01-turn-runtime.md)
- [F-07 Tool approvals](F-07-tool-approvals.md) (interaction baseline)
- [F-09 Session persistence](F-09-session-persistence.md)
- [F-10 Multi-agent](F-10-multi-agent.md)
- [D-29 Per-turn usage accounting](../design/D-29-per-turn-usage-accounting.md)
- ACP: https://agentclientprotocol.com/protocol/v2/overview (draft),
  prompt lifecycle, tool calls, extensibility
- TUI BottomBar context usage behavior (`packages/tui` bottom-bar feature)
