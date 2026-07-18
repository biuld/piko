# GPUI Desktop Client Design

> Status: implemented through M3; M4 automated gates pass and native manual gates remain open
> Feature contract: [GUI Workbench](gui-workbench-feature.md)
> Runtime dependency: [Client Core Design](client-core-design.md)
> Phase 0 baseline: [Client Core Contract Baseline](client-core-contract-baseline.md)
> Execution plan: [GPUI Desktop Client Implementation Plan](gui-implementation-plan.md)
> External references: [GPUI](https://gpui.rs/),
> [GPUI Component](https://longbridge.github.io/gpui-component/docs/components/)
> Visual system: [Piko GUI UI Guidelines](../packages/gui/docs/ui-guidelines.md)
> Chrome presentation (draft): [Feature](gui-chrome-presentation-feature.md) ·
> [Design](gui-chrome-presentation-design.md)

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
   Agent/Conversation Inspector. Activity remains in the center workflow; the
   center pane remains independently complete.
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
├── app              GPUI startup, window, actions, key bindings
├── transport        hostd process and JSONL reader/writer
├── bridge           Client Core effects and GPUI foreground updates
├── workbench        Timeline, Activity Center, Composer, status
├── session_sidebar  Session discovery, search, create, and open
├── inspector        right-side Agent Tree and Conversation Map composition
├── agent_tree       AgentInstance hierarchy and selection
├── activity_center  bounded live status and attention items
├── conversation_map selected Agent branch/tree navigation
├── overlays         approval, interaction, Session selection
├── components       piko-specific visual components only
├── config           `[gui]` schema and defaults
├── theme            piko tokens mapped onto GPUI Component theme
└── cli              cwd/session/hostd launch options
```

`packages/gui` depends on `piko-client-core`, `piko-protocol`, `piko-comms`,
GPUI, and GPUI Component. It does not depend on TUI, hostd, or orchd libraries.
The hostd executable is a child process, as it is for the TUI.

## 4. GPUI Application Structure

Application startup performs:

1. create the platform application;
2. initialize GPUI Component;
3. register piko actions and GUI key bindings;
4. spawn hostd and start the stdout pump;
5. open the Workbench window;
6. wrap the root Workbench entity in GPUI Component `Root`;
7. execute Client Core bootstrap effects;
8. deliver decoded host messages to Client Core on the GPUI foreground.

The root entity owns presentation entities and a Client Core bridge:

```text
Root
└── DesktopApp
    ├── ClientBridge
    │   ├── ClientState
    │   └── HostTransport handle
    ├── WorkbenchView
    │   ├── TimelineView
    │   ├── ActivityCenter
    │   ├── Composer InputState
    │   ├── StatusBar
    │   └── local scroll/focus/layout state
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
| Session navigation | `Sidebar` + `List` | Persistent left pane on wide windows; Sheet on narrow windows |
| Agent Tree | `Tree` with custom rows | Upper Inspector panel and primary Agent selector |
| Activity Center | `Badge`, `Collapsible`, compact rows | Between Timeline and Composer; bounded, not an event log |
| Conversation Map | `Tree` with custom rows | Lower Inspector panel for selected Agent; not a second Timeline |
| Settings/transient navigation | `Sheet` | Layer above the Workbench on demand |
| Pane sizing | `Resizable` | Persist outer widths; retain the right Inspector vertical split for the window lifetime |
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

Default window size is approximately 1360 × 840 points with a practical minimum
near 760 × 600. Exact platform bounds are validated during the GPUI spike.

```text
DesktopApp
├── Native-integrated TitleBar
├── Window canvas + Horizontal Resizable
│   ├── SessionSidebar island     default 300, minimum 220
│   ├── CenterWorkbench island    minimum readable width 620
│   │   ├── TimelineViewport
│   │   ├── ActivityCenter        auto, collapsed, maximum 180
│   │   └── ComposerCard
│   └── Inspector column          default 340, minimum 260
│       └── Vertical Resizable
│           ├── AgentTree island         minimum 160
│           └── ConversationMap island  minimum 180
└── Edge-to-edge StatusBar
```

The center Workbench never depends on a side panel. Responsive layout uses
available center width rather than treating window width alone as authority:

- wide: both outer sidebars visible by default;
- medium: Session sidebar visible, Inspector collapsed;
- narrow: both collapsed and available through left/right Sheets.

Hidden panels are removed from the resizable group rather than collapsed to
unusable slivers. Manual visibility choices win until the window can no longer
maintain the center minimum. Responsive collapse does not overwrite the user's
stored wide-window preference.

The center has no persistent header and starts directly with Timeline.
`SessionSidebar` and the native window title own Session/project context.
`AgentTree` is the primary Agent selector; `ComposerCard` repeats its target and
owns model/thinking controls because they affect the next submission.
`StatusBar` is stable environment telemetry. Activity Center owns operational
and actionable state and remains available when Inspector is hidden.

### 7.1 Session Sidebar

The Session sidebar is a navigation surface over `SessionList` results, not a
second source of Session authority. It contains:

- a compact search field;
- New Session;
- current-directory Sessions in host-provided order;
- current live Session, pending open target, and loading/error indicators;
- later groups for recent folders or imported Sessions.

Selecting a row emits the Client Core open intent. The sidebar marks the target
as pending, while the center changes authority only through the normal
reconcile path. GUI drafts are stored by Session id when known, with a separate
no-session draft for first-submit creation.

On narrow windows the same view is hosted in a left Sheet. Opening a Session
closes the Sheet after successful reconciliation, not after identity-only open
success.

### 7.2 Inspector Sidebar

The right sidebar is one Inspector surface with two vertically stacked panels.
Agent Tree and Conversation Map divide its height through a resizable split.
Their split position is window-local. The Inspector is hidden as a unit when
responsive layout cannot preserve the center minimum; the narrow Sheet switches
between the two panels explicitly.

On narrow windows the Inspector opens as a right Sheet. It presents Agent Tree
or Conversation Map one at a time with an explicit panel switcher, preserving
the same selection semantics without squeezing both trees into short regions.

### 7.3 Agent Tree

The Agent Tree projects the reconciled AgentInstance parent/child hierarchy.
GPUI Component `Tree` supplies expansion and keyboard navigation, while piko
owns row content and selection behavior. Rows may show display name, role,
lifecycle, active Turn, queued work, unread activity, and failure indicators;
they do not expose transient internal runs as durable Agents.

Activating a row selects that Agent through Client Core. Selection changes the
Timeline, Composer target, selected-Agent Turn status, and Conversation Map as
one projection. It never starts work. Expansion, tree focus, and scroll are
GUI-local; hierarchy, identity, and runtime state remain reconciled host data.

### 7.4 Conversation Map

The Conversation Map projects the selected Agent's entries and current leaf
into a compact outline. GPUI Component `Tree` is a suitable base because it
supports expansion, keyboard navigation, custom item rows, programmatic
selection, and scroll-to-item behavior. Piko owns the conversion from entries
to tree nodes and all navigation semantics.

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

Activity Center is a bounded projection of live operational state between the
Timeline and Composer. This placement keeps pending work adjacent to both its
conversation context and the user's next input, and keeps it available when
either outer sidebar is hidden. It combines:

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

The default collapsed row shows aggregate badges. Actionable items expand the
panel up to its maximum height; overflow scrolls inside the panel. User collapse
is respected except that a newly arrived approval, interaction, or error raises
a visible badge and accessibility announcement. Expanded height is taken only
from `TimelineViewport`; `ComposerCard` and `StatusBar` retain their layout.

### 7.6 Information Ownership and StatusBar

Each recurring fact has one primary surface. Other surfaces may expose a
minimal fallback only when their primary surface is hidden or when the fact is
required to understand an action.

| Information | Primary surface | Allowed fallback |
|---|---|---|
| Session and project | Session Sidebar | Native window title |
| Full cwd | Session Sidebar tooltip/detail | Abbreviated StatusBar item only while Session Sidebar is hidden |
| Selected Agent | Agent Tree | Composer target label |
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

Initial rendering uses a vertical `Scrollable` container. This is simpler and
safer for streaming content whose height changes while text arrives. The view
tracks:

- whether it is near the bottom;
- the selected Agent's scroll handle/state;
- whether unseen content arrived;
- the anchor required when an item above the viewport changes height.

Virtualization is an optimization gate, not a first-slice requirement. Adopt
`VirtualList` only after tests prove correct variable-size measurement, message
expansion, Agent switching, and live-delta anchoring. The Client Core projection
may retain a bounded live view while hostd remains the durable authority.

Committed assistant content may use GPUI Component Markdown. Realtime output is
rendered with a lightweight text path and re-rendered as committed Markdown
when the authoritative message arrives, avoiding a full Markdown parse on every
small delta.

## 9. Composer and Actions

The Composer owns one GPUI Component `InputState` configured for multi-line,
soft-wrapped, auto-growing input. It subscribes to change, Enter, focus, and blur
events. Its action row owns:

- selected Agent target, opening or focusing Agent Tree when activated;
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

Without a requested Session, the left sidebar shows current-directory Sessions
and New Session while the center shows a lightweight welcome/empty state. On a
narrow window, the same Session view opens as the initial left Sheet. After a
Session is live, the persistent sidebar or Sheet can switch Sessions without
replacing the Workbench layout.

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
| Agent running | owning Agent Tree row and Activity item | reconciled Agent activity |
| Tool running | owning Timeline tool card and Activity item | authoritative tool update |
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

GPUI and GPUI Component are pre-1.0 and their current documentation follows git
`main`. Before implementation, create a dependency spike that compiles:

1. `gpui_platform::application()` on macOS with `font-kit`;
2. GPUI Component `init` and `Root`;
3. auto-growing Input and Enter/Shift+Enter events;
4. Scrollable content;
5. non-closable AlertDialog;
6. Sheet and Resizable primitives.

After the spike, pin all three repositories to exact compatible revisions:

- `gpui` and `gpui_platform` use the same Zed revision;
- `gpui-component` uses one exact revision or release tag;
- the selected GPUI revision matches the component revision's lock/dependency
  expectation;
- wildcard versions, moving branches, and unpinned `main` are forbidden.

The current GPUI Component release may be used as a baseline candidate, but the
revision is chosen from the successful compatibility spike, not from the latest
label alone. Record the revisions, Rust toolchain, macOS deployment target, and
upgrade procedure in the GUI crate documentation.

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

## 15. Delivery Slices

The normative phase gates, deliverables, and validation steps live in the
[GPUI Desktop Client Implementation Plan](gui-implementation-plan.md). Its
milestone sequence is:

1. contract baseline and GPUI compatibility spike;
2. Client Core Session kernel;
3. GUI shell, transport, and Session entry;
4. conversation vertical slice;
5. Activity, tools, and prompts;
6. desktop Workbench structure;
7. UX/rendering hardening;
8. regression and release gate.

GUI crate operator docs for M4 live under `packages/gui/docs/` (`launch.md`,
`known-limitations.md`, `support.md`, `release-checklist.md`,
`dependency-pins.md`, `manual-ux-checklist.md`).

Every phase keeps the single-column Workbench path operational. Dock/Tiles,
multiple windows, TUI migration, and complete feature parity remain deferred.

## 16. Risks

| Risk | Mitigation |
|---|---|
| GPUI/component API churn | Exact revision lock plus isolated dependency spike |
| Component abstractions leak into core | GPUI types remain in GUI crate only |
| Timeline jank during streaming | Lightweight draft renderer; defer virtualization |
| Dialog lifecycle diverges from host prompts | PromptCoordinator remains host-event-driven |
| GUI and TUI semantics drift | Shared normative cases and scenario comparison |
| Three-pane layout crowds the conversation | Enforce center minimum and collapse outer sidebars responsively |
| Two Inspector trees appear interchangeable | Distinct titles, row vocabulary, and selection effects; Agent Tree answers who, Conversation Map answers where |
| Inspector trees become too short | Use the resizable vertical split and one-at-a-time trees in the narrow Sheet |
| Activity reduces Timeline height | Collapse quiet state to one row and cap expanded height; overflow scrolls internally |
| Activity duplicates StatusBar or toast content | StatusBar is ambient summary, Activity is inspectable/actionable state, and toasts are short-lived feedback |
