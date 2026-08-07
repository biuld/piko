# Component Visual & Interaction Feedback

> Status: draft (contract); base-component feedback landed for List, menu,
> table, workflow, suggestions, agent strip, help/status frames
>
> Parent: [ui-ux.md](./ui-ux.md) (shell information architecture & product UX)
> Paint system: [themes.md](./themes.md) (semantic color tokens)
>
> Source: Grok Build TUI density and state language — adapted to piko slots,
> LIFO focus, and semantic theming.

## Overview

This PRD defines the **design principles and feedback contract for TUI base
components**: how every interactive and non-interactive building block looks
in each state, and how the UI responds when the user acts.

It sits **below** surface feature PRDs (Timeline, Editor, BottomBar, session
list) and **beside** the shell UI/UX contract:

| Document | Owns |
|----------|------|
| [ui-ux.md](./ui-ux.md) | Shell zones, information architecture, commands, Esc priority |
| **This PRD** | Reusable component states, visual feedback, interaction feedback |
| [themes.md](./themes.md) | Token names and theme files (not interaction timing) |
| Surface feature PRDs | Product-specific copy, layout, and bindings |

**Base components** (logical, not implementation names):

- **List** — filterable, keyboard-navigable rows (selectors, palette, sessions)
- **Hierarchical menu** — nested list with drill-in / drill-out
- **Text field** — single- or multi-line editable text (editor, forms, rename)
- **Table** — columnar read/scan lists (processes, diagnostics)
- **Choice workflow** — numbered options + optional free text + multi-step tabs
- **Confirm step** — destructive or explicit-commit decision
- **Read-only body** — help, status, long diagnostic text
- **Chrome chromelets** — borders, separators, hint lines, spinners, carets,
  state glyphs, notification chips

Product panels **compose** these primitives; they must not invent a private
selection or focus language that contradicts this document.

## Design principles

### 1. Feedback is meaning, not ornament

Every visual change answers a user question:

- Where is focus?
- What is selected vs what is already active/current?
- Can I act now, or is the surface waiting / disabled?
- Did my last action succeed, fail, or need another step?

If a color, glyph, or motion does not encode one of these, it is decoration and
must be removed.

### 2. One language, many surfaces

The same states use the same tokens and glyphs everywhere:

- **Selected** always reads as “keyboard highlight for the next Enter”
- **Active / current** always reads as “this is already applied or in force”
- **Focused frame** always uses accent border (or equivalent frame cue)
- **Error / warning / success / info** always map to the same semantic tokens

A session row, a model row, and a workflow choice must feel like the same
family of list.

### 3. Selected ≠ Active ≠ Focused

These three concepts must remain visually distinct:

| Concept | Meaning | Typical cue |
|---------|---------|-------------|
| **Focused** | This surface owns the keyboard | Accent border / frame; caret in text fields |
| **Selected** | Highlighted row/item within the focused surface | Accent text and/or selected background; `❯` caret |
| **Active / current** | Authoritative “in use” value (model, agent, session) | Secondary marker (e.g. `●`, check, muted “current” label) without stealing the selection style |

Never use only one style for both “highlighted” and “already selected value.”

### 4. Instant local feedback; patient host feedback

| Layer | Expectation |
|-------|-------------|
| **Local interaction** (move selection, type, open/close, expand) | Feedback on the **next frame** after the key is handled |
| **Host projection** (submit, load session, list models) | Explicit **loading** state until authority arrives; never fake success |
| **Unknown data** | Placeholders (`—`, muted copy), never invented numbers or labels |

### 5. Density with readable hierarchy

Terminal space is scarce. Prefer:

1. **Primary** — title / main label (`text` or `accent` when selected)
2. **Secondary** — one detail line or trailing metadata (`muted` / `dim`)
3. **Tertiary** — hints, ids, counts (`dim`)

Do not put three competing accents on one row. One accent ownership per row.

### 6. Motion is sparse and stateful

Allowed motion:

- Quiet **spinner** for working / loading
- Optional **phase glyphs** for long-running work
- Caret blink only if the terminal/default editor behavior already does

Forbidden:

