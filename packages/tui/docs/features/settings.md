# Settings surface

> Status: implemented (value visibility + shared MenuStack; draft→reviewed pending
> product walkthrough)
>
> Parent: [ui-ux.md](./ui-ux.md) (full overlay, slash/palette openers)
> Feedback language: [component-feedback.md](./component-feedback.md)
> (Selected ≠ Active; new Settings components listed there)
> Persistence transport: hostd `ConfigGet` / `ConfigUpdate` (JSON merge patch;
> design: [config-unification.md](../design/config-unification.md))

## Overview

The Settings surface lets the user inspect and change runtime configuration
while staying in the chat session. It opens as a **full overlay** (replaces
zones A–D above BottomBar), same placement family as Sessions / Tree.

Settings are **not** a free-form editor and **not** a wall of Enable/Disable
commands. They are a **value-first catalog** of named settings: at every level
the user can see **what is currently in force**, which option is **active**,
and (when relevant) whether a change is **persisted only** or also **already
live**.

Settings and command-palette-style surfaces share the **same drill-down
component** (`MenuStack`) — depth stack, per-frame filter, Esc pop. What makes
Settings “settings” is the **row payload**, not a second navigation stack:
`MenuRow` carries an optional ValueSummary, EffectBadge, Active flag, and
exclusive choice children, painted as `SettingsRow` / `SettingsOption`
layouts on the shared List selection language.

This PRD is the product contract for Settings: surface behavior, catalog, and
the **TUI components** the surface composes.

## Problem (why this revision)

Today the menu fails basic “settings list” jobs:

1. **No current value on the way in.** Root groups such as Observability,
   Sandbox, Retries show only static description text. The user cannot tell
   On vs Off without leaving the TUI or reading `settings.toml`.
2. **Boolean pairs look like two equal commands.** “Enable OTLP export” and
   “Disable OTLP export” compete with no in-force highlight on the chosen side.
3. **Incomplete Active markers.** Only thinking level, theme, and a few TUI
   presentation flags participate in “selected vs active.” Host-backed keys
   (observability, sandbox, retry, compaction, endpoints) mostly use `_ => false`.
4. **Latency-to-effect is invisible.** OTel exporters install at hostd process
   start. A user can “Enable” and believe export is live after toast success
   while the running process still uses the old stack.
5. **Stale local picture.** TUI only bootstraps `ConfigGet { namespace: "tui" }`.
   Host namespace values are never loaded for the settings panel, so even a
   perfect active marker has nothing truthful to show.
6. **Wrong component.** A plain action tree cannot express “section + live
   value + effect class + exclusive choice”. `MenuRow` (shared with Auth /
   palettes) carries those fields so one drill-down component serves both,
   with Settings-specific row layouts.

## Goals

- Compose the Settings surface from the shared **`MenuStack`** (see
  § Component kit) with value-aware `MenuRow` payloads; stop encoding settings
  solely as plain action rows.
- At the **catalog root**, every setting that owns a durable value shows a
  **value summary** (and effect hint when needed). Scanning answers “what do
  I have set?” without drilling.
- Inside a choice leaf, the **in-force option** uses **Active** (accent label), distinct
  from keyboard **Selected** (`❯`).
- TUI holds a **client mirror** of host + tui keys shown in the catalog.
- Apply via `ConfigUpdate`; mirror updates optimistically so reopen is honest.
- **Restart hostd** (and similar) effect classes are disclosed by components
  designed for that field — not buried only in docs.

## Non-goals

- Free-text path/URL/number entry (no Settings text fields in this PRD).
  Closed enum / preset / boolean only. Custom values beyond presets remain
  file-based but **must still display** in value summaries.
- Multi-column property sheets, tabs, or mouse-first forms.
- Live re-init of OTel inside a running hostd (Settings only discloses).
- Editing every hostd key — only the catalog in this PRD.
- Replacing the shell List selection language (Settings components **extend**
  List/chromelets; they do not invent private carets or focus borders).
- Reintroducing a separate settings navigation stack: `MenuStack` is the one
  drill-down component for Settings and menu-style surfaces.

## Component kit

Settings is a **surface** (panel + focus mode) that composes the shared
drill-down component with value-aware rows. Names are product contracts;
rust module names may differ as long as the boundaries stay clear.

