# TUI UI/UX Contract

> Status: draft (contract); P0 BottomBar usage projection implemented
>
> Source: Grok Build TUI UX (interaction vocabulary, density, status language) —
> adapted to piko layout rules (slots, LIFO focus, hostd-authoritative state).

## Overview

This PRD defines the **cross-cutting UI/UX contract** for the piko terminal
client: how surfaces look and feel when the user interacts with them, and
**what information** each surface is responsible for showing.

It is the parent contract for individual feature PRDs (Timeline, Editor,
BottomBar, tool workflow, sessions, and so on). Feature docs own detailed
layout and bindings for one surface; this doc owns consistency across them.

Goals:

1. **Interactive clarity** — every focusable surface has a predictable
   selection, confirm, cancel, and empty/loading/error language, inspired by
   Grok Build’s dense but scannable coding-agent TUI.
2. **Information completeness** — the user can answer, without leaving the
   chat shell: *what is running, for whom, with which model/context budget,
   what needs my decision, and what just failed or finished*.
3. **Command clarity** — every user-visible slash/palette command has a
   stable purpose, argument UX, and result presentation; missing *display*
   is fixed by consuming existing host capabilities before inventing new wire
   commands.
4. **Projection-only** — the TUI never invents durable state; it reflects
   hostd/protocol truth with graceful placeholders when data is unknown.

## Design principles

1. **Keyboard-first, mouse optional.** Full product use is possible without a
   mouse. When a terminal supports click/scroll, those map to the same actions
   as keyboard (select row, open, expand, scroll).
2. **One focus owner.** Focus is a stack (LIFO). Only the top owner receives
   navigation keys. Opening a panel pushes focus; closing pops it. No
   tab-roaming among unrelated chrome.
3. **Esc means step back, not “quit.”** Esc closes the current interactive
   layer, cancels a provisional UI step, or aborts the active turn according to
   a fixed priority table (below). It never exits the process.
4. **No floating chrome.** Every visible element sits in a layout slot or a
   defined overlay placement. No absolute-positioned popups that obscure
   arbitrary content without a placement rule.
5. **State over decoration.** Color and motion encode *state* (running,
   needs input, success, failure, idle, loading). Decorative animation without
   meaning is avoided.
6. **Density with hierarchy.** Primary facts stay one glance away (status row,
   agent strip, turn indicator). Secondary detail is collapsed by default and
   expands on demand (tools, thinking, long results).
7. **Semantic theming.** Surfaces use meaning tokens (accent, success, error,
   warning, muted, dim, border), not hard-coded “green/red.” Themes change
   paint, not structure.
8. **Authoritative projection.** Loading, empty, live, and error views must
   never present stale data from another session, agent, or turn as current.
9. **Grok Build affinity, piko identity.** Match Grok Build’s *interaction
   grammar* (list + filter, expand/collapse, footer hints, usage in chrome,
   state glyphs) where it helps. Keep piko’s slot layout, hostd authority, and
   multi-agent AgentPanel model.

## Shell layout (information zones)

Workspace + shell chrome (not A–E slots):

```text
┌──────────────────────────────────────────────────────────────┐
│ STREAM            conversation / tools (plane, grow)         │
├──────────────────────────────────────────────────────────────┤
│ DOCK (plane bottom stack)                                    │
│   Notice?         ephemeral warning/error                    │
│   Suggest?        completions when free composer             │
│   Composer        editor                                     │
├──────────────────────────────────────────────────────────────┤
│ CHROME            agent · model · cwd · context · cost       │
└──────────────────────────────────────────────────────────────┘

Z-modals (intent):
  Browse  CoverBody    — sessions, tree, diagnostics…
  Select  ComposerBand — agents, models, auth, MCP, processes…
  Dock    ComposerBand — approval, tool workflow
  Modal   Centered     — settings, status
```

Agents are not a permanent strip: F4 or `/agents` opens Select surface `Agents`
(ComposerBand) to switch the viewed session agent. Chrome shows a compact agent
chip only.

## Global interaction grammar

These patterns apply to **all** interactive components unless a feature PRD
explicitly narrows them.

### Focus and open/close

| Gesture | Meaning |
|---------|---------|
| Open overlay / panel | Push focus; capture navigation keys |
| `Esc` / cancel binding | Pop focus; discard uncommitted UI choice; do not destroy Editor draft unless the surface owns that draft |
| Confirm (`Enter` where defined) | Commit selection/action and close (or advance multi-step workflow) |
| Global quit | Explicit quit binding only (not Esc) |

