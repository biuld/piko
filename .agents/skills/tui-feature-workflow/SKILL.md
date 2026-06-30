---
name: tui-feature-workflow
description: Guide piko TUI feature work from initial user discussion through feature selection, implementation design, code changes, validation, and final user-facing documentation. Use when Codex is asked to add, change, or plan a piko terminal UI feature, especially work involving packages/tui panels, layout slots, focus/input behavior, settings, overlays, or docs/design and docs/features artifacts.
---

# TUI Feature Workflow

## Core Rule

Keep the flow explicit:

1. Discuss with the user and reduce the request to one concrete user-visible feature.
2. Write the Feature Doc / Feature Brief that defines the product contract.
3. Only after the Feature Doc is clear, write an implementation design when contracts or subsystem boundaries need agreement.
4. Implement the feature in the TUI architecture.
5. Validate behavior.
6. Update the Feature Doc after behavior is stable.

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

Write the result as a **Feature Doc** before design or implementation. Keep it concise, but include:

- feature name
- user-visible behavior
- slot/panel placement
- open/close/change triggers
- keyboard shortcuts or commands
- state ownership and persistence expectations
- in-scope behavior
- out-of-scope behavior

If the user asks for broad work, propose one narrow feature candidate and continue once the user agrees. Do not begin design or implementation until the feature has a clear user-visible behavior and scope.

## Feature Doc

The Feature Doc is the product contract and comes before design.

For lightweight discussion, keep it in the conversation as a Feature Brief. When the user asks to formalize it, or when the feature is large enough to need a checked-in artifact, create or update a draft under `packages/tui/docs/features/` before writing the design doc.

Write from the user's perspective:

- overview
- layout
- behavior and interactions
- configuration
- non-goals

Do not include code blocks, file paths, internal structs, or implementation rationale in a Feature Doc. Keep design rationale in `docs/design/`.

After implementation and validation, update the same Feature Doc so it accurately reflects stable behavior.

## Design Gate

The design doc must be derived from the Feature Doc. Do not start with internal structs, protocol shapes, or implementation phases before the user-visible feature contract is stated.

Write a design doc before implementation when the feature affects any of these:

- more than one crate
- protocol DTOs or hostd-to-tui commands/events
- settings schemas or persisted config
- focus/input routing
- layout slot behavior
- a new reusable component
- multiple panels or an overlay lifecycle

Place design docs under `packages/tui/docs/design/`. Start the design doc by naming the selected Feature Doc / Feature Brief, then describe responsibilities, data flow, protocol or config shape, state ownership, panel placement, focus behavior, and key tradeoffs. Code sketches are acceptable, but keep the design focused on satisfying the feature contract.

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
- Feature Doc / Feature Brief status
- design doc path, if created
- implementation files changed
- feature doc path, if created
- validation commands run and their results
