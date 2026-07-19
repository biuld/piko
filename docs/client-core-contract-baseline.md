# Client Core Contract Baseline

> Status: accepted and implemented Phase 0/2 baseline
> Product contract: [GUI Workbench](gui-workbench-feature.md)
> Architecture: [Client Core Design](client-core-design.md),
> [GPUI Desktop Client Design](gui-design.md)
> Behavioral reference: unchanged `packages/tui` (especially
> `packages/tui/docs/features/session-view-lifecycle.md`)

## 1. Purpose

This document freezes the hostd conversation-path contract implemented by
Phase 2 `piko-client-core` pure reducer cases. It derives message
sequences and expected state transitions from the existing protocol DTOs and
TUI behavior. It does not introduce runtime code, change the TUI, or require
protocol edits for the M1/M2 path.

Phase 2 turned these cases into reusable fixture builders and
`(state, message) -> (state, effects)` tests without copying ratatui state or
rendering code.

## 2. Protocol Gate

**Decision:** existing commands and `ServerMessage` variants are sufficient for
Session open/create, transcript render, Agent select/submit/stop, approval, and
interaction. No protocol edit is planned up front.

If a later Phase 2/4/5 case cannot be expressed:

1. document the missing product fact here;
2. add or adjust the DTO in `piko-protocol`;
3. implement and test hostd production/recovery;
4. only then consume it in Client Core.

### 2.1 Wire envelope

| Direction | Root type | Tag field |
|---|---|---|
| Client → hostd | `Command` | `type` (snake_case) |
| hostd → client | `ServerMessage` | `kind` (snake_case) |

Payload structs mostly use camelCase fields. Host settings patches use
kebab-case keys.

Every `Command` carries `command_id`. Business completion arrives as
`CommandResponse { command_id, result }` and/or required follow-on events.
There is no separate transport ACK.

### 2.2 Authoritative projection messages

| Message | Client Core meaning |
|---|---|
| `SessionReconciled` | Atomic replace of the visible Session projection |
| `SessionCleared` | Authoritative return to no-session |
| `SessionCreated` / `SessionOpened` | Identity only; never enter `Live` |
| `StateSnapshot` result (`Empty`) | Refresh requested; projection arrives via reconcile |
| `AgentSubscribed` | Replace selected-Agent timeline from host replay; not a full Session reconcile |
| Incremental events | Apply only when `session_id` / `agent_instance_id` match live scope |

## 3. Resolved Assumptions

### 3.1 Agent selection and subscription

| Question | Resolution |
|---|---|
| Does selecting a non-current Agent require `AgentSubscribe`? | **Yes.** Reconcile restores hierarchy and a selected Agent plus committed branch transcript. Switching to another Agent for a live timeline requires `AgentSubscribe`. |
| When does TUI emit it? | On explicit Agent activation (AgentPanel Enter). Not automatically on reconcile or ChatSubmit. |
| What `after_seq` should Client Core use in v1? | **`None`**, matching TUI. Request full agent-view replay. Incremental cursor subscribe is deferred. |
| How is replay applied? | Prefer `AgentSubscribed.snapshot.events` when non-empty; otherwise use `replay`. Clear the selected Agent timeline before applying sequenced messages. |
| Does subscribe change host selection? | Yes. hostd sets/persists the selected Agent. It does not emit `SessionReconciled`. |
| Reconcile selected Agent fallback | Use `snapshot.selected_agent_instance_id` if present in `agents`; else root (`parent_agent_instance_id == None`); else first agent. |

Client Core intent `SelectAgent` therefore emits `AgentSubscribe` and waits for
`AgentSubscribed` before treating the selected timeline as host-authoritative
for that Agent. Optimistic local selection of the Agent row is a frontend
presentation choice; the timeline swap remains command-result driven.

### 3.2 Conversation Map / Session tree

| Fact | Owner / source |
|---|---|
| Session conversation DAG | `SessionSnapshot.entries` |
| Current leaf | `SessionSnapshot.current_leaf_id` |
| Active-path Timeline content | Leaf→root walk of `entries`, then filter/route by `agent_instance_id` |
| Agent hierarchy | `SessionReconciled.agents` / `AgentChanged` |
| Selected Agent timeline after subscribe | `AgentSubscribed` sequenced events (committed + prior live buffer) |
| Branch switch | `SessionNavigate { session_id, entry_id, summarize, custom_instructions? }` |
| Navigate result | `SessionNavigated` may supply `editor_text` / `summary_entry`; authoritative tree/leaf arrive via subsequent `SessionReconciled` |

**Active branch algorithm (TUI reference):** if `current_leaf_id` is absent,
treat all `entries` as visible; otherwise walk parent links from the leaf to
the root, reverse to chronological order, and stop on missing parent or cycle.

**Map navigation contract for GUI:**