### List surfaces (filterable lists, menus, selectors)

Shared behavior for model selector, session list, thinking
selector, settings lists, hierarchical menus:

| Interaction | Behavior |
|-------------|----------|
| Open | Show list; focus filter or first selectable row |
| Type | Live-filter by label and description (case-insensitive) |
| `↑` / `↓` | Move highlight; keep highlight in view |
| `PageUp` / `PageDown` | Jump by viewport |
| `Enter` | Confirm highlighted item |
| `Esc` | Close without applying |
| Empty filter result | Explicit “no matches” row; confirm is a no-op |
| Loading | Skeleton or single “loading…” row with quiet spinner; list is non-confirmable until data arrives |
| Error load | Error row or notification; allow retry entry if the surface defines one |

**Visual selection:** highlighted row uses accent (or accent background).
Current/active value (e.g. selected model) uses a distinct marker so “current”
and “highlighted” are not confused.

### Expand / collapse (content blocks)

Used by tool cards, thinking blocks, long results, and optional session
sections:

| Interaction | Behavior |
|-------------|----------|
| Expand / collapse selected or last tool | Toggle detail density |
| Expand/collapse all (optional binding) | Apply to all blocks of that class |
| Default | Collapsed for completed tools and long results; running tools may auto-expand a short live preview |
| Manual fold respect | Once the user collapses a block, live updates do not force it open until the user expands again (optional preference; default on for tools when implemented) |

### Streaming and scroll follow

| Condition | Behavior |
|-----------|----------|
| Viewport pinned to latest | New stream content keeps the view on the tail |
| User scrolled up | Do not steal scroll position; show a compact “new content below” hint |
| Jump to latest | One gesture returns to the tail and re-enables pin |
| Confirm replaces draft | Streaming draft is replaced in place by the authoritative final message; no duplicate rows |

### Escape priority (high → low)

1. Close ephemeral UI: completion menu, inline search, text selection if any.
2. Close top overlay / partial panel (palette, model, settings, status,
   session list, tree, workflow, approval when cancel is allowed).
3. While a turn is running: cancel the active turn for the viewed agent; keep
   the Editor draft.
4. Idle + non-empty Editor draft: first Esc is a soft arm (“press again to
   clear”); second Esc within a short window clears the draft (and may save
   it to history). Single Esc must not clear silently.
5. Idle + empty draft: no destructive action; optional future “rewind” is
   out of scope here unless a dedicated feature ships.

Ctrl+C (or the app clear binding) clears the draft in one step when that is
the bound clear action; it remains distinct from Esc cancel semantics.

### Footer / help hints

Interactive panels show a **single compact hint line** of currently valid
keys (e.g. `↑/↓ navigate · Enter confirm · Esc cancel`). Hints update when
the step changes (e.g. multi-question workflow). Full binding lists live in
the keybindings docs, not in every panel.

BottomBar does **not** show key hints (read-only status only).

### Running / progress language

| State | Presentation |
|-------|--------------|
| Working / streaming | Animated quiet spinner or phased glyph; warning/accent color for “in progress” |
| Needs user input | Distinct filled marker + warning/accent; panel or notification draws attention |
| Completed | Success token; static check or filled success glyph |
| Failed / cancelled | Error or muted token; short reason when available |
| Idle | Hollow or dim marker |
| Loading projection | Dim “loading…” with spinner; never show previous session’s content as if live |

Prefer **one** primary running indicator per context (turn strip or agent row),
not multiple competing spinners for the same work.

### Notifications

| Level | Where | Behavior |
|-------|-------|----------|
| Info | Transient; may appear in Status or as a brief toast-like row if implemented | Does not permanently steal layout height by default |
| Warning / Error | Notification row (zone C) | Visible until replaced or cleared; error uses error token |
| Turn rejection / command failure | Notification + optional Timeline system/error line | Must be readable after the ephemeral row scrolls away if durable |

## Information architecture

What each surface **must**, **should**, and **must not** show.

### Timeline (conversation)

**Must show**

- User prompts (after server accept), visually distinct from assistant text
- Assistant final text (and progressive draft while streaming)
- Tool executions as separate cards with name, status, short id, concise
  preview
- Session-level notices and errors that belong in the transcript
- Clear visual difference: user / assistant / tool / notice / error

**Should show**

- Thinking / reasoning as quieter nested content when enabled
- Collapsed vs expanded tool detail (args, result summary, parent linkage)
- Syntax-colored fenced code when language is known
- Per-agent conversation when an agent is selected (no cross-agent mix)

