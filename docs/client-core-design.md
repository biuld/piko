# Client Core Design

> Status: proposed normative design (not yet implemented)
> Replaces: `tui-hostd-interaction-design.md`
> GUI consumer: [GPUI Desktop Client Design](gui-design.md)
> Execution plan: [GPUI Desktop Client Implementation Plan](gui-implementation-plan.md)
> Related: [Single-Agent Runtime Model](single-agent-runtime-model.md),
> [Turn-Agent Run Boundary](turn-agent-run-boundary-design.md),
> [TUI Elm-ish Runtime](../packages/tui/docs/design/elmish-runtime.md),
> [Session View Lifecycle](../packages/tui/docs/features/session-view-lifecycle.md)

## 1. Feature Contract

### 1.1 Overview

The GPUI desktop client is built on one headless Client Core. The Client Core
interprets the hostd protocol, maintains the authoritative client-side
projection of the visible session, and turns product-level user intents into
hostd commands.

The existing ratatui TUI remains unchanged during the GUI-first phase. It is the
product and behavioral reference used to derive Client Core semantics, not an
initial consumer of the new crate. A later project may migrate the TUI onto
Client Core after the GUI path is stable, but that migration is not part of this
design's first implementation scope.

The first Client Core feature is the common conversation path:

1. start with no session, open a session, or create one on first submit;
2. wait for authoritative reconciliation;
3. display the selected Agent's committed and realtime timeline;
4. submit and cancel Turns;
5. restore and answer approvals and user interactions;
6. clear or rebuild the visible projection when hostd instructs it.

### 1.2 User-visible behavior

- GUI preserves the authoritative Session, Agent, Turn, transcript, approval,
  and interaction semantics already established by hostd and exercised by the
  TUI reference implementation.
- Opening or creating a session remains visibly loading until hostd sends
  `SessionReconciled`; an identity-only command result never exposes a partial
  live view.
- Committed transcript entries survive reconciliation. Realtime drafts may be
  discarded and must never replace committed history.
- Events for another session or Agent do not leak into the visible projection.
- Pending approvals and interactions restored by same-process reconciliation
  become available to the GUI's native prompt UI.
- GUI may use desktop overlays, docks, pointer input, and GUI-specific
  shortcuts without changing the underlying product operation. TUI retains its
  existing terminal panels and bindings independently.

### 1.3 Configuration and persistence

Client Core does not define a persisted configuration namespace. Shared
product configuration remains hostd-owned. Presentation configuration remains
frontend-specific under namespaces such as `[tui]` and `[gui]`.

Client Core does not persist a second session cache. Recoverable session facts
come from hostd reconciliation and live events. Editor drafts, focus, scroll,
expanded rows, overlay placement, and window layout belong to the frontend.

### 1.4 Feature non-goals

- Sharing ratatui widgets, GPUI elements, layout engines, or focus managers
- Making Client Core authoritative over hostd session facts
- Connecting TUI and GUI to one shared hostd process in the first version
- Supporting multiple simultaneous writers for one live session
- Moving hostd or orchd execution logic into a frontend library
- Modifying or migrating the TUI during the GUI-first phase
- Migrating every TUI feature before the GUI conversation path is stable

## 2. Purpose and Scope

This document defines the target responsibilities and behavioral contracts of
the future `piko-client-core` crate. It generalizes the former TUI-to-hostd
interaction design so that host protocol semantics have one GUI-side owner,
while the unchanged TUI remains the comparison reference.

The design covers:

1. ownership and dependency boundaries;
2. the headless state projection;
3. product intents, messages, and effects;
4. hydration, live events, and command correlation;
5. GUI adapter responsibilities;
6. deriving compatible behavior from the unchanged TUI reference.

It does not redesign protocol DTOs, hostd session storage, orchd execution, or
frontend presentation.

## 3. Architecture and Dependencies

```text
                piko-tui
          unchanged reference client
             │ JSONL        ╲ behavioral reference
             ▼               ╲
           hostd              ╲
             ▲                 ╲
             │ JSONL            ╲
          piko-gui ◄──── piko-client-core
                              ▲
                              │ DTOs
                         piko-protocol
```

Both frontends remain hostd clients. Only the GUI depends on Client Core in the
first phase.

Dependency rules:

1. `piko-client-core` depends on `piko-protocol` only, plus general-purpose
   data-structure or serialization libraries when required.