- On-path entry activation scrolls Timeline only (GUI-local).
- Off-path entry activation is a GUI preview; it must not mutate authority.
- Explicit Switch Branch emits `SessionNavigate`. If leaving the current branch
  requires summarization/confirmation, the focused workflow runs first (GUI),
  then the intent is sent with the chosen `summarize` / instructions.
- Tree selection alone never changes `current_leaf_id`.

`entries` are session-wide. Conversation Map shows the selected Agent's
structural view of that shared tree (and/or agent-scoped labels), not a second
durable store. Full message bodies remain Timeline-owned.

### 3.3 Model and thinking

| Operation | Mechanism |
|---|---|
| List catalog | `ModelList` → `CommandResult::ModelListed` |
| Read active model/provider/thinking | `ModelEvent::ConfigChanged` (not a typed get-current command) |
| Write model | `ConfigUpdate` merge patch: `default-provider`, `default-model` |
| Write thinking | `ConfigUpdate` merge patch: `default-thinking-level` |
| Frontend presentation config | `ConfigGet` / `ConfigUpdate` under `[gui]` (and TUI under `[tui]`) |

**Important host behavior:** `ConfigUpdate` currently emits observer events
(including `ModelEvent::ConfigChanged`) and typically **does not** emit
`CommandResponse` for the caller's `command_id`. Client Core must correlate
model/thinking writes by observing `ConfigChanged` (and command failure if the
transport reports one), not by waiting for an Empty typed result.

`ConfigGet { namespace: "tui" }` returns the TUI blob only. It is not a source
for default model/thinking. Session tree entries `ModelChange` /
`ThinkingLevelChange` on the active branch may also refresh the displayed
active values during reconcile; treat that as TUI-compatible behavior for
comparison scenarios.

Composer selectors are enabled only after Phase 4's model contract gate, using
the rules above. They are not required to enter `Live`.

## 4. Workbench Fact Ownership

| Workbench fact | Protocol / core source | GUI-local |
|---|---|---|
| Session list rows | `SessionListed` | search filter, sheet open/close |
| Live vs pending open target | core session phase + open target | sidebar pending chrome |
| Session / project title | `SessionSnapshot` + cwd | native window title formatting |
| Agent tree | `agents` / `AgentChanged` | expansion, scroll, focus |
| Selected Agent | reconcile selection + `AgentSubscribe` | row highlight before result |
| Timeline committed items | reconcile branch + `TranscriptCommitted` | Markdown, folding, virtualization |
| Timeline realtime draft | `RealtimeMessage` | lightweight text animation |
| Tool cards | tree tool entries + `ToolExecution` | expand/collapse |
| Active Turn / Stop | `active_turns` + `TurnLifecycle` | button enablement chrome |
| Queue counts | `Queue::Updated` (+ attention projection) | Activity row layout |
| Approvals / interactions | pending snapshots + Requested/Resolved | PromptCoordinator / dialogs |
| Attention items | derived in Client Core from Agent/tool/queue/prompt/Turn | Activity Center presentation |
| Model / thinking | `ModelEvent` + optional tree markers | Composer menus |
| Connection status | transport observation (GUI → core msg) | StatusBar spinner |
| Context / cost | `cumulative_usage` when present | StatusBar formatting |
| Composer draft | none | per-Session / no-session draft |
| Scroll / Jump to latest | none | per-Agent scroll state |
| Sidebar sizes / visibility | hostd `[gui]` | local while editing, persisted after changes |
| Toast success | command correlation outcome | Notification layer |

## 5. Session Phases (Client Core)

```text
IdleNoSession
      │ OpenSession / CreateSession
      ▼
OpeningOrCreating          // identity CommandResponse may arrive here
      │ matching SessionOpened / SessionCreated
      ▼
Hydrating                  // still not Live
      │ matching SessionReconciled
      ▼
Live
      │ SessionCleared (matching previous live id)
      └──────────────────► IdleNoSession
```

Transport failure and command errors are orthogonal. They must not invent a
Session. Failed open/create restores the previous live Session when one exists.

Acceptance rules mirrored from TUI:

- Live incremental events: accept only for the live `session_id` while not
  hydrating a different target.
- Reconcile: accept for live session or the explicit open/create target
  (`opening_id` equivalent).

## 6. Canonical Cases

Notation:

- `→` Client Core effect / outbound command
- `←` inbound `ServerMessage`
- State names refer to §5 and the partitions in Client Core Design §5

Deterministic `command_id` values are allocated by the update context. Tests
must inject them.

### C1 — No-session startup and SessionList

**Intent:** `DiscoverSessions { scope, cwd? }`

```text
state: IdleNoSession
→ SessionList { command_id, scope, cwd? }
← CommandResponse { command_id, Ok(SessionListed { sessions, .. }) }
state: IdleNoSession + session_list projection
effects: none beyond completing the pending discover operation
```