**Must not show**

- Unconfirmed local “ghost” prompts that duplicate server-committed rows
- Every partial stream delta as a new row
- Floating tool windows
- Partial tool-output streaming until protocol supports it (remain deferred)

Detail ownership: [timeline.md](./timeline.md), [turn-lifecycle.md](./turn-lifecycle.md),
[thinking.md](./thinking.md).

### Agent strip

**Must show**

- Agents in the active session, hierarchical when parent/child exists
- Which agent is **selected** (Timeline + next submit target)
- Coarse lifecycle/activity: loading, idle, running, needs input, failed
- Unread / report badges when the protocol provides them
- Queue pressure for the viewed context when non-zero (counts or short label)

**Should show**

- Quiet spinner on the agent that owns the active turn
- Tree connectors for spawn relationships
- Empty and loading rows that cannot be mistaken for a real agent

**Must not show**

- Agents from another session
- Invented activity when hostd has not projected state

Detail ownership: agent panel / agent-directed chat feature docs.

### Composer (Editor)

**Must show**

- Current draft text and caret
- Multi-line content with vertical scroll inside fixed height
- That the editor is the default focus when no overlay is open

**Should show**

- Slash-command and file/`@` completions in the suggestions zone
- History browse of recent submissions
- Placeholder when empty (optional, muted)

**Must not show**

- Model/cost/context (those belong in BottomBar or selectors)
- Approval or workflow questions (those replace the composer zone)

Detail ownership: [editor.md](./editor.md), [auto-completion.md](./auto-completion.md).

### BottomBar (status chrome)

Always visible, **non-interactive**, compact single row.

**Must show** (when data known; placeholders when not)

| Item | Content | Unknown |
|------|---------|---------|
| Model + thinking | Active model id and thinking level (omit level when `off`) | `—` |
| Working directory | Abbreviated cwd (`~`, left-truncate) | `—` |
| Context | `used/total` humanized tokens | `—/—` |
| Cost | Session cumulative cost in USD | `—` |

**Should show**

- Instant update when model, usage, or cwd projection changes
- User-configurable order/visibility of items

**Must not show**

- Keybinding help
- Interactive affordances (buttons, focus)
- Per-tool token breakdown (belongs in Status / future usage detail)

Detail ownership: [bottom-bar.md](./bottom-bar.md). Usage wiring is part of this
contract’s acceptance: placeholders are valid only while hostd has not provided
usage; once provided, BottomBar must display them.

### Notification row

**Must show** the most recent user-actionable warning or error when present.

Transient information may appear only when no actionable notice is pending.
The row is dismissible by mouse or the configured keyboard binding.

**Must not** permanently list full history. The bounded in-memory NoticeCenter
is an attention queue, not a durable event log; durable session facts belong in
typed Timeline components projected from hostd state.

### Approval panel

**Must show**

- What action needs permission (command/tool summary)
- Risk-relevant detail the host provides (cwd, command, scope)
- Explicit allow / deny (and any host-defined variants)
- Valid key hints

**Must not** look like a free-form chat question; it is a gate, not a
questionnaire. Shares interaction chrome with tool workflow where possible.

Detail ownership: tool-approvals (system) + TUI approval presentation.

### Tool interactive workflow

**Must show**

- Active question text
- Numbered/selectable choices
- Optional free-text for choices that allow it
- Multi-question tabs and a Submit step when confirmation is required
- Hint line for valid keys

**Must not** silently submit incomplete multi-question workflows.

Detail ownership: [tool-interactive-workflow.md](./tool-interactive-workflow.md).

### Session list / resume / tree

**Must show**

- Enough identity to choose a session: title/summary, recency, cwd or project
  hint when available
- Current vs other sessions clearly
- Loading and empty states

**Should show**

- State glyph language consistent with the agent strip (idle / active / failed)
- Filter/search when the list is long

**Must not** mutate session contents as a side effect of browsing.

Detail ownership: [resume-session.md](./resume-session.md),
[session-tree.md](./session-tree.md).

### Model selector / thinking selector / settings

**Must show**

- Filterable options with current value marked
- Short description lines where space allows
- Immediate preview where safe (e.g. theme preview) without committing until
  confirm—or commit-on-select if the feature PRD says so, but never silent
  discard of the previous value without Esc-cancel semantics

### Status / diagnostics panel

**Must show** (read-only snapshot)

- Session id (or none)
- Active turn id (or none)
- Queue summary (steer / follow-up / next-turn counts; previews when present)
- Tool tally (running / completed / failed)
- Pending approval count
- Notification count