2. It does not depend on `piko-tui`, GPUI, hostd, orchd, or a concrete async
   runtime.
3. GUI depends on Client Core and owns its transport and presentation adapters.
4. TUI continues to depend directly on `piko-protocol`; no TUI source, state,
   test, or dependency changes are required by the GUI-first phase.
5. `piko-protocol` remains serializable DTOs only; client projection logic does
   not move into the protocol crate.

## 4. Responsibility Boundaries

| Concern | Owner | Client Core responsibility | Frontend responsibility |
|---|---|---|---|
| Durable session facts | hostd | Mirror and reconcile | Render only |
| Agent runtime facts | hostd | Scope and project | Render names/status |
| Turn lifecycle | hostd | Track target-aware state | Show progress/cancel affordance |
| Committed transcript | hostd | Deduplicate and project | Layout and style entries |
| Realtime draft | hostd stream | Maintain ephemeral projection | Animate/render streaming |
| Approvals/interactions | hostd | Queue and correlate responses | Native panel/modal workflow |
| Commands | Client Core | Convert product intent to `Command` | Supply intent and local input |
| Command correlation | Client Core | Track business outcome | Present loading/error chrome |
| hostd process/JSONL I/O | frontend adapter | Accept decoded messages only | Spawn, pump, stop, report failure |
| Layout/focus/keymap | frontend | None | Full ownership |
| Editor draft/selection | frontend | None, except submit recovery metadata | Full ownership |
| TUI/GUI settings | each frontend + hostd storage | None | Schema, defaults, rendering |

### 4.1 Host authority

**Invariant A1.** hostd is the only writer of user-visible Session, Turn, Agent,
approval/interaction, and committed transcript facts. orchd never talks to
Client Core or a frontend directly.

**Invariant A2.** Client Core never invents authoritative Agents, display names,
transcript entries, Turns, approvals, or interactions. Loading and explicit
empty states are not session facts.

**Invariant A3.** A Client Core consumer does not maintain a parallel
authoritative session projection. The GUI renders Client Core projection and
keeps only presentation-local state. The unchanged TUI remains outside this
invariant during the GUI-first phase.

## 5. Headless State Model

The exact Rust types may evolve, but the state has these semantic partitions:

```text
ClientState
├── shell
│   ├── connection observation
│   └── bootstrap readiness
├── session
│   ├── phase and open target
│   ├── session identity and tree
│   ├── AgentInstance hierarchy and selected AgentInstance
│   ├── timeline projection per AgentInstance
│   ├── active Turns per AgentInstance
│   ├── pending approvals
│   ├── pending interactions
│   ├── queue projection
│   ├── attention projection derived from authoritative session state
│   └── cumulative usage
├── model
│   └── active model/provider/thinking projection
└── pending_commands
    └── command id → product operation
```

### 5.1 Session phases

```text
IdleNoSession
      │ open/create
      ▼
OpeningOrCreating
      │ identity result
      ▼
Hydrating
      │ SessionReconciled
      ▼
Live
      │ SessionCleared
      └──────────────────► IdleNoSession
```

Transport and recoverable errors are orthogonal to the session phase. A
frontend may present them differently, but the core exposes enough state to
distinguish loading, no-session, live, and failed operations.

### 5.2 Timeline projection

Client Core stores structured presentation-neutral items, not pre-rendered
terminal lines or GPUI elements. The projection preserves message identity,
sequence, role, structured content, Agent identity, source Turn, realtime versus
committed status, and tool lifecycle data.

Client Core owns:

- committed-message deduplication;
- realtime delta ordering and assembly;
- replacing or suppressing a draft after its committed message arrives;
- routing entries by `agent_instance_id`;
- discarding ephemeral drafts on full reconciliation;
- requesting refresh when a reliable sequence inconsistency is detected.

Frontends own Markdown layout, code highlighting, folding, tool-card expansion,
row measurement, virtual scrolling, and visual animation.

## 6. Update Contract

Client Core follows an Elm-ish update boundary:

```text
(ClientState, ClientMsg, UpdateContext) → (ClientState, Vec<ClientEffect>)
```

Message categories:

- **Host message:** a decoded `ServerMessage` from the transport adapter.
- **Product intent:** a frontend-neutral user operation such as open session,
  submit Turn, select Agent, cancel Turn, or answer an approval.
- **Transport observation:** connected, decode failure, or closed. Transport
  observations inform state but do not make the core own process I/O.

