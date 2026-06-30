---
name: tui-feature-workflow
description: Guide piko TUI feature work from initial user discussion through feature selection, implementation design, code changes, validation, and final user-facing documentation. Use when Codex is asked to add, change, or plan a piko terminal UI feature, especially work involving packages/tui panels, layout slots, focus/input behavior, settings, overlays, or docs/design and docs/features artifacts.
---

# TUI Feature Workflow

## Core Rule

Keep the flow explicit:

1. Discuss with the user to identify one concrete feature.
2. Write an implementation design first when contracts or subsystem boundaries need agreement.
3. Implement the feature in the TUI architecture.
4. Validate behavior.
5. Write or update the user-facing feature doc after behavior is stable.

Use `packages/tui/AGENTS.md` as the source of truth for TUI layout, panel, component, focus, input, config, and docs conventions. Read it before making design or implementation choices.

## Discovery

Start by reducing the request to one feature that can be implemented and reviewed.

Ask only for missing decisions that materially affect the product contract. Prefer concise questions about:

- what the user should see
- what action opens, closes, or changes the feature
- where it lives in the slot layout
- which keyboard shortcuts or commands are expected
- whether settings or persisted state are required
- what is explicitly out of scope

If the user asks for broad work, propose one narrow feature candidate and continue once the user agrees. Do not begin implementation until the feature has a clear user-visible behavior.

## Design Gate

Write a design doc before implementation when the feature affects any of these:

- more than one crate
- protocol DTOs or hostd-to-tui commands/events
- settings schemas or persisted config
- focus/input routing
- layout slot behavior
- a new reusable component
- multiple panels or an overlay lifecycle

Place design docs under `packages/tui/docs/design/`. Describe responsibilities, data flow, protocol or config shape, state ownership, panel placement, focus behavior, and key tradeoffs. Code sketches are acceptable, but keep the design focused on contracts and architecture.

Skip the design doc for small single-panel rendering changes that do not alter contracts. State the reason briefly before coding.

## Implementation

Follow the TUI architecture from `packages/tui/AGENTS.md`:

- Treat every visible element as a panel assigned to a layout slot.
- Avoid floating UI and absolute positioning.
- Add new panels under `packages/tui/src/panels/` with a panel struct and `render()` method.
- Use reusable components under `packages/tui/src/components/` only when the abstraction serves multiple panels or clearly matches an existing pattern.
- Keep `build_constraints()` pure: it should depend on layout mode and dynamic measurements, not on concrete panels.
- Model overlay open/close with `AppMode`, `Placement`, `FocusTarget`, and LIFO `FocusManager` behavior.
- Preserve input priority: global Esc/Enter first, focus owner second, editor fallback third.
- Put TUI-specific config under `[tui]`, owned by `packages/tui/src/config/`, with hostd storing the blob.

When protocol, session, auth, prompts, skills, compaction, queues, or settings behavior is needed outside TUI config, keep `hostd` authoritative for user-visible state. Put shared wire types in `packages/protocol`.

## Feature Doc

After the implementation behavior is stable, create or update a feature spec under `packages/tui/docs/features/`.

Write from the user's perspective only:

- overview
- layout
- behavior and interactions
- configuration
- non-goals

Do not include code blocks, file paths, internal structs, or implementation rationale in a feature doc. Keep design rationale in `docs/design/`.

## Validation

Run focused checks for the changed area first, then broader checks when the blast radius warrants it.

Prefer:

```bash
cargo fmt --all
cargo test -p tui
cargo clippy --workspace --all-targets -- -D warnings
```

Use `cargo test --workspace` for shared protocol, hostd, orchd, llmd, or sandbox changes, or when the feature crosses crate boundaries.

Before finishing, summarize:

- the selected feature
- design doc path, if created
- implementation files changed
- feature doc path, if created
- validation commands run and their results