```text
Settings surface (full overlay)
└─ MenuStack<SettingsAction>  # shared drill-down (also Auth); depth, title,
   │                          # Esc pop, filter scoped to current frame
   ├─ root frame              # SettingsRow layout: catalog sections
   │    └─ MenuRow::Section    # label · ValueSummary · EffectBadge · drill
   ├─ branch frame            # MenuRow::Branch (e.g. Compaction) → SettingsRow
   └─ choice frame            # SettingsOption layout: exclusive options
        └─ MenuRow::Option     # label · consequence detail · Active
```

Shared chromelets (from component-feedback): selection caret, active marker,
frame border, list filter, footer hints, loading placeholder.

### Why MenuRow carries settings fields

| Concern | Plain action row | `MenuRow` (Settings) |
|---------|------------------|----------------------|
| Show current value on parent | Detail is static string, hand-authored | `value` (ValueSummary) derived from client mirror |
| Exclusive active among N options | Optional callback; often incomplete | `is_active` required on option rows |
| On/Off | Two parallel “Enable/Disable …” actions | binary options with short labels |
| Restart vs live | Free-text in detail | `badge` (`restart hostd`) on summary |
| Intermediate section | Same node type as leaf action | `Branch` vs `Choice` children |
| Thinking selector | Band-mode picker (`/thinking`) | Settings → Thinking shares level copy/actions |

New catalog keys must land as **`MenuRow` compositions**, not one more
static-string action row.

### MenuStack (shared drill-down)

**Job:** Own drill depth, panel title (current section name), filter string
scoped to the **current frame only**, Esc/q pop, open-at-branch (thinking
picker).

**Must**

- Title always reflects depth context (`settings` → `Observability` → `OTLP endpoint`).
- Popping clears filter for the revealed frame.
- Never apply a value on pop.

**Must not**

- Own ConfigUpdate semantics (parent surface / session applies).
- Mix unrelated surfaces’ nodes into the same stack without a product reason.

### SettingsRow (catalog / branch row)

**Job:** One navigable row that answers “what is this setting **now**?” and
invites drill-in.

```text
❯ Observability                                        >
  On · http://127.0.0.1:4318              [restart hostd]
```

| Zone | Content | Style class |
|------|---------|-------------|
| Primary | Setting / section title | text; selected → accent |
| Trailing | Drill `>` when children exist | dim |
| Secondary | **ValueSummary** (required when the section owns a value) | muted/dim |
| Affix | **EffectBadge** when effect class is not Live/default silence | dim warn or badge language |

**Must**

- ValueSummary is **data-driven** from the client mirror, not a hard-coded blurb.
- Missing mirror while loading: summary is loading/placeholder (`…` / spinner
  row), not a fake “Off”.
- Filter matches title, summary text, and badge labels.

**Must not**

- Use Active (accent label) as a substitute for the value summary on section rows.
- Present two peer command slogans (“Enable…”, “Disable…”) as the section row.

### ValueSummary

**Job:** One short, scannable line derived from the mirror for a setting or
composite section (e.g. Observability = enable + endpoint).

Rules:

- Booleans compact to `On` / `Off` (or domain synonyms only when clearer).
- Numbers compact (`16k`, not `16384 tokens`) when that is the catalog style.
- Compounds join with middle dots: `On · http://127.0.0.1:4318`.
- Custom-not-in-preset values still render the **actual** stored string
  (truncation allowed with end ellipsis); may add `custom` only when useful.
- Never invent defaults that contradict host defaults documentation.

### EffectBadge

**Job:** Surface **latency-to-effect** without burying it in prose paragraphs.

| Effect class | Badge / affix (product copy may refine tokens) | When shown |
|--------------|------------------------------------------------|------------|
| Live | (none by default) | — |
| Presentation | optional silent | after tui apply |
| Restart hostd | `restart hostd` (or short equivalent) | on section summary **and** on apply toast |

Badge is secondary; it must not steal Selection accent. Text alone must work
without color-only meaning.

### Choice frame (SettingsOption rows)

**Job:** Exclusive pick among closed options for **one** logical key (or a
tight pair of keys only when a product catalogs them as one choice set).