Notes:

- Listing never opens a Session by itself.
- GUI may auto-open after list as a frontend policy; Core only performs the
  intent it is given.

### C2 — Open Session to Live

**Intent:** `OpenSession { session_id, session_path? }`

```text
state: IdleNoSession | Live(previous)
→ begin OpeningOrCreating(target=session_id); clear ephemeral projection for the pending target
→ SessionOpen { command_id, session_id, session_path? }
← CommandResponse { Ok(SessionOpened { session_id, .. }) }
state: Hydrating(target=session_id)   // still not Live
← SessionReconciled { session_id, reason=InitialHydration, snapshot, agents, .. }
state: Live {
  identity, entries/leaf, agents, selected agent,
  per-agent committed timelines from active branch,
  active_turns, pending approvals/interactions, usage
}
effects: none required; optional frontend rename/submit follows GUI policy
```

Reject:

- `SessionOpened` alone must not populate Timeline as live.
- Foreign `SessionReconciled` must be ignored.

### C3 — Create Session to Live

**Intent:** `CreateSession { cwd }`

Same phase machine as C2 with `SessionCreate` → `SessionCreated` → matching
`SessionReconciled`.

First-submit path (TUI-compatible):

```text
IdleNoSession + stashed submit text
→ CreateSession
… Hydrating …
← SessionReconciled
→ ChatSubmit { target = selected agent after reconcile, text = stash }
```

### C4 — Refresh

**Intent:** `RefreshSession`

```text
Live(session_id)
→ StateSnapshot { command_id, session_id }
← CommandResponse { Ok(Empty) }          // not an apply path
← SessionReconciled { reason=ExplicitRefresh, .. }
state: Live with atomically replaced projection; realtime drafts discarded
```

### C5 — Clear / delete visible Session

**Intent:** `DeleteSession { session_id }` (visible session)

```text
Live(session_id)
→ SessionDelete { command_id, session_id }
← CommandResponse { Ok(Empty) }          // not yet cleared
← SessionCleared { previous_session_id = session_id }
state: IdleNoSession                     // all session-owned projection gone
```

An empty `SessionListed` is **not** a clear signal.

### C6 — Committed and realtime transcript

```text
Live + selected agent A
← RealtimeMessage { session_id, agent_instance_id=A, message_id=M, delta_seq, delta }
state: ephemeral draft for M on A's timeline
← TranscriptCommitted { session_id, agent_instance_id=A, message_id=M, transcript_seq, message }
state: committed M replaces draft; later realtime for M ignored
```

Rules:

- Scope by session and `agent_instance_id`.
- Deduplicate committed by message identity; conflicting seq/content may emit
  refresh intent (`StateSnapshot`) as TUI does.
- Reconcile discards unrecovered realtime drafts.

### C7 — Agent select / subscribe / replay

**Intent:** `SelectAgent { agent_instance_id }`

```text
Live(session_id)
→ AgentSubscribe { command_id, session_id, agent_instance_id, after_seq=None }
← CommandResponse { Ok(AgentSubscribed { snapshot, replay, next_seq, .. }) }
state: selected agent = agent_instance_id;
       selected timeline cleared then rebuilt from snapshot.events or replay
```

Foreign session subscribe results are ignored. Selecting the already-selected
Agent may still subscribe; Core should remain idempotent in projection.

### C8 — Submit and cancel Turn

**Intent:** `SubmitTurn { text }`

```text
Live + selected agent A
precondition: text non-empty after trim; session Live; target Agent known
→ ChatSubmit { command_id, session_id, target_agent_instance_id=A, text }
← CommandResponse { Ok(Empty) }          // no turn_id here
← TurnLifecycle::Queued | Started { turn_id, agent_instance_id=A, .. }
state: active_turns[A] tracks turn_id / status
← TranscriptCommitted / RealtimeMessage as usual
← TurnLifecycle::Completed | Failed | Cancelled
state: clear or update A's active turn
```

**Intent:** `CancelTurn`

```text
resolve turn_id from selected Agent's active turn
→ TurnCancel { command_id, session_id, turn_id }
← Empty + TurnLifecycle Cancelling/Cancelled (or queued cancel)
```

No optimistic user message insertion. Send may remain available when host queue
rules allow; Stop is shown only while the selected Agent has a cancellable Turn
(GUI chrome over core Turn projection).

### C9 — Approval lifecycle

```text
← Approval::Requested { session_id, approval_id, agent_instance_id, tool_name, tool_args, .. }
state: pending_approvals contains approval_id
→ ApprovalRespond { command_id, session_id, approval_id, decision, note? }
state: prompt remains pending; mark response-in-flight for that id
← Approval::Resolved { approval_id, decision, .. }
state: remove pending approval
```