Effects are initially limited to protocol operations such as sending a
`Command`. Process spawn, redraw, clipboard, URL opening, file dialogs,
notifications, and application exit remain frontend effects.

Command ids are allocated through an injected deterministic source in the
update context. The reducer must not call UUID or wall-clock APIs directly.

## 7. Product Intents

The first version exposes semantic operations for the GUI conversation path:

| Intent | Required core behavior |
|---|---|
| Discover sessions | Emit `SessionList`; correlate result |
| Open session | Begin hydration; emit `SessionOpen` |
| Create session | Begin creation; emit `SessionCreate` |
| Refresh session | Emit `StateSnapshot`; await reconcile |
| Select Agent | Select host-provided AgentInstance and manage subscription when required |
| Submit Turn | Validate live session/Agent; emit `ChatSubmit` |
| Cancel Turn | Resolve selected Agent's active Turn; emit `TurnCancel` |
| Respond approval | Validate pending prompt; emit `ApprovalRespond` |
| Respond interaction | Validate pending prompt; emit `UserInteractionRespond` |
| Navigate/fork/delete session | Correlate command; await reconcile/clear where required |

Physical shortcuts, buttons, menus, slash-command parsing, and palette entries
map to these intents in the GUI adapter. Client Core does not route raw keyboard
or pointer events. The TUI continues using its existing action routing.

## 8. Hydration and Reconciliation

**Invariant H1 — Authoritative replacement.** The only messages that replace
the visible session projection are `SessionReconciled` and `SessionCleared`.

**Invariant H2 — Identity is not hydration.** `SessionCreated` and
`SessionOpened` identify the target and complete their command result, but do
not bind a live projection. The state becomes `Live` only after a matching
`SessionReconciled` is applied.

**Invariant H3 — Snapshot is a request.** `StateSnapshot` requests a refresh;
its result is not a second apply path. The resulting projection arrives through
`SessionReconciled`.

**Invariant H4 — Atomic apply.** Reconciliation replaces session identity,
tree/leaf, Agent projection, timelines, active Turns, pending approvals,
pending interactions, queue-derived session state, and usage as one logical
operation. Frontends must not render a mixture of old and new session facts.

**Invariant H5 — Matching target.** A reconcile is accepted only for the
current live session or the explicit open/create target. Other reconciles and
session-scoped live events are ignored.

**Invariant H6 — Delete to empty.** Deleting the visible session becomes
authoritative only when `SessionCleared` arrives. An empty list result is not a
clear signal.

### 8.1 Recoverable projection

The reconcile payload must recover, for the current hostd process:

| Projection | Source |
|---|---|
| Session identity, cwd, name | `SessionSnapshot` |
| Transcript tree and leaf | `entries`, `current_leaf_id` |
| Selected AgentInstance | `selected_agent_instance_id` |
| Runtime Agent hierarchy, state, and display names | `SessionReconciled.agents` |
| Active Turns | `active_turns` |
| Pending approvals | `pending_approvals` |
| Pending interactions | `pending_interactions` |
| Usage | `cumulative_usage` |

Realtime drafts, frontend editor state, focus, scrolling, expansion, and window
layout are not hydration authority.

Pending approvals and interactions can be recovered only while their owning
hostd process is still alive. After process exit, hostd cannot recreate pending
oneshot responders; interrupted Turns follow hostd's interruption/finalization
rules instead.

## 9. Live Projection Rules

**Invariant L1.** Every session-scoped live event is rejected unless its
`session_id` matches the live session.

**Invariant L2.** Agent-scoped transcript, realtime, tool, approval, and
interaction data is routed by `agent_instance_id`. The selected view is not an
ambient identity.

**Invariant L3.** `AgentInfo.name` is the primary display label. `agent_id` is
template identity and `agent_instance_id` is runtime identity; neither is a
replacement for the host-provided display name.

**Invariant L4.** Committed transcript is authoritative and durable. Realtime
messages and in-flight tool chrome are best-effort and may be dropped during
reconciliation.

**Invariant L5.** Turn running state follows `TurnLifecycle` and reconciled
`active_turns`, never assistant text completion or runtime availability
inference.

**Invariant L6.** Approval and interaction requests remain pending until their
resolved event or a full reconcile removes them. Sending a response does not
optimistically delete the prompt.

## 10. Transport and Command Results

The initial frontends each spawn their own hostd child and communicate over
JSON-lines stdio:

