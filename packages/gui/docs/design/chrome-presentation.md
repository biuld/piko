# GUI Chrome Presentation Design

> Status: implemented design derived from the Feature Doc
> Feature contract: [GUI Chrome Presentation](../features/chrome-presentation.md)
> Visual rules: [UI Guidelines](../ui-guidelines.md)
> Parent design: [GPUI Desktop Client Design](overview.md) §12
> Locked decisions: Empty icons (§3.2), full Activity i18n templates,
> v1 locale = `en` only (no `[gui].locale`)

## 1. Purpose

Satisfy the Chrome Presentation feature contract with three coordinated modules
inside `packages/gui`:

1. **Icons** — vendored Lucide-compatible SVG subset + typed names.
2. **Typography** — named text styles built on existing `UiMetrics`.
3. **i18n** — GUI-owned chrome catalog with **`en` only** for v1; call sites use
   keys so later locales do not require island rewrites.

This design does not change Client Core, protocol DTOs, or hostd Session
semantics. No new `[gui]` keys were required for this wave.

Header/tree trailing alignment and tool-window row geometry are **not** part of
this feature; see [Tool-Window Row Layout](chrome-tool-window-layout.md).

## 2. Responsibilities

| Owner | Owns | Does not own |
|---|---|---|
| `theme/metrics.rs` | Spacing + type sizes / line heights | Copy or icons |
| `theme/typography.rs` | Named text styles (`Meta`, `Label`, `Body`, …) | Island layout |
| `theme/icons.rs` | `PikoIcon` enum, sizes, `IconNamed` paths | Asset bytes on disk |
| `assets/icons/*.svg` | Vendored SVG files | Runtime tint |
| `i18n/` | `rust_i18n` catalog + thin `t!` helpers | Host / transcript text |
| Island / chrome renderers | Call icons + typography + `t!` | Hardcoded glyphs / string literals |

`IslandMedia::Icon(SharedString)` remains for tests and escape hatches, but
product Empty / Loading paths should prefer `IslandMedia::Element` built from
`PikoIcon`, or a small `IslandPlaceholder::piko_icon(...)` helper.

## 3. Icons

### 3.1 Asset policy

- Store SVG under `packages/gui/assets/icons/`.
- Source: Lucide (or Lucide-compatible) paths already familiar from
  `gpui-component::IconName`, but **vendor only the subset piko uses**.
- Do not rely on gpui-component's internal asset bundle for product icons;
  piko registers its own `AssetSource` (or GPUI asset path) at startup.
- Keep filenames stable Lucide-style kebab-case (`plus.svg`,
  `chevron-down.svg`).

### 3.2 Typed names (v1 subset, locked)

| `PikoIcon` | Typical use | Replaces |
|---|---|---|
| `Plus` | Per-directory New Session action | `"+"` label |
| `ChevronRight` | Collapsed disclosure | `▸` |
| `ChevronDown` | Expanded disclosure | `▾` |
| `CircleDashed` | Loading placeholders | `◌` |
| `MessageSquare` | Timeline empty; Session rows | `✍` / none |
| `Circle` | Sessions empty / center no-session | `○` |
| `Bot` | Agents empty / agent rows; Timeline assistant; Tree assistant | `◇` |
| `User` | Timeline / Tree user rows | none |
| `Folder` / `FolderOpen` | Sessions directory rows; Open Directory header uses `FolderOpen` | none |
| `Wrench` | Tree tool call | none |
| `Brain` | Tree thinking-level change | none |
| `Cpu` | Tree model change | none |
| `GitBranch` | Tree branch summary | none |
| `Layers` | Tree compaction | none |
| `Network` | Tree empty | `▤` |
| `TriangleAlert` | Error placeholder | `"!"` |
| `PanelLeft` / `PanelRight` | Narrow Sheet openers (optional) | text-only buttons |

Leaf tree rows keep a non-interactive spacer or muted dash; they do **not**
need a filled icon. Status markers stay 6 px dots in `status_bar` /
Activity / Timeline.

### 3.3 Sizes

Align icon boxes to typography roles:

| Token | Box | Use |
|---|---:|---|
| `IconSize::Meta` | 12 px | Disclosure column, inline chrome |
| `IconSize::Label` | 14 px | Header ghost actions beside 13/18 labels |
| `IconSize::Placeholder` | 28 px | Empty / Loading mark (matches current glyph size) |

Helpers return `gpui_component::Icon` (or raw `svg()`) with theme color applied
via existing token HSLA / RGBA helpers.

### 3.4 Integration points

1. `tree_list.rs` — disclosure column uses `Chevron*`.
2. `composer/render.rs` — Activity disclosure uses the same chevrons.
3. `IslandPlaceholder` — Empty / Loading use `Placeholder` size icons.
4. Sessions header — `Button` with `FolderOpen` icon (Open Directory); directory
   rows use `Plus` for New Session.
5. Tests — assert `PikoIcon` path mapping; render tests may keep string icons
   only where they exercise `IslandMedia::Icon`.

## 4. Typography

### 4.1 Text styles

Add `TextRole` helpers on top of `UiMetrics`:

| Style (`TextRole`) | Size / LH | Weight default | Font |
|---|---:|---|---|
| `Meta` | 12 / 16 | Regular | UI |
| `Label` | 13 / 18 | Regular / Semibold when selected | UI |
| `Body` | 14 / 21 | Regular | UI |
| `BodyMono` | 14 / 21 | Regular | Monospace |
| `PlaceholderTitle` | 13 / 18 | Semibold | UI |
| `PlaceholderSubtitle` | 14 / 21 | Regular | UI |