Renders N `SettingsOption` rows. Exactly one is Active when the mirror matches a
listed option; zero when custom/unmatched.

Enter on an option → **apply** that value and close Settings (surface policy).

**Boolean specialization:** options are short `On` / `Off` (or Hide/Show). No
duplicate long Enable/Disable titles as the primary labels.

**Enum / preset specialization:** short primary (level name, theme name,
endpoint preset title); detail = consequence or full URL.

Used by: Thinking Level, Theme, On/Off keys, OTLP endpoint presets, tools
mode, compaction size leaves.

Thinking picker reuses **this** list shape for levels — not a one-off menu.

### SettingsOption row

**Job:** One choosable value.

```text
❯ On
  Export traces/metrics/logs when hostd starts
  Off
  Stderr only when hostd starts
```

| Zone | Content |
|------|---------|
| Primary | Option label |
| Active | accent label when in-force |
| Detail | What choosing this does (and restart note when Restart class) |

Selected ≠ Active always holds.

### Branch frame (SettingsRow rows)

**Job:** Intermediate folder whose children are more section rows or choice
lists (e.g. Automatic Compaction → enable + reserve + keep). Own value summary
is a **roll-up** of child mirrors (`On · reserve 16k · keep 20k`).

Enter drills; never applies a partial value at branch row.

### Settings surface (composition owner)

**Job:** Mode/focus, open/close, mirror load/refresh, map option confirm →
`ConfigUpdate` patch, optimistic mirror write, notifications.

Owns product catalog wiring (which section → which keys). Does not reimplement
List selection paint.

### Future settings components (explicitly out of this PRD’s ship, reserved names)

| Component | When needed |
|-----------|-------------|
| SettingTextField | Free URL/path/number entry |
| SettingConfirmStep | Destructive resets that need a second Enter |
| SettingMultiToggle | Independent flags on one screen without drill |
| SettingSearchIndex | Cross-tree fuzzy beyond current-frame filter |

Reserved so we do not overload ChoiceList later.

## Layout

Settings is a **full overlay** (replaces content above BottomBar; BottomBar
may remain for orientation). The catalog uses full terminal height for value
lists and nested options.

Root catalog is **domain-chunked**: non-selectable group captions group related
rows (Thinking · Context · Tools · Diagnostics · Appearance · Advanced). Section
rows use a **key–value** line (title left, ValueSummary right). Choice leaves use
**stacked** rows so each option’s consequence detail is readable under the label.

```
┌─ settings ────────────────────────── [1/9] ─┐
│   Thinking                                  │
│ ❯ Level                               medium ▸│
│   Blocks                               shown ▸│
│                                                 │
│   Context                                       │
│   Compaction          On · reserve 16k · keep 20k ▸│
│   API Retries                             On ▸│
│   Tools                                         │
│   Sandbox                                Off ▸│
│ …                                               │
│ ↑/↓ navigate · Enter open · Esc close           │
└─────────────────────────────────────────────────┘
```

Choice leaf (OTLP export):

```
┌─ OTLP export ─────────────────────── [1/2] ─┐
│ ❯ On                                       │
│   Export via OTLP HTTP · restart hostd      │
│   Off                                       │
│   Stderr only after hostd starts            │
│ ↑/↓ navigate · Enter apply · Esc back       │
└─────────────────────────────────────────────┘
```

### Frame copy

Footer hints may distinguish **open** (catalog/branch) vs **apply**
(choice leaf). Default list chrome still applies (component-feedback).

## Behavior / interactions

### Opening

- Open via `/settings` or any surface action defined as Open Settings.
- Opening **refreshes** the client’s host/tui mirror used by the tree when a
  refresh channel exists (`ConfigGet` for relevant namespaces). Stale disk
  from an external editor is not a hard guarantee if the user never reopens;
  reopen/open must re-fetch or rehydrate.
- `/thinking` opens the band-mode Thinking Selector (ComposerBand) with the
  **same** level set and Active/value rules as the Settings → Thinking branch.

### Navigation and confirm

Bindings stay keyboard-first (ui-ux + component-feedback):

