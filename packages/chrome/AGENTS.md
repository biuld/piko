# AGENTS.md — chrome kit crate

## Role

GPUI **Islands UI infrastructure kit** — not a product application.

Provides runtime contracts and composite widgets so multi-pane desktop clients
share one model: **archipelago → islands → list/overlay chrome**.

Consuming apps own product ids, domain messages, data bridges, and content that
fills islands. Prefer extending **this crate** over reimplementing focus,
keyboard, or routing inside application feature modules.

This package is intended for **independent release and iteration**. Do not
document or depend on any specific product app, host daemon, or protocol crate.

## Documentation layout

```text
docs/
├── features/   # WHAT — capability contracts for app authors
├── design/     # HOW  — implementation designs for maintainers
└── roadmap/    # STATUS — backlog, priorities, PR slices
```

Entry: [`docs/README.md`](docs/README.md).

## Allowed dependencies

- `gpui`, `gpui-component`, `pulldown-cmark`, `anyhow` (for `ChromeAssets`)

**Forbidden** (do not add):

- Any product application crate
- Product protocol / host / orchestrator libraries
- Feature/product enums (session ops, prompt kinds, settings IA, …)
- Product i18n catalogs, host transport, headless session kernels

## Source layout

```text
src/
├── runtime/           # L1–L2: pure-ish navigation + contracts
│   ├── archipelago/   # router, workspace, ChromeRoute
│   ├── island/        # IslandView, FocusTable, FocusMsg, host, schedule
│   └── layout/        # IslandNode, prune
├── components/        # L3–L4: reusable GPUI components
│   ├── panel/         # IslandPanel, body states, viewport
│   ├── overlay/       # envelope, surface, focus session
│   ├── list/          # ListKeyboard, tree_list, list_nav
│   └── markdown/      # opaque document, parse adapter, GPUI renderer
│       ├── parse/     # pulldown-cmark adapter and parser frames
│       └── render/    # block, inline, and table layout
├── theme/             # L4: tokens, metrics, typography, icons API
├── assets/            # L4: ChromeAssets (include_bytes → assets/icons)
└── lib.rs             # four explicit public namespaces
```

The public API mirrors ownership rather than flattening it:

| Namespace | Responsibility |
|---|---|
| `runtime::{archipelago,island,layout}` | State, routing, focus, and layout contracts |
| `components::{panel,overlay,list,markdown}` | GPUI elements and interaction components |
| `theme` | Tokens, metrics, typography, and icon helpers |
| `assets` | Embedded asset source |

Do not add crate-root facade modules or flat re-exports. A call site should make
the runtime-versus-presentation dependency visible. Markdown exposes only
`MarkdownDocument`, `parse_markdown`, and `render_markdown`; its semantic tree
is private implementation detail.

## Infra layers

```text
L1 Archipelago runtime   →  runtime/archipelago, runtime/layout
L2 Island runtime        →  runtime/island
L3 Composite components  →  components/panel, components/overlay, components/list
L4 Presentational kit    →  components/markdown, theme, assets
```

## Boundary rules

1. **Shell frames; apps fill.**
2. **Type parameters for product ids** — never hard-code product leaf names.
3. **No domain messages** — apps map `ListKeyEffect` / `FocusMsg` to intents.
4. **Keyboard/list cursor lives here** — apps must not reimplement wrap/cursor.
5. **No product paths or crates** in docs, comments, or dependencies.
6. **File size** — prefer ~300–400 lines; hard ceiling 500.

## When adding API

Ask: would a second multi-pane GPUI app need this **without** any one product’s
domain model?

- Yes → this crate  
- No → the consuming application

**Anti-pattern:** new ↑↓/Enter handling only inside an app feature island.

## Docs map

| Kind | Path |
|---|---|
| Features | [`docs/features/`](docs/features/) |
| Design | [`docs/design/`](docs/design/) |
| Roadmap | [`docs/roadmap/`](docs/roadmap/) |