`apply_*` helpers set `text_size`, `line_height`, optional `font_weight`, and
`font_family` so islands stop sprinkling raw `px` pairs.

### 4.2 Font policy

- UI: system UI font (GPUI default / platform). No bundling Inter/Roboto.
- Mono: platform monospace family already used for tool detail.
- Do not introduce a third display face in this wave.

### 4.3 Relationship to UI Guidelines

`ui-guidelines.md` §4 (Density and typography) remains
normative for numbers. The typography module is the implementation source of
truth that checklist items and render code must call. When metrics change,
update guidelines and `UiMetrics` together.

## 5. i18n

### 5.1 Stack choice

Use `rust_i18n` the same way `gpui-component` does:

- Catalog under `packages/gui/locales/` with **`en` entries only** for v1
  (same file layout gpui-component uses is fine; omit other locale columns
  until a later wave).
- `fallback = "en"`.
- Force `set_locale("en")` at startup (and keep `gpui_component::set_locale`
  on `en`) so OS locale cannot change chrome copy in this wave.
- Thin wrapper `crate::i18n::t(key)` / `t!(...)` so call sites do not import the
  macro crate everywhere.

Do not share catalogs with TUI in this wave.

### 5.2 Key naming

Prefer stable semantic keys, not English sentences as keys:

```text
island.sessions.title
island.sessions.empty.title
island.sessions.empty.subtitle
island.sessions.action.new
composer.action.send
composer.action.stop
composer.placeholder
status.connection.connected
status.connection.disconnected
activity.summary.*
activity.item.*
dialog.action.submit
dialog.action.cancel
dialog.approval.decline
dialog.approval.arguments
```

Interpolation uses `rust_i18n`'s argument form for counts and dynamic host
fragments **only when the surrounding sentence is chrome-owned** (for example
`"Error: %{message}"` where `message` is still host text).

### 5.3 Locale resolution (v1)

```text
startup → set_locale("en") → gpui_component::set_locale("en")
```

No `[gui].locale`, no OS following, no runtime switch. Multi-locale resolution
is deferred; when it lands, update the Feature Doc first.

### 5.4 What gets keyed in v1

**Yes (GUI chrome):** island titles, placeholders, tooltips, Composer chrome,
**every** Activity summary and item label template owned by `activity_vm`,
StatusBar connection labels, Sheet button labels, dialog chrome labels,
Tree entry chrome templates such as `tool %{name}`.

**No:** transcript bodies, tool names/args/results as dictionary entries, Agent
names, model ids, thinking level raw values shown as data, host error strings
(may be wrapped by an English chrome prefix only).

### 5.5 Testing

- Unit-test key coverage: every `t!("...")` key exists for `en`.
- Assert a small set of resolved English strings.
- Existing VM tests that assert English substrings (`"running"`, `"approval"`)
  either resolve through the catalog under a locked `en` locale or assert on
  structured enums instead of rendered copy where practical.

## 6. Config shape

No `GuiSettings` changes in this wave. Locale override remains a future
addition with the first non-`en` catalog.

## 7. Module layout

```text
packages/gui/
├── assets/icons/*.svg
├── locales/…                 # rust_i18n catalog (en)
└── src/
    ├── i18n/mod.rs
    └── theme/
        ├── metrics.rs        # sizes
        ├── typography.rs     # text styles
        └── icons.rs          # PikoIcon + sizes
```

Startup in `main.rs` / `DesktopApp::new`:

1. `gpui_component::init`
2. register GUI assets
3. `i18n::init` + force `en`
4. `apply_piko_dark_theme`

## 8. Landed sequence

Implementation landed in this order:

1. Typography helpers
2. Assets + `PikoIcon` + AssetSource
3. i18n catalog (`en`) + force locale, including all Activity templates
4. Replacement of Unicode glyphs in tree, Activity, placeholders, New Session
5. UI Guidelines / known-limitations updates
6. Validation of English chrome + mixed CJK content beside icons on Retina

## 9. Tradeoffs

| Choice | Why | Cost |
|---|---|---|
| Vendor Lucide subset instead of using `IconName` directly | Matches gui-design §12; avoids depending on component asset paths | Duplicate a few SVGs |
| `rust_i18n` like gpui-component | One pattern in the desktop stack; ready for later locales | Macro/build input even for `en`-only |
| `en` only in v1 | Ships keys without translation scope creep | No zh-CN yet |
| No `[gui].locale` yet | Nothing to override until a second locale exists | Settings story deferred |
| Chrome-only i18n | Host content stays authoritative and uncorrupted | Users still see English tool/agent names |
| Keep 6 px markers | Guidelines already define status vs authorship | Two mark languages (dot vs icon) by design |

## 10. Decisions (closed)

1. **Empty / Loading icons** — locked to the table in §3.2 (`Circle`,
   `MessageSquare`, `Bot`, `Network`, `CircleDashed`, `TriangleAlert`, plus
   `Plus` / chevrons).
2. **Activity copy** — all summary and item templates go through the `en`
   catalog in the first landing; no leftover hardcoded Activity chrome
   literals.
3. **Locale** — v1 supports `en` only; no `[gui].locale`, no OS following.
   Additional locales require a Feature Doc update before implementation.
