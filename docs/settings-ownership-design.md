# Settings Ownership Design

> Status: Phase 1+ landed — presentation lives only under `[tui]` / `[gui]`;
> top-level `theme` / `hide-thinking-block` are not read. GUI Settings MVP
> exists; TUI Settings tree trim (§10 step 6) remains open.
> Related: [GUI Primary Surface Design](gui-primary-surface-design.md),
> [Host Command Catalog Design](host-command-catalog-design.md),
> [Client Core Design](client-core-design.md) §1.3,
> [Client Core Contract Baseline](client-core-contract-baseline.md)
> TUI reference: [Hierarchical Settings Menu](../packages/tui/docs/features/settings.md)

## 1. Purpose

Make configuration ownership explicit across **hostd**, **TUI**, and **GUI**:

- three **schema owners**
- one **persistence and CRUD plane** (hostd)
- each frontend Settings UI edits **host shared settings + its own namespace only**

This removes presentation fields from the top-level `HostSettings` schema and
stops hostd from appearing to "own" TUI/GUI UI preferences.

## 2. Problem (historical)

`HostSettings` previously mixed runtime and presentation at the top level
(`theme`, `hide-thinking-block`). Presentation now belongs only under `[tui]` /
`[gui]`; host top-level holds shared runtime fields only.

## 3. Decisions

1. **Schema ownership is threefold:** `host` | `tui` | `gui`.
2. **Persistence stays unified in hostd:** `~/.piko/settings.toml` and project
   `.piko/settings.toml`, merged by existing `SettingsManager`.
3. **CRUD stays on hostd:** `ConfigGet` / `ConfigUpdate` (and any thin wrappers).
   hostd does **not** parse or validate `[tui]` / `[gui]` interiors beyond JSON
   round-trip.
4. **Frontends own their namespace schema** in their crates (`TuiConfig`,
   `GuiSettings`).
5. **GUI Settings UI** (Primary Surface) shows Host sections + GUI sections.
6. **TUI Settings UI** shows Host sections + TUI sections.
7. Neither frontend edits the other frontend's namespace.
8. Auth remains **`auth.json`** (and env / runtime overrides), not `HostSettings`.
   Settings UIs may expose Account flows that call auth APIs.
9. Approvals remain **`approvals.json`** scopes; Settings may offer review/revoke
   later without folding the file into `HostSettings`.

## 4. Ownership map

### 4.1 Host schema (shared product / runtime)

Authoritative for agent execution and cross-frontend defaults:

```text
default-provider
default-model
default-thinking-level
session-dir
active-tool-names
mcp-servers
[compaction]
[retry]
[sandbox]
```

**Presentation is never shared on host.** Flags such as show/hide thinking in
the timeline are per-frontend (`[tui]` / `[gui]`). TUI and GUI may diverge.

### 4.2 TUI namespace `[tui]`

Schema in `packages/tui` (`TuiConfig` and children):

```text
[tui.bottom-bar]
[tui.editor]
[tui.theme]          # includes name; absorbs legacy top-level theme
[tui.tree]
hide-thinking-block  # or show-thinking — TUI-only presentation
```

TUI keybindings stay in `keybindings.json` (existing), not necessarily inside
`[tui]` for v1 of this redesign.

### 4.3 GUI namespace `[gui]`

Schema in `packages/gui` (`GuiSettings`):

```text
session-width / right-column-width
session-open / right-column-open
reduced-motion
hide-thinking-block  # or show-thinking — GUI-only; independent of [tui]
# future: locale, theme, keyboard prefs, last settings section, …
```

GUI must not read or write `[tui].*`.

### 4.4 Explicitly out of settings.toml schemas

| Concern | Store |
|---|---|
| API keys / OAuth | `~/.piko/auth.json` |
| Tool approval memory | session / workspace / permanent approvals files |
| Custom model catalogs | `~/.piko/models/*.toml` |
| Skills / prompts / theme files | filesystem libraries; Settings may "Reveal in Finder" |

## 5. Removed top-level presentation fields