- Continuous decorative animation
- Success confetti / flash storms
- Spinning entire panels

Motion **stops** when the encoded state ends.

### 7. Keyboard is the product; pointer is optional

All feedback must be complete without mouse. If pointer is supported later:

- Hover may preview selection **only** when the surface is already focused
- Click maps to the same action as the keyboard confirm for that target
- Hover never becomes the only way to discover an affordance

### 8. Semantic paint only

Components consume theme tokens (see [themes.md](./themes.md)):

| Intent | Tokens (primary) |
|--------|------------------|
| Body | `text` |
| Metadata | `muted`, `dim` |
| Selection / focus | `accent`, `borderAccent`, optional `selectedBg` |
| Success | `success`, optional `toolSuccessBg` |
| Warning / in progress | `warning`, optional `toolPendingBg` |
| Error | `error`, optional `toolErrorBg` |
| Quiet chrome | `border`, `borderMuted` |
| Info notice | `info` |

No hard-coded “green means go” outside tokens. Themes re-skin without structure
changes.

### 9. Fail closed on empty and error

Empty, loading, and error are **first-class layouts**, not blank holes:

- Empty → short calm copy (“No matches”, “No sessions”)
- Loading → spinner + short label; non-confirmable
- Error → error token + host message; recover path when defined

Confirm must be a no-op on empty/error rows unless the row is an explicit
retry action.

### 10. Compose, don’t fork

New panels must reuse List / Text field / Workflow / Confirm feedback rules.
A one-off “custom selected color only in Settings” is a defect against this PRD.

## Visual state model

Every interactive component supports a subset of these **visual states**.

### Frame / surface states

| State | When | Visual |
|-------|------|--------|
| Rest | Visible, not focus owner | `border` or `borderMuted`; body at normal contrast |
| Focused | Top of focus stack | `borderAccent` (or top-border accent for partial overlays) |
| Passive chrome | Never focusable (BottomBar, separators) | No accent border; muted/dim text only |
| Blocking | Awaits user decision (approval, workflow) | Focused frame + warning accent on title/prompt as appropriate |

### Item / control states

| State | Visual feedback |
|-------|-----------------|
| Default | Primary `text`, detail `dim`/`muted` |
| Selected | Leading `❯` (or equivalent), primary in `accent`, optional `selectedBg` |
| Active / current | Distinct marker (`●`, `✓`, or trailing muted “current”) **without** full selected style when not selected |
| Selected + active | Both cues may combine; selected still wins contrast |
| Disabled | `dim`; not selectable; confirm ignored |
| Hover (optional) | Same as soft selection preview; must not outrank keyboard selection |
| Loading row | Spinner + dim label; not confirmable |
| Error row | `error` text; optional retry as separate confirmable action |
| Empty list | Single non-selectable empty message |

### Content outcome states (cards, tools, notices)

| State | Visual |
|-------|--------|
| Pending / running | `warning` + spinner; optional pending background |
| Success / completed | `success`; static check or filled success glyph |
| Failed | `error`; short reason when available |
| Cancelled | `muted`/`dim`; not styled as success |
| Info / system | `info` or `accentAlt` for session/system lines |

### Text field states

| State | Visual |
|-------|--------|
| Blurred | Muted border; no caret emphasis |
| Focused | Accent border; terminal caret at insertion point |
| Placeholder | `dim` placeholder; disappears on first character |
| Invalid (forms) | `error` border or inline error line under field |
| Read-only display | No caret; body uses normal text tokens |

## Interaction feedback model

### Navigation

| User action | Immediate feedback |
|-------------|-------------------|
| Move selection ↑/↓ | Selection highlight moves; previously selected row returns to default/active style; ensure selected row stays in viewport |
| PageUp / PageDown | Jump by viewport; selection tracks policy of the surface (list: move selection; scroll-only: viewport moves) |
| Type to filter | List filters live; selection clamps to first match or stays on previous match if still visible; empty → “No matches” |
| Drill into hierarchy | Title/breadcrumb updates; list replaces with children; selection resets to first or remembered child |
| Drill out / Esc one level | Restore parent list and prior selection when possible |

