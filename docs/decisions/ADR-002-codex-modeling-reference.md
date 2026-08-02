# ADR-002: codex-rs is a modeling reference, not a parity target

> Status: accepted
> Date: 2026-08-02

## Context

ADR-001 established that codex-rs is behavior evidence and that piko follows a
PRD-first workflow. F-01 showed the line between "behavior worth keeping" and
"codex-shaped implementation detail" is not always clear: some codex
capabilities (turn lifecycle, input admission, abort markers) map cleanly onto
piko, while others (a central task taxonomy, a new durable task surface)
reflect codex-rs's own architecture and do not fit piko's layering. piko's
goal is a general-purpose agent, not a codex replica.

## Decision

- codex-rs is a **modeling reference**: reference its core design and build
  piko's own modeling. Details do not need 1:1 parity with codex-rs.
- Distill behavior into PRDs as before; do not translate codex-rs
  architecture or coupling.
- When codex-rs modeling conflicts with piko, or a design point is unclear,
  **discuss it with the user before choosing** — do not silently port the
  codex-rs shape.
- Resolve conflicts with **industry best practice as the benchmark** and keep
  the design that is best for piko, including when that means diverging from
  codex-rs.

## Consequences

- Differential validation applies to behaviors piko intentionally keeps, not
  to detail parity.
- Codex-shaped mechanisms without a piko consumer are rejected or deferred
  until a piko feature defines them.
- Fusion decisions (kept / adapted / rejected) are surfaced in PRDs and
  designs for review, and agents pause for user discussion on conflicts.
