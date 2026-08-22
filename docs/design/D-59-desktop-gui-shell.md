# D-59: Desktop GUI shell

> Status: accepted (Slices 1–6 landed; visual acceptance pending)
> Implements: [F-42](../features/F-42-desktop-gui-shell.md)
> Decisions: [ADR-022](../decisions/ADR-022-desktop-client-reintroduction.md)

## Goal

Deliver the F-42 two-column desktop shell — a floating sidebar, one
Timeline, and a bottom-floating Composer — in a single macOS GPUI window
that consumes host-authored projections through `piko-client-core` and
builds its chrome, layout, and focus from reusable `island-rs`
infrastructure.

## Constraints and non-goals

- hostd stays authoritative for all durable and user-visible product state
  (ADR-003, ADR-022). The shell stores only presentation preferences and
  recoverable drafts.
- `piko-client-core` remains the sole reducer for host-authored Timeline
  state (D-34 Slice 3b). The shell never forks projection logic.
- Product-independent GPUI machinery is island-rs territory; anything a
  second GPUI application could reuse must not be implemented privately in
  the piko crate (AGENTS.md package boundary 9).
- macOS/GPUI only in v1. Windows and Linux presentation are out of scope.
- Non-goals: a third column, per-message/tool/diff item rendering details
  (F-22 presentation-layer work), settings IA beyond the sidebar entry
  point, and any change to hostd behavior or wire vocabulary.
- The PRD's implementation questions are resolved by the landed v1 defaults:
  fixed adaptive sidebar width, one attention layer, and in-memory drafts.

## Proposed design

### Crate and module layout

New workspace member `piko-desktop` (binary) under `packages/desktop/`,
depending on `piko-client-core`, `piko-protocol`, `piko-comms`, and the
`island` crate. It owns only piko domain IDs, intents, host projections
and transport, localization, and product composition.

```text
packages/desktop/src/
├── main.rs            # app entry, window bootstrap, event loop
├── transport.rs       # hostd spawn + JSON-lines read loop
├── state.rs           # DesktopState: client-core State + shell-local fields
├── shell/
│   ├── mod.rs         # two-column composition and breakpoint logic
│   ├── keyboard.rs    # focus traversal + source-list keyboard routing
│   ├── lifecycle.rs   # warm reopen and hydration completion
│   ├── layers.rs      # model/thinking/attention/settings overlays
│   ├── rows.rs        # normalized Timeline row rendering
│   ├── sidebar.rs     # floating sidebar surface + narrow overlay mode
│   ├── timeline.rs    # Timeline surface, loading/empty/error, follow state
│   ├── composer.rs    # floating Composer, growth bound, occlusion
│   └── view.rs        # island window frame and product composition
├── focus.rs           # single focus owner, temp-layer stack, Escape policy
├── prefs.rs           # client-local presentation prefs and draft recovery
└── transport.rs       # shared host-client binding + client-core reduction
```

### State ownership

```text
hostd ──JSON-lines──▶ transport thread ──▶ client-core update::host
                                                │
                                                ▼
                              ClientState (sole projection store)
                                                │
                                                ▼
                 DesktopState (ClientState + shell-local GPUI entity state)
                    ┌───────────────────────────┼───────────────────────────┐
                    ▼                           ▼                           ▼
             sidebar surface             timeline surface             composer surface
```

Shell-local fields: window rect, sidebar presentation (`persistent` |
`collapsed`), selected surface focus owner, per-surface Composer drafts,
Timeline viewport follow state, return-to-latest visibility, and the
temporary-layer stack. None of these are product-authoritative: on
reconnect they are reconciled from the host snapshot, never merged into
it, and a host projection always wins a conflict.

### Window structure and layout

- The window composes navigation and Timeline through island's detached
  `WindowChromeFrame` and semantic `WorkspaceChrome` zones. The Timeline
  surface receives all remaining width.
- The sidebar renders as an elevated floating surface with visible
  separation from the window boundary and Timeline canvas (F-42 Window
  structure). v1 uses one adaptive product width, not user-resizable
  (F-42 resolved implementation question 1).
- Window controls, navigation controls, and Timeline actions live in the
  island chrome header zones; there is exactly one window header, never
  two competing title bars.
