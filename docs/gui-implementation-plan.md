# GPUI Desktop Client Implementation Plan

> Status: M0–M3 implemented; M4 automated gates pass and native manual gates remain open
> Product contract: [GUI Workbench](gui-workbench-feature.md)
> Architecture: [GPUI Desktop Client Design](gui-design.md)
> Headless projection: [Client Core Design](client-core-design.md)
> Phase 0 baseline: [Client Core Contract Baseline](client-core-contract-baseline.md)

## 1. Objective

Deliver a macOS-first GPUI desktop client that talks to hostd over the existing
JSONL protocol and completes the primary piko path:

1. discover, create, or open a Session;
2. reconcile and render its authoritative transcript;
3. select an Agent and submit or stop a Turn;
4. observe realtime, tool, queue, and Agent state;
5. resolve approvals and user interactions;
6. retain a complete single-column path while adding desktop sidebars and true
   overlay behavior.

The plan creates `packages/client-core` and `packages/gui`. It does not modify
or migrate `packages/tui`, and it never moves hostd or orchd logic into either
new crate.

## 2. Delivery Rules

These rules apply to every phase:

1. hostd remains the only authority for Session, Agent, Turn, prompt, and
   committed transcript facts;
2. Client Core contains no GPUI, process, filesystem, or concrete async-runtime
   types;
3. GUI owns hostd process lifecycle, JSONL I/O, GPUI entities, layout, focus,
   drafts, and other presentation state;
4. TUI remains unchanged and is used only as a behavioral comparison client;
5. protocol changes are evidence-driven, additive when possible, and land with
   hostd producer/consumer tests before Client Core or GUI consumes them;
6. GPUI dependencies remain pinned to exact compatible revisions selected by
   the dependency spike;
7. phase numbers express product dependency, not a requirement for fully serial
   execution; a worktree starts only after its declared inputs are stable;
8. every phase leaves the workspace buildable and its completed path usable.

## 3. Milestones

| Milestone | Completed after | User-visible outcome |
|---|---|---|
| M0 — Feasible stack | Phase 1 | Pinned GPUI stack can render and accept input on macOS |
| M1 — Read/write slice | Phase 4 | Open Session, read transcript, select Agent, send and stop a Turn |
| M2 — Primary path | Phase 5 | Approval, interaction, queue, tool, and failure states are operable |
| M3 — Desktop Workbench | Phase 6 | Session sidebar, stacked Inspector, responsive Sheets, and overlays work |
| M4 — Release candidate | Phase 8 | Main path passes automated and manual regression gates |

M1 is the first integration checkpoint, not the first release. M2 satisfies the
protocol path. M3 adds the required qualitative improvement over the TUI.
The current workspace reaches M3 and passes the Phase 8 automated commands,
including the real-hostd smoke. It must not be called M4 until the native macOS
checklist also passes.

### 3.1 Multi-Agent Worktree Strategy

Parallel work is organized as short-lived waves around one integration
worktree. The integration owner is responsible for shared manifests, public
facades, cross-module wiring, and merge gates. Feature worktrees implement
bounded modules against an agreed interface commit.

The recommended maximum is one integration owner plus two or three concurrent
feature worktrees. More branches would spend more time coordinating changes to
Client Core state and the GPUI root than they save.

Before the first parallel wave, the integration owner:

1. creates empty `packages/client-core` and `packages/gui` crate skeletons;
2. adds both workspace members in one commit;
3. establishes planned module directories and minimal public facades;
4. assigns file/path ownership for the wave;
5. records the base commit every worktree must use.

Workers do not independently add workspace members, select dependency
revisions, or reconcile `Cargo.lock`.

### 3.2 Parallel Waves

| Wave | Concurrent worktrees | Prerequisite | Integration gate |
|---|---|---|---|
| A — independent gates | protocol/host behavior audit; GPUI dependency spike; test-harness survey | crate skeleton/base commit | Phase 0 contract accepted and exact GPUI pins landed |
| B — foundations | Client Core Session kernel; host transport/codec/process adapter; static GUI shell/view modules | relevant Wave A gate plus frozen command/transport envelope | core tests pass and fake transport reaches the Session shell |
| C — shell integration | Client bridge; Session/sidebar view models; core Agent/subscription completion | stable core shell facade plus GPUI pin | real hostd reaches reconcile in GUI |
| D — conversation | Timeline renderer; Composer/actions; Agent Tree/selection | Phase 3 shell integrated and conversation view-model interfaces frozen | M1 read/write/stop scenario passes after merge |
| E — operations | Activity Center; tool cards; PromptCoordinator/dialogs | Turn/tool/prompt projections frozen | approval and interaction scenarios pass |
| F — desktop | Conversation Map; responsive layout/Sheets; GUI settings persistence | M2 and layout-state schema | single-column and wide Workbench both pass |
| G — hardening | Markdown/scroll performance; theme/accessibility/IME; regression harness/docs | M3 behavior frozen | M4 release suite passes |