### Activation

| User action | Immediate feedback |
|-------------|-------------------|
| Confirm (Enter) | Selected row commits: panel closes **or** advances step; host send only after local validation |
| Cancel (Esc) | Pop focus; discard uncommitted UI choice; preserve editor draft unless this surface owns the draft |
| Soft arm (double Esc clear) | First press: transient hint (“press again to clear”); second within window: clear |
| Toggle expand/collapse | Chevron/marker flips; body density changes in place; no list jump if avoidable |

### Continuous input

| User action | Immediate feedback |
|-------------|-------------------|
| Type character | Glyph appears at caret; completions may open |
| Delete / kill word | Text updates immediately |
| History browse | Field content swaps to history entry; live draft restored when leaving history |
| Submit text | Field clears only on **accepted** submit path; rejection restores or keeps draft per editor feature PRD |

### Async / host-bound

| Phase | Feedback |
|-------|----------|
| Request sent | Optional brief status; keep surface usable unless blocking |
| Waiting for list/data | Loading row or panel-level spinner |
| Success with data | Replace loading with content; select sensible default row |
| Success with side effect | Close overlay if that was the confirm contract; update chrome (e.g. BottomBar model) |
| Failure | Error notification and/or inline error; focus remains safe; draft preserved |

### Hint line feedback

Interactive panels show **one** footer hint line of *currently valid* keys:

```text
↑/↓ navigate · Enter confirm · Esc cancel
```

Rules:

- Hints update when the step changes (e.g. workflow Submit tab)
- Hints use `dim` / `muted`; never compete with selection accent
- BottomBar and other passive chrome **do not** carry key hints
- Full binding dumps live in Help, not on every component

## Component catalog (feedback contracts)

### List (filterable)

**Visual**

```text
  Current model          ← active marker (e.g. ●) without full selection style
❯ Other model            ← selected: ❯ + accent
  Third model            ← rest
  short detail…          ← dim on same or secondary line
```

**Interaction**

- Live filter, clamp selection, page navigation
- Confirm applies and typically closes
- Empty / loading / error rows as above

**Must not**

- Scroll selection out of view without following it
- Confirm a disabled or empty placeholder row

### Hierarchical menu

Same as List, plus:

- Group titles may be non-selectable or expandable section headers
- Enter on group drills in; Esc/left drills out (bindings per keybindings feature)
- Breadcrumb or panel title always shows depth context

### Text field

**Visual**

- Focused: accent frame + caret
- Multi-line: fixed or grow-within-cap height; overflow scrolls internally

**Interaction**

- All edits reflect immediately
- Completions appear in the suggestions zone without stealing frame focus
  ownership rules defined by input routing (completion captures nav keys while open)

### Table

**Visual**

- Header row `muted`/`dim`
- Selected data row same selection language as List
- Columns align; overflow elides with middle or end ellipsis consistently

**Interaction**

- Row navigation like List
- Horizontal scroll is non-goal unless a surface PRD explicitly adds it

### Choice workflow & confirm

**Visual**

- Top (or focused) border accent when panel is focus owner
- Choice rows numbered; selected uses `❯` + accent
- Multi-question tabs: active tab accent, inactive muted
- Confirm step: short summary + single primary action emphasis

**Interaction**

- Enter selects / advances; explicit Submit when required
- Esc cancels whole workflow when cancel is allowed
- Incomplete multi-question must not silent-submit

Shared shape for approval gates and tool questions; **copy and outcomes** stay
feature-specific (allow/deny vs answer questionnaire).

### Read-only body (help, status, diagnostics)

**Visual**

- Key labels may use `accent`; values `text`
- Long content scrolls; no fake “selected row” unless the surface is navigable

**Interaction**

- Esc closes
- Optional ↑/↓ or Page scroll without implying row activation

### State glyphs & spinners

| Glyph family | Use |
|--------------|-----|
| Spinner / phase dots | Working, loading projection |
| Filled circle `●` | Needs input, completed, failed (color disambiguates) |
| Hollow circle `○` | Idle / inactive |
| Chevron `❯` | Keyboard selection caret in lists |
| Expand marker `▸`/`▾` | Collapsed / expanded sections |