- The Composer floats inside the Timeline column with bottom and side
  margins, bounded width (readable composition), and never extends
  beneath the sidebar (F-42 Floating Composer).

### Responsive sidebar

- A breakpoint guards the minimum usable Timeline width. Above it the
  sidebar is persistent; below it the sidebar leaves the layout and a
  visible header control opens it as a temporary island overlay layer
  over the window. Selection or Escape closes the layer. Widening the
  window restores the persistent surface without changing the selected
  session or agent (F-42 Floating sidebar).
- Presentation preference (`persistent` | `collapsed`) is client-local
  and restored from prefs; sidebar contents always come from the current
  host projection.

### Timeline surface

- Renders the canonical selected-agent projection from client-core's
  normalized timeline items (D-34 Slice 3b). Item identity is stable
  across streaming and authoritative commit.
- The surface is keyed by `selected agent_instance_id`. Selection change
  enters an explicit loading state for the new target before any item is
  shown; entries from the previous target can never be labeled as the
  current one. In-flight snapshot responses keyed to a stale target are
  dropped.
- Viewport follow state is shell-local: at the tail, new content keeps
  the latest item in view; scrolled away, streaming never steals the
  position and a compact return-to-latest affordance appears above the
  Composer.
- Trailing scroll padding equals the current Composer footprint plus the
  return-to-latest affordance, recomputed when the Composer resizes. The
  final item can always scroll fully above both.
- Text selection and copy work without moving focus to another surface.

### Floating Composer

- Multiline input grows to a bounded maximum height, then scrolls
  internally. Resizing updates Timeline trailing space without losing
  the draft, selection, or follow choice.
- Submit maps to `ClientIntent::SubmitTurn`; cancel maps to
  `CancelTurn`. Empty submission is a no-op. A failed submission keeps
  the draft and shows an actionable error attached to the Composer; only
  an accepted submission clears the submitted draft (disposition per
  D-34 §3). Target agent, model, thinking level, and context state are
  available from the Composer chrome via temporary layers, never a
  permanent status column.
- A disconnected or non-live session disables host-required actions
  while keeping draft text recoverable.

### Focus and temporary layers

- Sidebar, Timeline, Composer, and any temporary layer resolve to a
  single focus owner through the island focus contracts. Opening a
  temporary layer transfers focus; closing restores focus to the
  initiating surface when it still exists.
- Temporary layers (model select, thinking select, session actions,
  approvals, interactions) ride the island overlay host and never
  resize the two-column shell. Escape dismisses the top layer or cancels
  its provisional action; it never discards a Composer draft and never
  exits the application.
- Keyboard traversal reaches every primary shell action via island
  list-keyboard focus contracts; pointer input invokes the same product
  intents.

### Transport and lifecycle

- The TUI's `HostdClient` (spawn + stdio + JSON-lines read loop,
  `packages/tui/src/host.rs`) moves into `piko-comms` as a shared,
  contract-parameterized host client. TUI migrates to it; desktop
  registers a `DesktopHostBridge` comms contract with its own capacity
  policy. The wire stays the JSON-lines `Command`/`ServerMessage`
  protocol; no new wire types are expected, and any projection gap is
  added only as an additive protocol change with `#[serde(default)]`.
- Connection states: `connecting` (spawn), `hydrating` (bootstrap:
  DiscoverSessions / OpenSession / ListModels / SyncModelConfig),
  `live`, `disconnected`, and `decode-error`. All are observable shell
  states per F-42; drafts and presentation prefs survive each.
- Restart restores client-local prefs (window rect, sidebar
  presentation) and reconciles sessions, selected agent, Timeline,
  runtime status, usage, and pending actions from the host before
  presenting anything as current.

### Prefs and drafts

- Client-local file under `$PIKO_HOME` (e.g. `desktop-prefs.json`),
  owned by the desktop client, holding window rect, sidebar
  presentation, and last-known session id for warm reopening. It never
  flows through hostd settings; no `[gui]` hostd namespace is restored
  (ADR-022).
- Non-durable Composer drafts survive session switches and temporary
  disconnects in memory. Draft persistence across a full application
  restart is deferred by F-42 resolved implementation question 3; the prefs
  file format leaves room for a later feature without committing to it.

## Package impact