Wave A is the first safe acceleration: the Phase 0 semantic audit and Phase 1
GPUI spike have no dependency on each other. Wave B begins per track: Client
Core waits for the contract baseline, GUI presentation waits for the GPUI pin,
and transport waits only for the frozen JSONL/command envelope. Phase 3 bridge
integration waits for both foundations. Later waves begin only after the
data/view-model interfaces they consume have an integration commit.

### 3.3 Suggested Path Ownership

Exact filenames may evolve, but each wave preserves these ownership seams:

During the initial Phase 2 kernel, one Core owner controls all
`packages/client-core/src/**`, especially `ClientState`, `update`, and
reconciliation. A second Core worktree may add black-box fixtures under
`packages/client-core/tests/**` after the public test facade freezes. The finer
module ownership below becomes safe only after that kernel is integrated.

| Track | Owned paths | Must not edit directly |
|---|---|---|
| Client Core session | `packages/client-core/src/session/**`, focused tests | GUI, protocol, workspace manifests |
| Client Core timeline | `packages/client-core/src/timeline/**`, focused tests | root reducer wiring and public re-exports |
| Client Core operations | `packages/client-core/src/agent/**`, `turn/**`, `prompt/**`, `attention/**` | protocol DTOs without a separate gate |
| GUI transport | `packages/gui/src/transport/**`, transport tests | GPUI views and Client Core state |
| Timeline UI | `packages/gui/src/workbench/timeline/**` | Composer and root layout |
| Composer UI | `packages/gui/src/workbench/composer/**` | Timeline and prompt coordinator |
| Inspector UI | `packages/gui/src/inspector/**` | center Workbench layout |
| Prompt UI | `packages/gui/src/overlays/**` | Client Core prompt authority |
| Visual system | theme implementations after token schema freezes; vendored GUI icons | shared token/config schema, product reducers, and protocol |

The integration owner alone normally edits:

- root `Cargo.toml` and `Cargo.lock`;
- new crate manifests once the dependency spike starts;
- Client Core `lib.rs`/`mod.rs` re-exports, top-level state/update routing, and
  public facade;
- GUI application root, Client bridge wiring, actions/keymap, shared view-model
  enums, and top-level Workbench layout;
- shared theme-token and GUI config schemas;
- cross-track documentation and final formatting changes.

The GPUI spike is the sole temporary exception for dependency experiments. No
other worktree touches dependency versions or lockfiles; the integration owner
lands the selected exact pins and reconciled lockfile from the successful
spike.

A worker that needs one of these files proposes the required interface change
to the integration owner first. The owner lands a small interface commit, then
all affected worktrees rebase before continuing.

### 3.4 Merge Order and Worktree Validation

Within each wave, merge in dependency order:

1. protocol/host contract change, if Phase 0 proved one necessary;
2. Client Core projection and tests;
3. GUI leaf component or transport adapter;
4. application/root wiring;
5. cross-module tests and documentation.

Every feature worktree must:

- rebase onto the wave interface commit before handoff;
- run focused tests for its owned crate/module;
- avoid unrelated formatting or generated lockfile churn;
- describe its required wiring without performing broad integration edits;
- contain no TUI changes.

The integration worktree runs the wider crate/workspace checks after merging.
Concurrent Cargo builds use per-worktree target directories; worktrees do not
share a mutable `CARGO_TARGET_DIR`.

### 3.5 Work That Must Stay Serial

Do not parallelize both sides of these boundaries before the interface lands:

- protocol DTO plus hostd production/recovery plus Client Core consumption;
- `ClientState` shape plus top-level reducer routing plus GUI bridge mapping;
- GPUI/Zed revision selection plus `Cargo.lock` reconciliation;
- PromptCoordinator ordering plus Root overlay ownership;
- responsive collapse policy plus top-level Resizable composition;
- final workspace formatting, clippy fixes, packaging, and release sign-off.

Read-only audits can run concurrently, but one owner consolidates their results
into normative docs. This prevents multiple worktrees from making conflicting
architecture decisions while appearing to work on separate files.

## 4. Phase 0 — Contract Baseline

