# GPUI Desktop Client Design

> Status: implemented through M3; M4 automated gates pass and native manual gates remain open
> Feature contract: [GUI Workbench](gui-workbench-feature.md)
> Runtime dependency: [Client Core Design](client-core-design.md)
> Phase 0 baseline: [Client Core Contract Baseline](client-core-contract-baseline.md)
> External references: [GPUI](https://gpui.rs/),
> [GPUI Component](https://longbridge.github.io/gpui-component/docs/components/)
> Visual system: [Piko GUI UI Guidelines](../packages/gui/docs/ui-guidelines.md)
> Chrome presentation: [Feature](gui-chrome-presentation-feature.md) ·
> [Design](gui-chrome-presentation-design.md)
> Chrome tool-window layout: [Design](gui-chrome-tool-window-layout-design.md)

## 1. Purpose

Define the crate boundaries, application structure, component choices, state
flow, and delivery sequence for a macOS-first GPUI desktop client. The GUI is a
second JSONL hostd client and is built on `piko-client-core`. The existing TUI
is a read-only behavior and product reference during this work.

## 2. Decisions

1. Add `packages/gui` as the `piko-gui` binary crate.
2. Add `packages/client-core` as the headless protocol projection and product
   operation crate defined by the Client Core design.
3. Keep hostd authoritative and keep orchd unreachable from GUI code.
4. Keep host process and JSONL I/O in the GUI adapter, outside Client Core.
5. Use official GPUI plus `gpui-component` as the desktop component layer.
6. Target macOS first; other GPUI platforms are compatibility follow-ups.
7. Use a responsive three-pane Workbench: Sessions, conversation, and a stacked
   Agents / Tree column. Activity lives in the Composer island; the center
   column remains independently complete.
8. Do not change `packages/tui` in the GUI-first phase.

## 3. Crate and Module Boundaries

```text
packages/client-core
├── state            authoritative client-side projection
├── timeline         structured committed/realtime projection
├── intent           frontend-neutral product operations
├── update           Msg → state + effects
└── tests            protocol conformance and projection cases

packages/gui
├── app              DesktopApp, actions, layout prefs, island dispatch/refresh
├── transport        hostd process and JSONL reader/writer
├── bridge           Client Core effects and GPUI foreground updates
├── chrome           Workbench layout, TitleBar, StatusBar, IslandPanel, IslandMsg
├── islands          Sessions, Timeline, Composer (+ Activity), Agents, Tree
├── overlays         approval, interaction, PromptCoordinator
├── projections      Client Core → island view models
├── config           `[gui]` schema and defaults
├── theme            tokens, typography, icons mapped onto GPUI Component
├── i18n / locales   English chrome catalog (`rust_i18n`)
├── assets           vendored Lucide-compatible SVG icons
└── cli              cwd/session/hostd launch options
```

`packages/gui` depends on `piko-client-core`, `piko-protocol`, `piko-comms`,
GPUI, and GPUI Component. It does not depend on TUI, hostd, or orchd libraries.
The hostd executable is a child process, as it is for the TUI.

Each Workbench island is an independent GPUI Entity. Cross-island actions use
directed `IslandMsg` routing through `DesktopApp`, not a full Workbench rebuild.

## 4. GPUI Application Structure

Application startup performs:

1. create the platform application with GUI assets;
2. initialize GPUI Component and force chrome locale `en`;
3. apply the piko dark theme;
4. register piko actions and GUI key bindings;
5. spawn hostd and start the stdout pump;
6. open the Workbench window wrapped in GPUI Component `Root`;
7. execute Client Core bootstrap effects;
8. deliver decoded host messages to Client Core on the GPUI foreground.

The root entity owns presentation entities and a Client Core bridge:

```text
Root
└── DesktopApp
    ├── ClientBridge
    │   ├── ClientState
    │   └── HostTransport handle
    ├── Workbench layout
    │   ├── Sessions island
    │   ├── Timeline island
    │   ├── Composer island (Activity Center + InputState)
    │   ├── Agents island
    │   └── Tree island
    ├── StatusBar
    └── PromptCoordinator
        └── maps pending core prompts to Root overlay layers
```

Client Core remains ordinary Rust state. GPUI entities notify and rerender after
the core applies a message. GPUI entity handles, focus handles, windows, and
contexts never enter Client Core.

## 5. Message and Effect Flow

```text
click / key / InputEvent
        │
        ▼
GUI Action ──► ClientIntent ──► Client Core update
                                      │
                                      ▼
                              Effect::Send(Command)
                                      │
                                      ▼
                                HostTransport
                                      │ JSONL
                                      ▼
                                    hostd
                                      │ ServerMessage
                                      ▼
foreground callback ──► Client Core update ──► notify views
```

Transport decode failure and child exit become explicit GUI transport messages.
They may update connection chrome but never fabricate Session state. Window
close stops and waits for the child process.

The first implementation may use a dedicated stdout reader thread because
hostd stdio is blocking. The thread crosses into GPUI through one declared
frontend bridge and performs no state mutation itself.

## 6. GPUI Component Adoption

GPUI Component provides useful primitives, but piko owns composition and
product behavior.

| Workbench need | Component choice | Decision |
|---|---|---|
| Window provider/layers | `Root` | Required first-level window child |
| Theme tokens | `Theme` / `ActiveTheme` | Map piko semantic tokens; do not style every view ad hoc |
| Composer | multi-line `InputState` + `Input` | Auto-grow input plus target/model/thinking action row |
| Timeline scrolling | `Scrollable` initially | Own bottom anchoring and detached-scroll behavior |
| Large Timeline | `VirtualList` later | Adopt only after variable-height measurement and streaming updates are proven |
| Approval | `AlertDialog` or non-closable `Dialog` | Disable backdrop/Escape close; resolution stays host-driven |
| User interaction | `Dialog` | Structured questions with explicit submit/cancel |
| Session navigation | `Sidebar` + list rows | Persistent left Sessions island on wide windows; Sheet on narrow windows |
| Agents | `Tree` with custom rows | Upper right island and primary Agent selector |
| Activity Center | Compact rows inside Composer island | Between Timeline and input; bounded, not an event log |
| Tree | Custom tree list | Lower right island for selected Agent; not a second Timeline |
| Settings/transient navigation | `Sheet` | Layer above the Workbench on demand |
| Pane sizing | `Resizable` | Persist outer widths; retain the right column Agents/Tree split for the window lifetime |
| Agent/tool state | `Badge`, `Spinner`, `Collapsible` | Compose into piko-specific rows/cards |
| Errors | `Notification` | Recoverable transport/command feedback |
| Shortcuts/help | `Kbd`, `Tooltip`, `Menu` | Display bound actions rather than hardcoded labels where possible |
| Assistant content | Markdown/text primitives | Committed Markdown; lightweight realtime rendering while streaming |

GPUI Component `Root` already provides dialog, sheet, and notification layers.
Piko uses those layers rather than creating an unrelated absolute-position
overlay system. Prompt ordering and protocol response semantics remain piko
responsibilities.

### 6.1 Components deliberately deferred

- Dock/Tiles: too much layout freedom before panel responsibilities stabilize.
- Full code editor: the Composer needs multi-line text, not LSP or line numbers.
- DataTable: not required for the first conversation path.
- Fully custom window controls: use GPUI's native-integrated transparent title
  bar and retain native macOS traffic lights, dragging, and double-click
  behavior.

## 7. Workbench Layout

Default window size is approximately 1360 × 840 points. The native window
minimum is locked to the single-column center budget: width is
`CENTER_MIN_WIDTH` (620) plus the left/right canvas gutters (8+8), height is
600. That keeps the window floor consistent with the center readable minimum
and prevents shrink-below-center overflow.

```text
DesktopApp
├── Native-integrated TitleBar
├── Window canvas + Horizontal Resizable
│   ├── Sessions island           default 300, minimum 220
│   ├── Center column             minimum readable width 620
│   │   ├── Timeline island
│   │   └── Composer island
│   │       ├── Activity Center   auto, collapsed, maximum 180
│   │       └── Composer input
│   └── Right column              default 340, minimum 260
│       └── Agents + Tree islands (8 px canvas gutter; vertical resize)
│           ├── Agents island     minimum 160
│           └── Tree island       minimum 180
└── Edge-to-edge StatusBar
```

The center Workbench never depends on a side panel. Responsive layout uses
available width against the center minimum (plus docked column minima and
gutters) rather than fixed pixel breakpoints as dock authority:

- when prefs and width allow, both outer sidebars dock;
- when both are preferred but space is tight, the right column collapses first,
  then the left;
- columns that cannot dock remain available through left/right Sheets via the
  TitleBar toggles or `cmd-b` / `cmd-i`.

TitleBar hosts Fleet-style left/right panel icon toggles. Pressed state tracks
effective dock visibility. Hidden panels are removed from the resizable group
rather than collapsed to unusable slivers. Manual visibility preferences win
until the window can no longer maintain the center minimum. Responsive
collapse does not overwrite the user's stored open preference.

The center has no persistent header and starts directly with Timeline.
Sessions owns Session/project context; StatusBar may show abbreviated cwd when
Sessions is hidden. The TitleBar shows the brand mark only. Agents is the
primary Agent selector; Composer repeats its target and owns model/thinking
controls because they affect the next submission. `StatusBar` is stable
environment telemetry. Activity Center lives in the Composer island, owns
operational/actionable state, and remains available when the right column is
hidden.

### 7.1 Sessions

Sessions is a navigation surface over global `SessionList` results
(`SessionListScope::All`), not a second source of Session authority. It
contains:

- Open Directory (island header): native folder picker; if the cwd is new,
  create a Session there immediately; if the cwd already appears in the list,
  show a notice and skip create;
- New Session on each directory group (create uses that group's cwd);
- all Sessions grouped by working directory, sorted alphabetically by path;
- current live Session, pending open target, and loading/error indicators.

`SessionCreated` upserts the new Session into Client Core's `session_list` so
the sidebar updates without waiting for another discover. Selecting a row
emits the Client Core open intent. The sidebar marks the target as pending,
while the center changes authority only through the normal reconcile path.
GUI drafts are stored by Session id when known, with a separate no-session
draft for first-submit creation.

On narrow windows the same view is hosted in a left Sheet. Opening a Session
closes the Sheet after successful reconciliation, not after identity-only open
success.

Header/tree trailing action alignment is chrome layout ownership; see
[Tool-Window Row Layout](gui-chrome-tool-window-layout-design.md).

### 7.2 Right column (Agents + Tree)

The right column stacks two independent islands: Agents above Tree. They share
column width and divide height through a resizable split separated by the same
8 px canvas gutter used horizontally. Their split position is window-local.
The column is hidden as a unit when responsive layout cannot preserve the
center minimum.

On narrow windows the right column opens as a Sheet that stacks Agents above
Tree with the same selection semantics.

### 7.3 Agents

Agents projects the reconciled AgentInstance parent/child hierarchy. GPUI
Component `Tree` supplies expansion and keyboard navigation, while piko owns
row content and selection behavior. Rows may show display name, role,
lifecycle, active Turn, queued work, unread activity, and failure indicators;
they do not expose transient internal runs as durable Agents.

Activating a row selects that Agent through Client Core. Selection changes the
Timeline, Composer target, selected-Agent Turn status, and Tree as one
projection. It never starts work. Expansion, tree focus, and scroll are
GUI-local; hierarchy, identity, and runtime state remain reconciled host data.

### 7.4 Tree

Tree projects the selected Agent's entries and current leaf into a compact
outline (closer to a symbol tree/minimap than a second Timeline). Piko owns
conversion from entries to tree nodes and all navigation semantics.

Map rows contain only structural information:

- entry label or compact first-line summary;
- message/tool/model-change kind;
- branch depth and connecting guide;
- current-leaf, active-path, and selected-preview states;
- tool/Turn status only when it materially aids navigation.

Expansion, preview selection, and sidebar scroll are GUI-local state and are
retained per Agent when practical. The current leaf and active path come only
from Client Core. Selecting another Agent replaces the visible map. Applying
reconcile rebuilds it, preserves expansion for surviving entry ids, selects the
authoritative leaf, and scrolls it into view when appropriate.

For an active-path entry, activation scrolls the Timeline to the corresponding
item. For an off-path entry, activation selects a preview and exposes an
explicit Switch Branch action. That action emits the existing Session navigate
intent and participates in its summarization/confirmation workflow; tree
selection alone never changes the authoritative branch.

### 7.5 Activity Center

Activity Center is a bounded projection of live operational state rendered
inside the Composer island, visually between Timeline and the input. This
placement keeps pending work adjacent to conversation context and the next
input, and keeps it available when either outer sidebar is hidden. It combines:

- host-authoritative Agent activity, active or failed tool state, queue counts,
  pending approvals, pending interactions, and unread inbox/report counts;
- current command failures and transport/connection warnings observed by the
  client;
- ownership links that can select the relevant Agent or restore the relevant
  prompt or Timeline location.

The component does not invent a durable event history. Host-derived items are
removed only by the corresponding host event or reconcile. GUI notifications
are bounded and may be dismissed locally. Ordinary informational success uses
GPUI Component notifications instead of occupying the panel.

The default collapsed row shows aggregate status. Actionable items expand the
panel up to its maximum height; overflow scrolls inside the panel. User collapse
is respected except that a newly arrived approval, interaction, or error raises
a visible badge. Expanded height is taken from Timeline; Composer input and
StatusBar retain their layout.

### 7.6 Information Ownership and StatusBar

Each recurring fact has one primary surface. Other surfaces may expose a
minimal fallback only when their primary surface is hidden or when the fact is
required to understand an action.

| Information | Primary surface | Allowed fallback |
|---|---|---|
| Session and project | Sessions | Abbreviated StatusBar cwd while Sessions is hidden |
| Full cwd | Sessions tooltip/detail | Abbreviated StatusBar item only while Sessions is hidden |
| Selected Agent | Agents | Composer target label |
| Model and thinking | Composer action row | None |
| Host connection | StatusBar | Connection warning in Activity Center when actionable |
| Turn, tool, queue, prompt, and Agent report state | Activity Center | Compact markers on owning Agent/Timeline rows |
| Context consumption and cumulative cost | StatusBar | Detailed status/settings view later |
| Short-lived command success | Toast | None |

`StatusBar` is always visible, single-line, read-only, and not focusable. Its
stable order is connection, optional cwd fallback, context usage, and cumulative
cost. Unknown values render as restrained unavailable values or are omitted;
they never use fabricated measurements. It contains no shortcut hints and does
not become a notification log.

## 8. Timeline Design

Client Core exposes structured Timeline items. `TimelineView` maps them to
piko-specific GPUI elements:

- message row/card;
- assistant Markdown blocks;
- thinking disclosure;
- tool call card with status and expandable details;
- Session/model/system notice;
- error notice;
- realtime assistant draft.

Initial rendering uses a vertical `Scrollable` container (via shared
`IslandPanel` viewport). This is simpler and safer for streaming content whose
height changes while text arrives. The view tracks:

- whether it is near the bottom (follow preference);
- the selected Agent's scroll handle/state;
- whether unseen content arrived while detached;
- the anchor required when an item above the viewport changes height.

When detached, `cmd-j` reattaches follow and scrolls to the end. There is no
in-panel Jump button. Virtualization is an optimization gate, not a first-slice
requirement. Adopt `VirtualList` only after tests prove correct variable-size
measurement, message expansion, Agent switching, and live-delta anchoring.

Committed assistant content may use GPUI Component Markdown. Realtime output is
rendered with a lightweight text path and re-rendered as committed Markdown
when the authoritative message arrives, avoiding a full Markdown parse on every
small delta.

## 9. Composer and Actions

The Composer owns one GPUI Component `InputState` configured for multi-line,
soft-wrapped, auto-growing input. It subscribes to change, Enter, focus, and blur
events. Its action row owns:

- selected Agent target, opening or focusing Agents when activated;
- model and thinking controls for the next submission;
- Send action for a new submission;
- a distinct Stop action only while the selected Agent has a cancellable Turn;
- command-local pending state.

Agent display name and model/thinking values come from Client Core projections.
The Composer does not keep an authoritative parallel copy. Removing the former
top toolbar does not remove access to these controls. The action row renders
separate controls, for example `Main ▾`, `Model ▾`, `Think ▾`, `Stop`, and
`Send`; it is not a sentence or a combined `Send/Cancel` control. Send remains
available when product rules permit queueing behind a running Turn.

Actions are semantic GPUI actions with frontend-specific bindings:

| Action | Default binding | Behavior |
|---|---|---|
| SubmitTurn | `Enter` in Composer | Send trimmed non-empty text |
| InsertNewline | `Shift+Enter` | Keep editing |
| CancelTurn | `Command+.` | Request selected Agent Turn cancellation |
| FocusComposer | `Command+L` | Restore Composer focus |
| CloseTransientOverlay | `Escape` | Close local overlays only; never resolve a host prompt |

The bridge snapshots submitted text with the pending command. On rejection it
restores that text only when the current Composer has not diverged, so newer
typing is not overwritten.

## 10. Prompt Coordination and Focus

Pending approval and interaction facts live in Client Core. Presentation uses
one `PromptCoordinator`:

1. approval prompts take priority over interactions;
2. prompts of the same kind preserve host order;
3. only the front prompt is interactive;
4. sending a response disables its controls but leaves it visible;
5. a resolved event or reconcile advances/removes the prompt;
6. closing a user interaction sends explicit cancellation;
7. approvals cannot close through Escape, backdrop click, or a window-local
   close button.

Dialogs trap focus. On resolution, focus returns to the previously focused
element, normally the Composer. A prompt restored by reconcile opens after the
new projection is installed, never during partial hydrate.

The coordinator treats GPUI Component's dialog layer as rendering machinery,
not as prompt authority. If the library supports only one active dialog, piko's
queue remains the source for opening the next dialog.

## 11. Session Entry and Loading

Without a requested Session, Sessions lists all Sessions globally (grouped by
cwd) with Open Directory while the center shows a lightweight welcome/empty
state. On a narrow window, the same Sessions view opens as the initial left
Sheet. After a Session is live, the persistent sidebar or Sheet can switch
Sessions without replacing the Workbench layout.

Client Core phases drive visible states:

| Core phase | GUI presentation |
|---|---|
| IdleNoSession | Session entry view; Composer may retain a draft |
| Opening/Creating | skeleton rows and disabled submit |
| Hydrating | empty/loading Timeline and pending Session/Agent rows |
| Live | authoritative Workbench projection |
| Operation error | notification and previous live projection when available |

`SessionOpened` and `SessionCreated` never unlock the live Workbench. Only the
matching `SessionReconciled` does.

### 11.1 Spinner Ownership

Animation is attached to the operation owner rather than to global application
state:

| Waiting condition | Spinner/skeleton location | Terminal authority |
|---|---|---|
| Host connect or reconnect | StatusBar connection item | transport state |
| Session open/create | pending Session row | correlated command result followed by reconcile |
| Initial hydrate with no live content | Timeline skeleton | matching `SessionReconciled` or `SessionCleared` |
| Agent running | owning Agents row and Activity item | reconciled Agent activity |
| Tool running | owning Timeline tool chip and Activity item | authoritative tool update |
| Submit awaiting result | Composer Send action | correlated command or Turn event |
| Prompt response awaiting resolution | active dialog action | resolved event or reconcile |

A spinner never changes the underlying phase and never disappears merely because
a timeout elapsed. When the previous live Session can remain visible during an
open attempt, it remains rendered; only the pending target row animates.

## 12. Theme and Visual Identity

Define a small piko semantic token layer mapped to GPUI Component Theme:

- canvas, surface, elevated surface;
- primary and muted foreground;
- border and focus ring;
- accent;
- success, warning, danger, and info;
- user, assistant, thinking, tool, and system accents.

The first theme is dark and should retain recognizable relationships to the TUI
without copying terminal geometry. Typography, whitespace, card depth, pointer
states, and animation are desktop-native. A light theme and external theme
directory support are later work.

Icons use an explicit, vendored subset of Lucide-compatible SVG assets. The GUI
does not depend on an implicit system-wide icon directory. Chrome typography
roles and localizable chrome copy are specified in
[GUI Chrome Presentation](gui-chrome-presentation-feature.md) and designed in
[GUI Chrome Presentation Design](gui-chrome-presentation-design.md).

## 13. Dependency Policy

GPUI and GPUI Component are pre-1.0. Phase 1 pinned them to exact crates.io
releases (`gpui = "=0.2.2"`, `gpui-component = "=0.5.1"`). Wildcard versions,
moving branches, and unpinned `main` remain forbidden.

Proven pins, toolchain, macOS deployment target, verified primitives, and the
upgrade procedure live in
[GPUI Dependency Pins](../packages/gui/docs/dependency-pins.md).

## 14. Validation

### 14.1 Client Core

Pure tests cover reconcile, scoping, transcript/realtime merge, Turn lifecycle,
prompt recovery, command correlation, and deterministic effects without GPUI.

### 14.2 GUI adapter

- transport line parsing and child exit;
- Action-to-Intent mapping;
- Composer submit/newline/rejection behavior;
- PromptCoordinator ordering and non-dismissable approval rules;
- phase-to-view-model mapping;
- per-Agent scroll-state selection.

### 14.3 GPUI interaction

Use GPUI test support where stable for focus and action dispatch. Manually
verify on macOS:

- resize from minimum to wide Workbench;
- trackpad and scrollbar behavior;
- text selection and clipboard;
- IME/CJK Composer input;
- streaming while attached to and detached from the bottom;
- restored approval after same-process refresh;
- dark-theme contrast and keyboard-only completion of the main path.

The unchanged TUI is used only for scenario comparison. GUI validation must not
require TUI source changes.

## 15. Delivery Status

M0–M3 are landed (pinned GPUI stack, Client Core Session kernel, shell/transport,
conversation path, Activity/tools/prompts, and desktop Workbench). Operator and
validation docs live under `packages/gui/docs/`:

- [`launch.md`](../packages/gui/docs/launch.md)
- [`known-limitations.md`](../packages/gui/docs/known-limitations.md)
- [`dependency-pins.md`](../packages/gui/docs/dependency-pins.md)
- [`ui-guidelines.md`](../packages/gui/docs/ui-guidelines.md)

Dock/Tiles, multiple windows, TUI migration, and complete feature parity remain
deferred.

## 16. Risks

| Risk | Mitigation |
|---|---|
| GPUI/component API churn | Exact revision lock plus isolated dependency spike |
| Component abstractions leak into core | GPUI types remain in GUI crate only |
| Timeline jank during streaming | Lightweight draft renderer; defer virtualization |
| Dialog lifecycle diverges from host prompts | PromptCoordinator remains host-event-driven |
| GUI and TUI semantics drift | Shared normative cases and scenario comparison |
| Three-pane layout crowds the conversation | Enforce center minimum and collapse outer sidebars responsively |
| Agents and Tree appear interchangeable | Distinct titles, row vocabulary, and selection effects; Agents answers who, Tree answers where |
| Right-column islands become too short | Keep Agents and Tree as separate islands with a resizable gutter; Sheet stacks both |
| Activity reduces Timeline height | Collapse quiet state to one row and cap expanded height; overflow scrolls internally |
| Activity duplicates StatusBar or toast content | StatusBar is ambient summary, Activity is inspectable/actionable state, and toasts are short-lived feedback |