| Input | Behavior |
|-------|----------|
| ↑ / ↓ | Move selection in current frame |
| typing | Filter **current** MenuStack frame |
| Enter on SettingsRow / Branch frame | Drill in; clear filter |
| Enter on SettingsOption | Apply value; **close** Settings; return to chat |
| Esc / q | Pop MenuStack; at root, close surface |

Closing without Enter applies nothing.

### Active vs selected

Per component-feedback:

- **Selected** = keyboard row (`❯`).
- **Active** = in-force option on `SettingsOption` rows only.
- Section/branch rows use **ValueSummary**, never Active as a fake value.

When the mirror value is outside the choice set (custom endpoint):

- Zero option rows Active.
- ValueSummary still shows the stored string.
- Optional `custom` hint when it reduces confusion.

### Apply / persistence

1. Enter on an action builds a JSON merge patch for that field only (same
   ConfigUpdate model as today).
2. Local client mirror updates **optimistically** for every key the panel
   displays, including host keys that do not currently push a ConfigEntry
   event.
3. Success feedback: short status + notification, including **restart
   required** when the key has process-boot semantics.
4. Failure of ConfigUpdate: mirror should not claim success; user is told
   of rejection (existing command-error path).

### Restart / live effect classes

Each catalog entry is tagged in product language with one class:

| Class | Meaning | UI duty |
|-------|---------|---------|
| **Live** | Host or TUI applies without process restart | No restart copy |
| **Presentation** | Client-only (e.g. hide thinking, theme) once TuiConfig is applied | No host restart |
| **Restart hostd** | Value is durably stored but runtime exporters/log stack take effect on next hostd process | Always disclose in group detail + apply toast |

Observability OTLP enable + endpoint are **Restart hostd** today. If hostd
later hot-reloads OTel, this PRD’s disclosure rule still holds until that
class is reclassified.

## Value summary rules

Root (and intermediate) group **detail** is derived from the client mirror:

| Chunk · row | Summary shape (examples) |
|-------------|---------------------------|
| Thinking · Level | `medium` |
| Thinking · Blocks | `shown` / `hidden` |
| Context · Compaction | `On · reserve 16k · keep 20k` or `Off` |
| Context · API Retries | `On` / `Off` |
| Tools · Sandbox | `On` / `Off` |
| Tools · Active Tools | `all` / `none` |
| Diagnostics · Observability | `On · http://127.0.0.1:4318` or `Off` (+ EffectBadge) |
| Appearance · Theme | `dark` / `light` |
| Advanced · Transport | `stdio` (or actual value) |

Summaries stay one short line; use compact numbers (`16k`) not full phrases.

## Configuration tree catalog

Closed set for this PRD (labels can be refined in copy review; behavior is norm):

### Choice enums

| Group | Options | Storage |
|-------|---------|---------|
| Thinking Level | off, minimal, low, medium, high, xhigh | host `default-thinking-level` (Live for new turns per existing model config hooks) |
| UI Theme | dark, light | `tui.theme.name` (Presentation) |
| Transport Preference | presets as product allows (e.g. stdio) | host transport (as today) |
| Compaction reserve / keep | closed token sizes | host `[compaction]` (Live on next compaction evaluation) |
| OTLP Endpoint | closed presets (`127.0.0.1:4318`, `localhost:4318`, …) | host `[observability].otel-endpoint` (**Restart hostd**) |

### Booleans → binary choice frame

| Group | Storage | Effect class |
|-------|---------|--------------|
| Thinking Blocks | `tui.hide_thinking_block` | Presentation |
| Automatic Compaction enable | `[compaction].enabled` | Live |
| API Retries | `[retry].enabled` | Live |
| Tool Sandbox | `[sandbox].enabled` | Live |
| Observability (OTLP export) | `[observability].enabled` | **Restart hostd** |
| Active Tools Mode | enable-all vs empty list via active-tool-names | Live |

Each maps to a binary **choice frame**, not a pair of parallel command
menu actions. Labels: short **On** / **Off** (or Hide / Show for thinking
blocks only when clearer).

### Client mirror source

| Namespace | Keys used by Settings |
|-----------|------------------------|
| `ConfigGet { namespace: "host" }` | compaction, retry, sandbox, observability, default-thinking-level, transport, active-tool-names, … as listed above |
| `ConfigGet { namespace: "tui" }` | theme, hide_thinking_block (existing TuiConfig) |
| Live model events | may refresh thinking/model chrome; must not diverge from Settings thinking Active when both are shown |