| Former top-level | Target | Notes |
|---|---|---|
| `theme` | `[tui].theme` | Ignored if still present in old files |
| `hide-thinking-block` | `[tui]` and `[gui]` separately | Not shared; no host read fallback |
| `transport` | Host-only if kept as runtime; not a presentation field | Must not say "TUI transport" in host schema comments |

No compatibility splice: unknown top-level keys are ignored on load. Users must
set presentation under `[tui]` / `[gui]`.

## 6. hostd persistence and CRUD

### 6.1 Storage shape

```toml
# ~/.piko/settings.toml / .piko/settings.toml

default-provider = "…"
default-model = "…"
# … host fields only …

[compaction]
# …

[tui]
# opaque to hostd — TUI schema

[gui]
# opaque to hostd — GUI schema
```

### 6.2 API (existing, clarified)

| Command | Role |
|---|---|
| `ConfigGet { namespace }` | `""` / `"host"` → host fields; `"tui"` / `"gui"` → blob |
| `ConfigUpdate { patch }` | JSON merge patch; frontends patch only keys they own |

Clarify in protocol docs:

- Namespace `"tui"` / `"gui"` return the object under that table.
- Host get may return the host field object without `[tui]`/`[gui]`, or a
  documented envelope — pick one and stick to it in implementation.
- `ConfigUpdate` must deep-merge `[tui]` / `[gui]` objects, not replace the
  entire blob unless the patch intentionally replaces it.

hostd remains responsible for user vs project write path (current
`SettingsManager` behavior). Settings UIs should eventually expose scope
(Advanced); not required to change merge semantics in this design.

### 6.3 What hostd must not do

- Import TUI or GUI config structs
- Validate theme names, dock widths, editor flags, etc.
- Expose presentation fields on a typed `HostSettings` public product API going
  forward (legacy fields only for compatibility)

## 7. Settings UI mapping

```text
GUI Settings surface
├── Host sections     → ConfigUpdate host fields + auth/approvals entry points
└── Appearance / Keyboard / … → ConfigUpdate [gui]

TUI Settings menu
├── Host sections     → same host fields
└── Theme / editor / … → ConfigUpdate [tui]
```

Fast paths (Composer / Command Palette model & thinking) continue to set **host**
defaults / session intents; they are not a substitute for Settings, and Settings
is not a substitute for the command catalog.

## 8. GUI Settings IA (product page)

Aligns with Primary Surface two-column nav. Chrome entry: **TitleBar trailing
gear** (see [GUI Primary Surface Design](gui-primary-surface-design.md)).

```text
General              host defaults (model, thinking, show-thinking policy)
Account & Providers  auth.json flows + custom models hint
Agent & Tools        active tools, MCP, sandbox, approvals
Context & Reliability compaction, retry
Appearance           [gui] only
Keyboard             [gui] only (when shipped)
Advanced             session-dir, config scope, open config folder
```

Priority for first GUI Settings ship: Account, General defaults, Agent & Tools
(MCP / tools / approvals). Appearance can start with `reduced-motion` only.

## 9. Non-goals

- Separate settings files per frontend with independent merge engines
- Putting `[tui]` keys into GUI Settings or vice versa
- Moving auth into `settings.toml`
- Rebuilding TUI Settings as a desktop-style form in the terminal
- Client Core owning any persisted settings schema

## 10. Implementation sequence

1. Document and freeze ownership tables (this doc).
2. Migrate TUI writers off top-level `theme`; move `hide-thinking-block` into
   `[tui]` / `[gui]` separately (not shared).
3. Slim `HostSettings` — no legacy read fallbacks.
4. Harden `ConfigGet`/`ConfigUpdate` namespace semantics + merge tests.
5. Implement GUI Settings surface against Host + `[gui]` only.
6. Trim TUI Settings tree to Host + `[tui]` (remove transport/theme leaks).

## 11. Open questions

1. Does `ConfigGet` without namespace return host-only or full file JSON?
2. Should project vs user write scope be user-visible in v1 Advanced?
3. MCP / approvals management depth for the first GUI Settings milestone?