### Goal

Turn the existing protocol and TUI behavior into explicit implementation cases
before creating runtime code.

### Status

**Accepted.** Normative cases, ownership map, and resolved assumptions live in
[Client Core Contract Baseline](client-core-contract-baseline.md).

### Work

- Record canonical message sequences for:
  - no-session startup and `SessionList`;
  - `SessionOpen` / `SessionCreate` identity result followed by
    `SessionReconciled`;
  - `SessionCleared`;
  - committed plus realtime transcript delivery;
  - Agent selection/subscription and replay;
  - running and queued Turns, cancellation, and failure;
  - approval and interaction request, response, resolution, and reconcile;
  - host stdout closure and malformed JSONL.
- Map every Workbench fact to its protocol source or GUI-local owner.
- Verify whether selecting a non-current Agent requires `AgentSubscribe`, how
  its replay cursor is applied, and what state is restored by reconcile.
- Verify the exact relationship between `SessionSnapshot.entries`, the selected
  Agent, `current_leaf_id`, and `SessionNavigate` before implementing
  Conversation Map navigation.
- Verify model/thinking read and update behavior through `ModelList`,
  `ConfigGet` / `ConfigUpdate`, and `ModelEvent` before enabling Composer
  selectors.
- Produce reusable protocol fixture builders in Phase 2 from these cases; do
  not copy ratatui state or rendering code.

### Protocol gate

The existing commands and messages are sufficient for the basic M1/M2 path.
No protocol edit is planned up front. If an audit case cannot be expressed:

1. document the missing product fact;
2. add or adjust the DTO in `piko-protocol`;
3. implement and test hostd production/recovery semantics;
4. only then consume it in Client Core.

### Exit criteria

- Each primary-path operation has an input message sequence, expected Client
  Core state, and expected effect sequence.
- Agent subscription and Conversation Map assumptions are resolved in writing.
- No unresolved gap blocks Session open, transcript render, submit, stop,
  approval, or interaction.

All exit criteria are satisfied by the contract baseline document.

## 5. Phase 1 — GPUI Compatibility Spike

### Goal

Prove the desktop stack before product architecture depends on unstable APIs.

### Work

- Create the minimal `packages/gui` binary crate and add it to the workspace.
- Compile a macOS window using official GPUI, `gpui_platform`, and
  `gpui-component`.
- Prove these primitives in one disposable spike view:
  - GPUI Component initialization and `Root`;
  - multiline auto-growing Input with Enter and Shift+Enter;
  - Scrollable content and bottom detection;
  - Tree custom rows and programmatic selection;
  - horizontal and vertical Resizable panes;
  - Sheet, Notification, Dialog, and non-dismissable approval behavior;
  - focus restoration and a basic application action.
- Record exact GPUI/Zed revision, GPUI Component revision, Rust toolchain,
  deployment target, required features, and upgrade procedure.
- Replace moving branches or wildcard versions with exact revisions and commit
  the resolved lockfile.

### Exit criteria

- `cargo check -p piko-gui` succeeds on the supported macOS/toolchain pair.
- The spike accepts IME text, dispatches actions, scrolls, resizes, and opens a
  non-dismissable dialog without a custom overlay implementation.
- Dependency revisions and platform limitations are documented.
- A failed primitive has an approved replacement before Phase 3; otherwise the
  project stops at M0 rather than building around an unknown stack.

## 6. Phase 2 — Client Core Session Kernel

### Goal

Create the headless reducer and authoritative client-side projection without
linking GPUI or reusing TUI state.

### Work

- Create `packages/client-core` depending on `piko-protocol` only.
- Implement small cohesive modules for:
  - `ClientState` and Session phases;
  - `ClientMsg`, `ClientIntent`, and `ClientEffect`;
  - injected deterministic command ids;
  - pending command correlation;
  - Session discovery, open, create, refresh, reconcile, and clear;
  - Agent hierarchy and selected Agent;
  - structured committed/realtime Timeline items;
  - per-Agent Turn and queue projection;
  - prompt and frontend-neutral attention projection.
- Keep state replacement atomic on `SessionReconciled`.
- Reject wrong-Session and wrong-Agent events without presentation involvement.
- Build fixture helpers from Phase 0 sequences and test update results as pure
  `(state, message) -> state + effects` cases.

### Required tests