Defaults when a key is missing match hostd / TUI defaults (e.g. observability
export **Off**, otel endpoint `http://127.0.0.1:4318`).

## User journeys

1. **See export state.** User opens Settings. Root row Observability detail
   shows `Off · restart hostd for export` without drilling.
2. **Turn export on.** User drills → On (becomes Active after confirm) →
   toast includes restart requirement → disk has `enabled = true` → after
   hostd restart, export is live; reopening Settings shows `On · <endpoint>…`.
3. **Custom endpoint file.** User set a non-preset endpoint in TOML. Settings
   shows that URL in the group summary; no preset row is Active.
4. **Sandboxed tools.** User sees Sandbox `On` at root, drills, Active is on
   the correct option, flips Off, session behavior follows host policy for
   **Live** keys without a “restart hostd” claim.

## Accessibility / scanability

- Value must be recoverable without color alone (text summary + ●).
- Active marker and Selected caret never fuse into one ambiguous glyph.
- Filter still useful: “otlp”, “retry”, endpoint host fragments match detail.

## Acceptance criteria

### Surface / data

- [ ] Opening Settings shows **current** ValueSummaries for all catalog
      sections that own a value (including Observability enable + endpoint).
- [ ] Settings mirror loads **host** + **tui** namespaces (bootstrap and/or
      open-time refresh).
- [ ] Apply updates mirror optimistically; next open does not require
      process restart to show the new **stored** value.
- [ ] Restart-class keys (observability) always show EffectBadge and apply
      toast until reclassified.
- [ ] Custom non-preset values display in ValueSummary; no false Active on presets.

### Components

- [x] Settings composes the shared MenuStack (§ Component kit) with
      SettingsRow / SettingsOption layouts; the catalog builds `MenuRow`
      trees, not ad-hoc static rows.
- [ ] Choice frames: one Active when matched; zero when custom.
- [x] Thinking selector shares level copy with Settings → Thinking.
- [ ] Selected ≠ Active holds on all SettingsOption rows.
- [ ] SettingsRow never uses Active in place of ValueSummary.
- [ ] No free-text field ships in this PRD.

### Regression

- [ ] Existing openers (`/settings`, palette, thinking open-at-branch) still
      work under MenuStack.

## Out of catalog (this PRD)

Approvals, guardian, MCP server lists, permission profiles, features pins,
guardian models, log levels — not editable in this Settings tree unless a
later PRD adds them with the same value-summary + Active rules.

## Open questions (resolve before or during design)

1. Should open Settings always `ConfigGet host` (freshness) or only bootstrap +
   optimistic apply? **Recommendation:** open + bootstrap for host and tui.
2. Should successful Restart-class applies keep the panel open with a
   non-dismissible note? **Recommendation:** keep close-on-Enter; put the
   note in notification + status.
3. Endpoint preset list growth (Aspire-only ports, cloud OTLP) — keep as preset
   PRs, not free text, until SettingTextField is designed.
4. Should EffectBadge be a glyph/chip on the primary line or only a trailing
   phrase in ValueSummary? **Recommendation:** trailing affix on SectionRow
   secondary line, shared badge chromelet.
5. Implementation: ~~evolve hierarchical_menu vs new modules~~ **Resolved:**
   one shared `MenuStack` (`ui/components/menu`) serves Auth + Settings;
   Settings adds row payload (value / badge / active) and row layouts.

## Configuration (product keys, not implementation)

- Panel openers: existing Settings surface / slash `/settings` / thinking
  sub-entry as today.
- No new keybinding required by this PRD.
- Host keys: existing `[observability]`, `[sandbox]`, `[retry]`,
  `[compaction]`, `default-thinking-level`, etc.
- TUI keys: existing `[tui]` blob fields used above.

## Related

- Host observability behavior: root `docs/features/F-15-observability.md`
- Visual language: [component-feedback.md](./component-feedback.md)
- Shell placement: [ui-ux.md](./ui-ux.md)
- Transport: [config-unification.md](../design/config-unification.md)
