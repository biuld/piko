# ADR-001: codex-rs is behavior evidence; PRD-first documentation workflow

> Status: accepted
> Date: 2026-08-02

## Context

piko will reimplement the agent runtime capabilities distilled from the
codex-rs core. codex-rs carries years of battle-tested behavior (turn loop,
tool execution, context management, compaction) encoded in its tests and
implementation, but it also carries OpenAI cloud coupling and a monolithic
architecture that piko must not inherit. Direct porting of codex-rs structure
would import that coupling.

## Decision

piko adopts a PRD-first documentation workflow across all packages:

1. Every feature starts with a technology-independent PRD under
   `docs/features/` (numbered `F-01`, `F-02`, …).
2. Targeted implementation designs live under `docs/design/` (numbered
   `D-01`, …) and link their PRD.
3. Cross-feature or cross-package decisions are recorded as ADRs under
   `docs/decisions/` (numbered `ADR-001`, …) and never deleted.
4. Acceptance evidence and differential validation results live under
   `docs/verification/`.

codex-rs is **evidence, not specification**: its behavior may be inspected,
tested against, and distilled into PRDs, but its architecture, crate graph,
and OpenAI coupling are not translated. A behavior enters piko only when the
Feature PRD intentionally keeps it.

## Consequences

- PRDs are the single source of behavior truth; the codex-rs reference stays
  available for differential validation.
- Existing piko behavior documents that conflict with a landed PRD are marked
  `Status: superseded by docs/features/<F-XX>` instead of being deleted.
- Feature work starts with a PRD; no implementation happens before the
  behavior contract is agreed.