**Should show** (when protocol + hostd already expose them)

- Prompt-debug / turn-diff entry points or last-fetched summaries via
  slash or status subsections (developer-oriented; not Timeline clutter)
- Cumulative usage mirrors (context, cost) consistent with BottomBar

**Must not** become a second chat log.

## Commands: content, results, and when to add new ones

User-facing “commands” are not the same thing as every host wire `Command`.
The UI/UX contract covers **what the user can invoke and what they see back**.
Wire protocol design stays in system/feature design docs.

### Two layers

| Layer | Owner | Purpose |
|-------|--------|---------|
| **Presentation commands** | TUI (local) | Open Settings, Tree, Sessions, Models, Thinking, Status, Notifications, Agents, diagnostics, and Quit. Never sent as host catalog ids. |
| **Host product commands** | hostd catalog + wire | Session/auth/runtime/model intents (`session.new`, `auth.login`, `session.compact`, `process.list`, …). Frontends map stable dotted ids to wire calls and local slash names. |

Rules:

1. **Slash names are TUI-local** (e.g. `/new` → `session.new`). Host catalog
   never ships slash strings or “open panel” semantics.
2. **Slash suggestions list the merged catalog**: local presentation rows plus
   host-advertised product rows (with TUI slash aliases when mapped).
3. **Every listed row must do something visible**: open a surface, send a host
   intent, or show a structured result. Dead rows are a defect.
4. **Do not add a new wire command** if session snapshot, push events, or an
   existing host command already carry the data (example: context/cost on
   BottomBar come from session/turn usage projection, not a new “get usage”
   command).

### User-visible command content (must/should)

Each slash entry **must** expose:

| Field | Rule |
|-------|------|
| **Title** | Short product name (“New session”, “MCP servers”) |
| **Detail** | One-line “what happens when I run this” |
| **Invoke shape** | Immediate / needs args / needs confirm (from host catalog when host-owned) |
| **Result surface** | Where output lands: notification, status line, dedicated panel, Timeline system line, or selector overlay |

**Argument UX** when invoke needs input:

- Prefer a **focused form or filterable picker** over silent failure.
- Slash text args are allowed for power users (`/rename …`, `/import …`) but
  must show a clear usage string when required args are missing.
- Confirm-class commands (`/delete`) require an explicit confirmation step;
  a bare slash must not destroy data.

### Current presentation commands (content contract)

| Slash (illustrative) | Opens / does | Result the user sees |
|----------------------|--------------|----------------------|
| `/resume` | Session list | Openable sessions; select opens session |
| `/tree` | Session tree | Branch navigation / labels as feature allows |
| `/models` | Model selector | Apply model; BottomBar model text updates |
| `/thinking` | Thinking selector (ComposerBand) | Apply level; BottomBar thinking text updates |
| `/settings` | Settings panel | Editable host-backed settings |
| `/status` | Centered Status modal | Session/turn/queue/tools/approvals snapshot |
| `/noti` | Centered Notifications modal | In-memory notices; Current/All title-affix scope |
| `/agents` | Session agents | Switch viewed agent; BottomBar agent chip updates |
| `/diff` | Shared Diagnostics surface (diff mode) | Last or active turn workspace diff |
| `/prompt-debug` | Shared Diagnostics surface (prompt mode) | Prompt assembly diagnostics |
| `/quit` | Exit | Process exits (not Esc) |

### Current host product commands (content + result)

| Host id | Typical slash | User-visible result |
|---------|---------------|---------------------|
| `session.new` | `/new` | New empty session; Timeline/Agent strip rehydrate |
| `session.fork` / `session.clone` | `/fork`, `/clone` | New session branch; view switches or status confirms |
| `session.rename` | `/rename` | Name updates in session list / title surfaces |
| `session.delete` | `/delete` | After confirm, session gone; empty or previous view |
| `session.import` | `/import` | Imported session openable |
| `auth.login` / `auth.logout` | `/login`, `/logout` | Login flow / signed-out confirmation |
| `session.compact` | `/compact` | Compaction progress/result via host events; Timeline keeps live rules |
| `process.list` / `process.stop` | `/top` | Selectable process table in ComposerBand; Enter arms stop, second Enter confirms |
| `mcp.status` | `/mcp` | ComposerBand with per-server state, counts, and errors |
| `model.set` / `thinking.set` | (via selectors or future slash) | BottomBar + settings reflect new defaults |