- identity-only create/open never enters `Live`;
- reconcile atomically replaces all recoverable Session facts;
- stale Session events do not leak into the visible projection;
- committed messages deduplicate and supersede matching realtime drafts;
- Agent selection and subscription effects are deterministic;
- Turn/queue/prompt state is scoped by owning Agent and Session;
- prompt responses remain pending until resolved/reconciled;
- command failure clears only the correlated pending operation;
- `SessionCleared` returns to an honest no-session projection.

### Exit criteria

- `cargo test -p piko-client-core` passes without linking GPUI, TUI, or hostd.
- Public core types contain no layout, focus, window, widget, process, or I/O
  concepts.
- The Phase 0 main-path fixtures are executable tests.

## 7. Phase 3 — GUI Shell, Transport, and Session Entry

### Goal

Connect a real GPUI window to a real hostd process and reach authoritative
Session hydration.

### Work

- Replace the spike view with the GUI application shell and Client Core bridge.
- Implement hostd executable resolution, child spawn, JSONL stdin writer,
  blocking stdout reader, decode/closed observations, and deterministic child
  shutdown in the GUI crate.
- Cross from the reader thread into GPUI through one declared bridge. The reader
  never mutates Client Core or GPUI entities.
- Implement initial views:
  - native `piko — <project>` window title;
  - Session Sidebar with current-folder search, New Session, live target, and
    pending target;
  - no-session, opening, creating, hydrating, live, and failed phases;
  - stable StatusBar connection/cwd fallback/context/cost slots;
  - local spinner and skeleton ownership.
- Preserve the previous live Session when a different open attempt fails.

### Validation

- Unit-test line decoding, command encoding, EOF, malformed JSON, and child-exit
  mapping without GPUI rendering.
- Test phase-to-view-model mapping with a fake transport.
- Run one real hostd smoke path: launch GUI, list Sessions, open/create, receive
  reconcile, and close without an orphaned child.

### Exit criteria

- GUI reaches `Live` only from matching `SessionReconciled`.
- A transport failure is visible and does not fabricate an empty Session.
- Session Sidebar and StatusBar remain usable at minimum window width.

## 8. Phase 4 — Conversation Vertical Slice

### Goal

Reach M1: read and write a real multi-Agent conversation.

### Work

- Render selected-Agent committed Timeline items and lightweight realtime text.
- Implement follow-new-output behavior only while near the bottom and a Jump to
  latest affordance otherwise.
- Implement a functional Agent Tree and Agent selection/subscription path.
- Implement Composer:
  - per-Session draft;
  - target Agent selector;
  - model/thinking selectors after the Phase 0 contract gate;
  - multiline auto-grow, Enter submit, Shift+Enter newline;
  - separate Send and Stop controls;
  - restore rejected text without overwriting newer input.
- Project Turn state into the minimal collapsed Activity row.
- Keep Send available when host rules permit queueing; show Stop only for a
  cancellable selected-Agent Turn.
- Preserve per-Agent Timeline scroll state in the GUI.

### Exit criteria

- A real hostd path completes: open Session -> see transcript -> select Agent ->
  submit text -> see committed/realtime response -> stop an active Turn.
- Switching Agent changes Timeline, Composer target, Activity scope, and scroll
  state together.
- No user message or running state is inserted optimistically.
- Core and adapter tests cover submit rejection, queued submission, stop, and
  Agent switching.

## 9. Phase 5 — Activity, Tools, and Prompts

### Goal

Reach M2: make every blocking or actionable primary-path state operable.

### Work

- Complete Activity Center between Timeline and Composer:
  - Agent/tool running and failure;
  - queued work;
  - pending approval and interaction;
  - unread Agent reports;
  - transport and command warnings requiring action.
- Keep Activity bounded: one quiet row, capped expansion, internal overflow
  scroll, and no Composer/StatusBar displacement.
- Add tool cards with running, completed, failed, and expandable detail states.
- Add `PromptCoordinator` over GPUI Component Root layers:
  - approvals before interactions;
  - stable host order;
  - one interactive prompt at a time;
  - approval cannot close through Escape/backdrop;
  - response controls disable while pending;
  - host resolution/reconcile is the only removal authority;
  - focus returns to the prior control.
- Restore pending prompts after same-process refresh.

### Exit criteria

- The real hostd path completes an approval and a structured interaction.
- Reconcile during a prompt neither duplicates nor loses it.
- Activity, StatusBar, toast, Timeline, and local spinner responsibilities match
  the design ownership table.
- The complete M2 path is keyboard-operable.

## 10. Phase 6 — Desktop Workbench Structure

### Goal

Reach M3 by delivering the desktop-native layout improvement without making
sidebars mandatory.

### Work