Color always accompanies glyph for success/error/warning; glyph alone is not
enough for color-blind terminals—prefer **glyph + word status** on critical
rows when space allows.

### Notification chip / row

| Level | Token | Behavior |
|-------|-------|----------|
| Info | `info` | Prefer transient; may not steal permanent layout |
| Warning | `warning` | Visible row until replaced/cleared |
| Error | `error` | Visible row; durable reason may also land in Timeline/system when required |

### Passive status row (BottomBar)

- Never focused, never selected
- Updates when projection changes (model, cwd, context, cost)
- Unknown → `—` / `—/—` in `dim`
- Separators `·` in `dim`

## Timing & motion guidelines

| Feedback | Budget |
|----------|--------|
| Selection move, caret, border focus | Same render tick as input handling |
| Filter as-you-type | Same tick; no debounce required for local lists under a few thousand rows |
| Spinner frame advance | Modest rate (order of ~8–12 Hz class); not full terminal refresh spam for motion alone |
| Double-Esc clear arm window | Short, fixed (order of ~800ms class); first press shows hint |
| Host loading without timeout UI | Stay on loading until success/error; do not invent a fake deadline spinner stop |

## Accessibility & terminal constraints

- Assume **16-color and 256-color** degradation: semantic roles must still differ
  when truecolor is unavailable (themes already target this).
- Do not rely on **blink** as the only selected-state signal.
- Prefer contrast: selected row must remain readable on both dark and light themes.
- Screen-reader tree is non-goal for v1; still keep status words in copy for
  critical failures.

## Consistency checklist (for new components)

Before shipping a new base component or panel composition:

1. [ ] Focused / selected / active are three distinct cues when all apply  
2. [ ] Empty, loading, and error states are designed, not blank  
3. [ ] Confirm and cancel paths are defined; Esc never quits the process  
4. [ ] All colors are theme tokens  
5. [ ] Hint line lists only currently valid keys  
6. [ ] Local actions feedback instantly; host actions show loading/error  
7. [ ] No private selection style that diverges from List  
8. [ ] Motion stops when state ends  

## Configuration

This PRD does not introduce new settings. It constrains how existing systems
present themselves:

| Area | Role |
|------|------|
| Theme tokens | Paint for all states ([themes.md](./themes.md)) |
| Keybindings | Which keys fire navigation/confirm; feedback still follows this PRD |
| Surface feature PRDs | May narrow which states a surface uses, not redefine their look |

## Acceptance criteria

1. Any List-based selector (models, sessions, palette, settings rows) shares the
   same selection caret, accent, and empty/loading language.
2. A user can always answer “where is focus?” from border/caret alone.
3. Active model/agent/session remains identifiable when the highlight moves away.
4. Workflow and approval share frame + choice interaction feedback; labels differ.
5. BottomBar and notification levels use the passive/outcome rules above without
   becoming focus targets (v1).
6. No component uses hard-coded colors for state.
7. Violations of this PRD in surface docs are resolved by updating the surface
   or explicitly listing an exception under Non-goals.

## Non-goals

- Shell information architecture and command inventory (see [ui-ux.md](./ui-ux.md))
- Theme file format and token registry (see [themes.md](./themes.md))
- Pixel-perfect clone of Grok Build chrome
- Mouse-only affordances, drag-and-drop, or rich hover cards
- Sound / haptic feedback
- Animation curves beyond spinner frame advance
- GUI/GPUI component feedback (separate package docs)
- Per-keybinding default tables (see keybindings feature)

## Related documents

- [ui-ux.md](./ui-ux.md) — shell UX contract  
- [themes.md](./themes.md) — semantic colors  
- [keybindings.md](./keybindings.md) — input routing  
- [tool-interactive-workflow.md](./tool-interactive-workflow.md) — workflow composition  
- [editor.md](./editor.md) — text field product behavior  
- [timeline.md](./timeline.md) — content outcome cards in conversation  
- [bottom-bar.md](./bottom-bar.md) — passive status chrome  