- frontend to hostd: one serialized `Command` per line;
- hostd to frontend: one serialized `ServerMessage` per line.

Transport acceptance and business outcome are distinct. There is no early
transport ack message. A command completes through its typed
`CommandResponse`, required authoritative follow-on events, or both.

Every command carries a `command_id`. Client Core correlates that id with the
pending product operation. An `Empty` command response is valid only for an
operation whose typed business result is empty; required lifecycle or projection
events still arrive separately.

No reconnect or multi-client attachment is required in the first Client Core
version. A new frontend process spawns a new hostd process and hydrates again.

## 11. GUI Adapter Contract

GUI must:

1. spawn and stop its hostd transport;
2. decode transport lines into core messages;
3. execute core `Send(Command)` effects;
4. translate keys, pointer actions, menus, and commands into product intents;
5. render Client Core projection without mutating authoritative facts;
6. retain frontend-local drafts, focus, selection, scroll, layout, and chrome;
7. represent core loading, empty, live, and error states honestly.

GUI is free to use Workbench, Stack, Split, Dock, modal cards, drawers,
animation, and pointer interactions. That presentation model is not part of
Client Core. The unchanged TUI remains free to use Slot A-E, panel replacement,
and LIFO focus without adapting those concepts to Client Core.

## 12. Initial Implementation Boundary

The first implementation slice includes:

- session phase and command correlation;
- `SessionReconciled` / `SessionCleared`;
- session scoping and structured AgentInstance hierarchy;
- structured transcript and realtime projection;
- selected Agent and active Turn projection;
- submit/cancel intents;
- approval and interaction projection/response intents;
- frontend-neutral attention items derived from Agent, tool, queue, prompt, and
  Turn state;
- pure reducer and projection tests.

Deferred until the GUI conversation path is stable:

- auth and model catalog workflows;
- settings forms and frontend config schemas;
- slash-command parsing and completion;
- session tree editing UI;
- notification presentation;
- shared transport runtime;
- multi-window or multi-client coordination.

## 13. GUI-First Delivery Strategy

Client Core is derived from TUI behavior without moving code out of the TUI or
changing the TUI dependency graph. During this phase, "derived" means a clean,
headless implementation based on the normative hostd contract, existing TUI
behavior, and TUI tests; it does not mean a mechanical source extraction.

1. Record the existing TUI behavior for reconcile, transcript, Turn, approval,
   and interaction paths as Client Core contract cases.
2. Implement those cases in Client Core using protocol DTOs and pure tests.
3. Build the GUI transport adapter and minimum Workbench on Client Core.
4. Compare the GUI and unchanged TUI against the same hostd main-path scenarios.
5. Add GUI-native overlay and optional dock behavior without expanding core
   presentation responsibilities.
6. Evaluate TUI adoption only as a separate, explicitly approved project after
   the GUI conversation path is stable.

Temporary semantic duplication between the unchanged TUI reducer and Client
Core is accepted during the GUI-first phase. The normative protocol and this
document define the required behavior; comparison scenarios detect divergence.

## 14. Validation Requirements

Client Core tests do not link ratatui or GPUI. At minimum they verify:

- identity-only open/create results do not enter `Live`;
- matching reconcile atomically installs the visible view;
- foreign-session and foreign-Agent events do not leak;
- committed transcript deduplicates and supersedes realtime drafts;
- reconcile discards unrecoverable realtime state;
- selected Agent restoration and fallback use host-provided Agents;
- submit/cancel commands target the correct session, Agent, and Turn;
- command errors clear or restore the appropriate pending operation state;
- same-process reconcile restores approvals and interactions;
- prompt responses do not remove prompts before resolution;
- `SessionCleared` removes all session-owned projection;
- deterministic command ids make reducer tests repeatable.

Existing TUI tests remain unchanged. GUI rendering and interaction tests are
adapter tests, not substitutes for core projection tests. Cross-client scenario
checks may run both binaries, but must not require TUI source modifications.

## 15. Non-Goals

- A universal frontend widget/component library
- A shared layout, focus, keybinding, or overlay state machine
- Direct hostd or orchd library calls from Client Core
- Durable storage separate from hostd session storage
- Optimistic mutation of authoritative session projection
- A compatibility layer for deprecated or undocumented protocol paths
- Immediate deletion or feature freeze of the TUI
- Refactoring the TUI to consume Client Core during the GUI-first phase
- Requiring every current TUI feature before the GUI vertical slice begins