Same-process reconcile restores `snapshot.pending_approvals` atomically.
Response must not remove the prompt before Resolved/reconcile.

Field note: live request uses `tool_args`; snapshot uses `request`. Client Core
normalizes into one pending-approval projection.

### C10 — User interaction lifecycle

```text
← Interaction::Requested { interaction_id, questions, require_confirm, .. }
→ UserInteractionRespond {
    Submit { answers } | Cancel { reason? }
  }
← Interaction::Resolved
```

Cancel must send an explicit response, not only close GUI chrome. Priority when
both exist: approvals before interactions; stable host order within a kind.

### C11 — Navigate / fork (Conversation Map authority path)

**Intent:** `NavigateSession { entry_id, summarize, custom_instructions? }`

```text
→ SessionNavigate { .. }
← SessionNavigated { old_leaf_id?, new_leaf_id?, selected_entry_id, editor_text?, .. }
← SessionReconciled   // authoritative entries + current_leaf_id
state: rebuild active-branch timelines; preserve GUI expansion for surviving ids
```

Fork:

```text
→ SessionFork { session_id, entry_id }
← SessionOpened (new id) + SessionReconciled(new id)
```

Track fork pending as an open/create-class operation.

### C12 — Transport failure and malformed JSONL

These are frontend transport observations delivered as Core messages:

| Observation | Core behavior |
|---|---|
| Connected | connection observation ready; no Session invented |
| Decode failure / malformed line | record transport error; do not mutate Session facts |
| Stdout closed / child exit | connection closed; clear pending commands as failed; do not fabricate `SessionCleared` |

GUI owns process restart policy. First Core version does not implement
multi-client reconnect; a new process hydrates again via open/create.

### C13 — Command error correlation

```text
pending_commands[command_id] = product operation
← CommandResponse { command_id, Err(message) }
state: clear only that pending operation; restore prior live Session on failed
       open/create when previous_live exists; surface error for GUI chrome
```

## 7. Effect Catalog (v1)

Client Core effects for the conversation path:

| Effect | Meaning |
|---|---|
| `Send(Command)` | Frontend transport writes one JSONL command |
| (none) | Pure projection update |

Deferred as frontend-only: spawn/stop hostd, redraw, clipboard, URL open, file
dialogs, notifications, app exit, focus moves.

## 8. Non-Blocking Ambiguities

Documented for implementers; none block M1/M2:

1. **`ConfigUpdate` lacks typed `CommandResponse`.** Correlate via `ModelEvent`.
2. **`ChatSubmit` / `TurnCancel` return `Empty`.** Discover `turn_id` from
   `TurnLifecycle` / `active_turns`.
3. **`AgentUnsubscribe` is effectively unused** by TUI; Core may omit intent.
4. **Reconnect / `RetentionExhausted` / epoch cursors** exist on wire but are
   out of first Core scope.
5. **TUI does not project `cumulative_usage` today.** Core should still store it
   from reconcile for GUI StatusBar.
6. **Slash-command catalog / settings forms / auth flows** are deferred past
   the conversation vertical slice.
7. **Serde tag mismatch** (`Command.type` vs `ServerMessage.kind`) is a codec
   concern for the GUI transport adapter, not Core.

## 9. Phase 2 Fixture Notes

Build fixtures from §6 without GPUI/TUI types:

- helpers to construct `SessionSnapshot`, `AgentInfo`, `SessionReconciled`,
  sequenced `RealtimeMessage` / `TranscriptCommitted`, approval/interaction
  pairs, and `AgentSubscribed` with empty-vs-nonempty `snapshot.events`;
- deterministic command-id source;
- assert phase, selected agent, timeline item identities, pending prompts,
  pending command map, and emitted `Send` commands.

Suggested initial test names mirror the cases: `c2_open_waits_for_reconcile`,
`c6_committed_replaces_draft`, `c7_subscribe_prefers_snapshot_events`,
`c9_response_keeps_prompt_until_resolved`, `c5_clear_only_on_session_cleared`.

## 10. Exit Criteria Checklist

- [x] Primary-path operations have input sequences, expected Core state, and
      effects (§6).
- [x] Agent subscription assumptions resolved (§3.1).
- [x] Conversation Map / `entries` / leaf / navigate assumptions resolved
      (§3.2).
- [x] Model/thinking read/write path resolved (§3.3).
- [x] Workbench facts mapped to protocol or GUI-local owners (§4).
- [x] Protocol gate: no up-front DTO change required for open, transcript,
      submit, stop, approval, or interaction (§2).
- [x] Ambiguities that remain are recorded as non-blocking (§8).

Phase 0 and the Phase 2 Client Core kernel are complete. The remaining release
decision is M4, governed by the automated and native GUI gates in the
implementation plan.
