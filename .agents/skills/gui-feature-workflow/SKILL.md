---
name: gui-feature-workflow
description: Guide piko GUI feature work from initial user discussion through feature selection, implementation design, code changes, validation, and final user-facing documentation. Use when Codex is asked to add, change, or plan a piko desktop UI feature, especially work involving packages/gui shell, features, islands, overlays, Settings, Command Palette, [gui] settings, or packages/gui/docs artifacts.
---

# GUI Feature Workflow

## Core Rule

Keep the flow explicit:

1. Discuss with the user and reduce the request to one concrete user-visible feature.
2. Write the Feature Doc / Feature Brief that defines the product contract.
3. Only after the Feature Doc is clear, write an implementation design when contracts or subsystem boundaries need agreement.
4. Implement the feature in the GUI architecture.
5. Validate behavior.
6. Update the Feature Doc after behavior is stable.

Use `packages/gui/AGENTS.md` as the source of truth for shell/features/app
boundaries, overlay/Primary Surface rules, config, and docs conventions. Read
it before making design or implementation choices. Visual rules live in
`packages/gui/docs/ui-guidelines.md`.

## Discovery

Start by reducing the request to one feature that can be implemented and reviewed.

Ask only for missing decisions that materially affect the product contract. Prefer concise questions about:

- what the user should see
- what action opens, closes, or changes the feature
- where it lives (Workbench island, Settings section, Overlay layer, TitleBar/StatusBar)
- which keyboard shortcuts or commands are expected
- whether `[gui]` prefs or host runtime settings are required
- what is explicitly out of scope

Write the result as a **Feature Doc** before design or implementation. Keep it concise, but include:

- feature name
- user-visible behavior
- surface placement (Workbench / Settings / Overlay)
- open/close/change triggers
- keyboard shortcuts or commands
- state ownership and persistence expectations
- in-scope behavior
- out-of-scope behavior

If the user asks for broad work, propose one narrow feature candidate and continue once the user agrees. Do not begin design or implementation until the feature has a clear user-visible behavior and scope.

## Feature Doc

The Feature Doc is the product contract and comes before design.

For lightweight discussion, keep it in the conversation as a Feature Brief. When the user asks to formalize it, or when the feature is large enough to need a checked-in artifact, create or update a draft under `packages/gui/docs/features/` before writing the design doc.

Write from the user's perspective:

- overview
- layout
- behavior and interactions
- configuration
- non-goals

Do not include code blocks, file paths, internal structs, or implementation rationale in a Feature Doc. Keep design rationale in `packages/gui/docs/design/`.

After implementation and validation, update the same Feature Doc so it accurately reflects stable behavior.

## Design Gate

The design doc must be derived from the Feature Doc. Do not start with internal structs, protocol shapes, or implementation phases before the user-visible feature contract is stated.

Write a design doc before implementation when the feature affects any of these:

- more than one crate
- protocol DTOs or hostd-to-GUI commands/events
- `[gui]` or host runtime settings schemas
- Primary Surface switching (Workbench ↔ Settings)
- overlay stack / Escape / focus restore
- island focus or `IslandMsg` shapes
- a new shared shell primitive (IslandPanel chrome, overlay panel, workbench column)
- multiple features or a new feature module boundary

Place design docs under `packages/gui/docs/design/`. Start the design doc by naming the selected Feature Doc / Feature Brief, then describe responsibilities, data flow, protocol or config shape, state ownership, shell vs feature placement, and key tradeoffs. Code sketches are acceptable, but keep the design focused on satisfying the feature contract.

Skip the design doc for small single-feature rendering changes that do not alter contracts. State the reason briefly before coding.

## Implementation

Follow the GUI architecture from `packages/gui/AGENTS.md`:

- Shell frames surfaces; features fill slots. Never add product forms under `shell/`.
- Put new product UI under `packages/gui/src/features/<name>/`.
- Keep `app/` as composition + `wiring/` only; move feature mutations to `features/*/actions.rs`.
- Respect dependency direction: `app → features → shell`; shell must not import features.
- `IslandMsg` payloads must stay shell-owned primitives; project from feature VMs at emit time.
- Follow `packages/gui/docs/ui-guidelines.md` for density, type roles, and island chrome.
- Put GUI prefs under `[gui]`, owned by `packages/gui/src/config/`, with hostd storing the blob.

When protocol, session, auth, prompts, skills, compaction, queues, or host settings behavior is needed outside `[gui]`, keep `hostd` authoritative. Put shared wire types in `packages/protocol`. Headless projection stays in `piko-client-core`.

## Validation

Run focused checks for the changed area first, then broader checks when the blast radius warrants it.

Prefer:

```bash
cargo fmt --all
cargo test -p piko-gui
cargo clippy -p piko-gui --all-targets -- -D warnings
```

Use `cargo test --workspace` / workspace clippy for shared protocol, client-core, hostd, or settings-schema changes, or when the feature crosses crate boundaries.

Before finishing, summarize:

- the selected feature
- Feature Doc / Feature Brief status
- design doc path, if created
- implementation files changed
- feature doc path, if created
- validation commands run and their results