Background/internal wire commands (`ChatSubmit`, `TurnCancel`,
`ApprovalRespond`, `StateSnapshot`, `AgentList`/`Subscribe`, `ConfigGet`,
`ModelList`, `CommandCatalogGet`, …) are **not** slash rows. They power
surfaces above; the user meets them as chat, Esc cancel, approval panels, and
hydration—not as slash spam.

### Gaps: display first, new wire second

These host capabilities already exist on the wire or catalog path but lack a
complete user-visible result journey. **Priority is TUI consume + present**,
not new protocol variants.

| Capability | Wire today | UX gap | Preferred fix |
|------------|------------|--------|---------------|
| Context / cost | Session/turn usage projection | BottomBar still placeholder when data exists | Project into BottomBar (no new command) |
| Prompt debug | `PromptDebugGet` + result | **Landed:** `/prompt-debug` → diagnostics panel | Local presentation command |
| Turn diff | `TurnDiffGet` + push `TurnDiff` | **Landed:** `/diff` + cache last push/result | Local presentation command |
| Queue steer | `QueueSteer` | No first-class user entry | Local slash or Editor mode only if product wants steer; else keep protocol for automation |
| `model.set` / `thinking.set` in catalog | Catalog advertised | Selectors use other paths | Keep selectors; optional slash args later; no second set API |

### When to add a **new** host wire command

Add a new protocol command **only if** all of the following hold:

1. The user or client must **imperatively request** something hostd does not
   already push or expose via an existing command/result.
2. The result is **authoritative host state** (not pure presentation).
3. No existing command can be extended without breaking neutrality or
   semantics.
4. A Feature PRD (system or TUI) names the behavior and acceptance tests.

Examples that **do not** justify a new wire command:

- “Show usage in the footer” → project existing usage fields.
- “Open settings / status” → local presentation command.
- “Pretty-print prompt debug” → consume `PromptDebugGet`.

Examples that **would** justify a new wire command (future, not required by
this PRD’s P0):

- New durable product action with no host API yet (e.g. managed remote share).
- New query that cannot be derived from snapshot + existing getters.

### When to add a **new** presentation (local) command

Add a local slash row when:

1. The action is open/toggle/quit/navigate chrome, or
2. It is a thin launcher over an existing host getter (debug/diff/status
   subsections), and
3. Slash suggestions can describe it with title, detail, and result surface.

Local commands **must not** invent host authority (e.g. claiming to change
model without going through host config/model APIs).

### Command result presentation rules

| Result kind | Where it appears |
|-------------|------------------|
| Session lifecycle | View rehydrate + short status/notification on failure |
| Lists (sessions, models, agents, processes, MCP) | Dedicated overlay or structured panel—not a wall of Timeline chat |
| Destructive confirm | Confirm step before wire send |
| Diagnostics (status, prompt debug, diff) | Full or partial overlay; monospace/scroll for long text |
| Transient ack | Status line or info notification |
| Errors | Error notification with host message; keep draft/focus safe |

**Must not:** dump large diagnostic blobs into the durable Timeline as fake
assistant messages.

### Remaining command roadmap (UX only)

The current command inventory has a visible result path. Future work is a
product decision rather than a missing-surface repair:

- Queue steer user entry (only if steerable queue is a first-class UX)
- Richer `/status` subsections linking the above
- New host wire commands for net-new host capabilities

## Component-level interaction catalog

| Component | Focusable | Primary interactions | Feedback |
|-----------|-----------|----------------------|----------|
| Timeline viewport | Scroll only (no row focus required in v1) | Page/line scroll, jump latest; click a tool to toggle that block | Pin/unpin, new-content hint, tool disclosure hover |
| Agent strip | Yes when navigating agents | ↑/↓ select, Enter activate | Selection + active markers, spinner |
| Editor | Default | Type, history, submit, newline | Caret; submit clears accepted text only after accept path |
| Suggestions | Transient | ↑/↓, Tab/Enter complete, Esc dismiss | Highlighted candidate |
| Filterable list | Yes | Filter, navigate, confirm, cancel | Highlight + current marker |
| Hierarchical menu | Yes | Same as list + drill in/out | Breadcrumb or title |
| Confirm dialog | Yes | Confirm / cancel | Accent on default action |
| Form / login | Yes | Field edit, submit, cancel | Validation errors inline |
| Approval | Yes | Allow / deny / variants | Blocks turn progress until resolved |
| Interactive workflow | Yes | Choice, text, tabs, submit | Step progression |
| BottomBar | No | — | Live projection only |
| Notification row | No (v1) | — | Level color |