- Finish the wide three-pane layout:
  - Session Sidebar;
  - center Timeline / Activity / Composer / StatusBar;
  - right Inspector with Agent Tree above Conversation Map.
- Persist outer split sizes, sidebar visibility, and reduced-motion preference
  under `[gui]`. Keep focus, map expansion, and per-Agent scroll/follow state
  window-local until their recovery semantics stabilize.
- Implement Conversation Map labels, active path, current leaf, preview
  selection, Timeline scroll-to-entry, and explicit Switch Branch action.
- Route any branch change requiring summary/confirmation through a focused
  workflow; tree selection alone never changes authority.
- Add responsive behavior:
  - wide: both outer sidebars visible;
  - medium: Inspector collapsed;
  - narrow: both sidebars represented by Sheets;
  - narrow Inspector: one tree at a time.
- Verify the center remains a complete single-column product when both sidebars
  are closed.

### Exit criteria

- At least one qualitative GUI improvement is complete: true overlay prompts
  plus the optional stacked Inspector.
- Every M2 operation works with both outer sidebars closed.
- Responsive collapse never persists over the user's wide-window preference.
- Conversation Map navigation passes active-path, off-path preview, confirm,
  reconcile, and surviving-expansion tests.

## 11. Phase 7 — UX and Rendering Hardening

### Goal

Turn the functional Workbench into a stable daily-use desktop client.

### Work

- Render committed assistant Markdown and code while keeping realtime rendering
  lightweight.
- Harden variable-height Timeline anchoring, entry expansion, detached scroll,
  and Agent switching. Adopt `VirtualList` only if measurement proves the
  initial Scrollable path insufficient.
- Finalize piko dark semantic tokens, selection/focus states, icons, contrast,
  reduced motion, and accessibility labels/announcements.
- Validate CJK/IME, selection, clipboard, keyboard traversal, screen resizing,
  and long Composer input.
- Complete StatusBar conditional cwd behavior and accurate context/cost scope.
- Add bounded notifications, error recovery, per-Session drafts, and safe
  application shutdown.
- Add Settings/Palette only for controls required by the first release; defer
  general feature parity.

### Exit criteria

- No streaming update moves a reader who is detached from the Timeline end.
- Main path works with keyboard only and with IME input.
- Minimum, medium, and wide layouts meet center width and panel-height rules.
- No active animation lacks reduced-motion or accessibility treatment.

## 12. Phase 8 — Regression and Release Gate

### Goal

Reach M4 with evidence that the GUI is a correct second hostd client.

### Work

- Run canonical scenarios against both GUI and unchanged TUI, comparing product
  outcomes rather than pixels or layout.
- Run failure scenarios: malformed JSONL, hostd exit, rejected commands, stale
  events, reconnect-style reconcile, prompt restoration, cancelled/queued Turn,
  and failed tool.
- Exercise large transcripts and rapid realtime deltas for memory, scroll, and
  render regressions.
- Verify exact dependency pins, license/package metadata, hostd discovery,
  process cleanup, macOS deployment target, and startup diagnostics.
- Write user-facing launch, support, known-limitations, and dependency-upgrade
  documentation.

### Required commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p piko-client-core
cargo test -p piko-gui
```

GPUI interaction and real-hostd smoke checks remain required in addition to
these commands because pure Rust tests cannot verify native focus, IME, pointer,
or window behavior.

### Release criteria

- Open/create -> reconcile -> transcript -> submit/stop -> approval/interaction
  passes with a real hostd process.
- The same path passes with both optional sidebars closed.
- GUI never talks to orchd and never owns durable Session state.
- TUI builds and its tests pass without source changes made for GUI.
- No moving GPUI dependency or unresolved first-release protocol gap remains.

## 13. Deferred Work

The following work requires a separate decision after M4:

- migrating TUI to Client Core;
- multiple windows or multiple simultaneous hostd clients for one Session;
- free-form Dock/Tiles layout;
- full auth/settings/skills/prompt-management parity;
- complete Session tree editing and import workflows;
- cross-platform release support;
- protocol or transport redesign unrelated to a proven GUI blocker.

## 14. Phase Change Checklist

Before declaring a phase complete:

1. its exit criteria are demonstrated;
2. new reducer/adapter behavior has deterministic tests;
3. host-authoritative state has no GUI-local competing copy;
4. new files respect the Rust file-size limits and crate boundaries;
5. documentation matches the implemented behavior;
6. unrelated TUI behavior and code remain untouched;
7. the next phase has no unresolved prerequisite disguised as polish.