| Package | Change |
|---|---|
| `piko-desktop` | New binary crate: shell composition, transport wiring, focus, prefs, theme. |
| `piko-comms` | Shared `HostdClient` (spawn + read loop) extracted from TUI; `DesktopHostBridge` contract and capacity policy. |
| `piko-tui` | Migrate to the shared host client; no behavior change. |
| `piko-client-core` | Reused as-is for state, intents, timeline; no change expected. |
| `piko-protocol` | No change expected; additive-only if a projection gap appears. |
| `piko-hostd` | No change. |
| `island-rs` | Consumed; gaps found while landing (e.g. floating-surface elevation material, overlay-host details) become island features, not piko-private components. |

## Reusable infrastructure

Island provides the reusable contracts; piko composes them. Integration
contract per surface:

- Two-column shell and headers: island `workspace-presentation`,
  `window-chrome-layout-host`, `chrome-host`, `split-window-chrome`.
- Floating sidebar: island `source-list` (session/agent navigation) with
  `list-keyboard` and `island-focus`; elevation/separation via island
  `material`/chrome. If the detached floating-surface presentation is
  missing, it is added to island as a feature and consumed here.
- Temporary layers: island `overlay-host` / `overlay-composite`.
- Timeline: island scroll + Markdown rendering over client-core normalized
  variable-height items. `collection-view` is intentionally not used because
  its uniform/justified virtualization contracts do not model variable-height
  streaming conversation rows.
- Composer: island `form-controls` with growth bound handled by piko
  shell logic (the bound is product policy).
- Any behavior a second GPUI application would need must be raised to
  island; piko-private code is limited to domain IDs, intents,
  projections, transport, localization, and product composition
  (AGENTS.md).

## Failure and cancellation

- Transport closure or decode failure moves the shell to an explicit
  disconnected/error state; host-required actions disable and drafts
  remain. On reconnect, hydration replaces the projection wholesale.
- Selection-change races: responses addressed to a stale target are
  dropped; the loading state is target-keyed so stale content is never
  shown as current.
- Submission failure preserves the draft and shows an operation-scoped
  error; host errors never replace unrelated Timeline content.
- Temporary layers restore focus on close only when the initiating
  surface still exists; unmounted surfaces leave focus to the shell's
  focus stack default.

## Verification

- Unit tests: Composer growth bound and Enter policy; sidebar breakpoint,
  keyboard reveal, and source-list routing; target-keyed Timeline loading
  guard and stable identity; follow-tail math; connection transitions;
  focus-stack restore; prefs round-trip.
- Integration-style reducer test: replay a recorded JSON-lines transport
  fixture through the production decoder and client-core reducer into shell
  state. Loading, empty, disconnected, streaming-follow, and failure behavior
  are also covered by focused state/reducer tests.
- Projection correctness stays covered by existing client-core tests.
- Manual acceptance against every F-42 acceptance criterion.

## Alternatives considered

| Alternative | Why not |
|---|---|
| Port TUI geometry or Dock Stack layout | Rejected by F-42; terminal slots solve different constraints |
| Desktop-private projection store | Duplicates client-core; violates D-34 Slice 3b sole-reducer rule |
| `[gui]` settings namespace on hostd | Rejected by ADR-004/ADR-022; presentation prefs are client-local |
| Desktop-private chrome/layout/focus | Violates AGENTS.md package boundary 9; island-rs exists for this |
| Permanent third column | Rejected by F-42 product decision |

## Rollout

1. Crate skeleton + island window; extract shared `HostdClient` into
   `piko-comms`, migrate TUI; connection state machine with observable
   connecting/hydrating/live/disconnected/error states. (landed)
2. Timeline surface over the client-core projection: target-keyed
   loading, empty (no session / no entries), follow-versus-reading, and
   error states. (landed)
3. Floating sidebar: source-list navigation, session/agent selection
   intents, narrow-window collapse to overlay layer and restore. (landed)
4. Floating Composer: submit/cancel mapping, growth bound with internal
   scroll, Timeline occlusion padding, return-to-latest affordance. (landed)
5. Focus stack and temporary layers: model/thinking/session/approval
   surfaces, Escape semantics, focus restore. (landed)
6. Client-local prefs restore and host reconciliation on restart. (landed;
   final visual acceptance remains recorded in V-59)