## Visual language (cross-cutting)

### Hierarchy

- **Primary text** — default text token
- **Secondary** — muted (metadata, descriptions)
- **Tertiary** — dim (placeholders, chrome separators `·`)
- **Accent** — selection, focus border, active agent
- **Success / error / warning / info** — outcomes and levels only

### Borders

- Composer and partial overlays: top border (and bottom when needed); focus
  uses accent border
- Full overlays: full frame allowed
- Timeline: no heavy box by default
- Agent strip: subtle separator from Timeline

### Motion

- Spinner only for live work or loading projection
- Frame rate stays modest; motion stops when state is idle
- No celebratory animation on success

### Empty, loading, error copy

| State | Copy tone |
|-------|-----------|
| Empty session | Short, calm: no fake history |
| Loading session/agents | “Loading…” with spinner |
| No filter matches | “No matches” |
| Unknown usage | `—` / `—/—` (never invent numbers) |
| Failed command | Specific host/protocol message when available |

## Consistency with Grok Build (reference mapping)

| Grok Build pattern | piko adoption |
|--------------------|---------------|
| Dense status + usage in chrome | BottomBar model/cwd/context/cost |
| List + live filter + footer hints | All selector overlays |
| Expand/collapse blocks | Tools, thinking |
| State glyphs + spinner for working | Agent strip + turn running |
| Esc layered step-back | Global Esc priority table |
| Scrollback pin vs manual scroll | Timeline follow behavior |
| Dashboard multi-agent roster | Out of scope as a separate product surface for now; Agent strip + session list cover session-local multi-agent |
| Vim vs simple scrollback modes | Optional later; default remains simple arrows + PageUp/Down |

piko does **not** require 1:1 key parity with Grok Build. Keybindings remain
owned by the keybindings feature and user config.

## Configuration

This contract introduces no mandatory new settings by itself. It constrains
how existing and future settings surface:

| Area | Expectation |
|------|-------------|
| `tui.bottomBar.items` | Order/visibility of status items |
| Theme tokens | All state colors |
| Keybindings | Actions named in feature PRDs; grammar above stays stable |
| Timeline presentation | Thinking visibility; tool expansion stays per session and per block |

## Acceptance criteria

1. A user can run a full turn (submit → stream → tools → complete) and always
   see: running state, final assistant text, tool outcomes, and model/context
   once hostd provides usage.
2. Opening any selector, completing or cancelling it, returns focus predictably
   with the Editor draft preserved.
3. Esc never quits the app; double-clear of draft requires an armed second press
   (or the explicit clear binding).
4. Switching session or agent never flashes another entity’s transcript as
   current; loading/empty states are explicit.
5. Approval and tool workflow are visually related (same interaction grammar)
   but copy and outcomes remain distinct.
6. BottomBar never shows fabricated context or cost; placeholders until real
   projection arrives, then live values.
7. Every slash/palette row either opens a surface, runs a host intent, or shows
   a structured result; the palette lists the same merged set the user can run.
8. Diagnostic host results (prompt debug, turn diff) are never silently
   discarded once their presentation commands ship; they do not pollute Timeline
   as fake chat.
9. Feature PRDs that contradict this document are updated or explicitly
   exception-listed under Non-goals / deltas.

## Non-goals

- Desktop-client layout and interactions
- Redesigning hostd/orchd protocols solely for aesthetics
- Defining every wire `Command` field or serialization shape (protocol design)
- Pixel-perfect clone of Grok Build, including dashboard, vim mode, or mouse
  affordances not listed above
- Plugins/hooks UI before runtime consumers exist
- Partial tool-output streaming without protocol events
- Replacing per-feature PRDs; this document does not restate every keybinding
- Accessibility beyond terminal norms (screen-reader tree) in the first cut

## Related documents

- [component-feedback.md](./component-feedback.md) — base component visual & interaction feedback, design principles
- [bottom-bar.md](./bottom-bar.md)
- [timeline.md](./timeline.md)
- [editor.md](./editor.md)
- [keybindings.md](./keybindings.md)
- [turn-lifecycle.md](./turn-lifecycle.md)
- [tool-interactive-workflow.md](./tool-interactive-workflow.md)
- [session-view-lifecycle.md](./session-view-lifecycle.md)
- [themes.md](./themes.md)
- [agent-directed-chat.md](./agent-directed-chat.md)

Implementation designs for layout/focus remain under `packages/tui/docs/design/`
and must not weaken the information and interaction rules above.
