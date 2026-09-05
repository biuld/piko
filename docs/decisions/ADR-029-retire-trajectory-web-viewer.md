# ADR-029: Retire the trajectory web viewer

> Status: accepted
> Date: 2026-09-05
> Supersedes: viewer-surface decision in F-36/D-49
> Related: [ADR-028](ADR-028-journal-derived-session-history.md),
> [F-52](../features/F-52-session-history-inspector.md)

## Context

F-36 shipped a loopback HTTP + SSE web viewer so developers could inspect
best-effort trajectory records before Session History existed. F-52 now
inspects sessions from required journal facts in the TUI and attaches
trajectory only as diagnostic detail. Keeping a second, run-oriented viewer
reintroduces an obsolete inspection model and a live HTTP surface that F-52
explicitly does not follow.

## Decision

Remove the trajectory web viewer, its static assets, SSE endpoints, and the
`[trajectory]` bind/port/enabled settings. Durable trajectory capture,
`trajectory.json`, and history diagnostic enrichment remain.

## Consequences

- Session History is the product inspector for both facts and diagnostics.
- Existing `[trajectory]` keys in user settings.toml are ignored.
- Capture stays best-effort and optional; dropped-record counters remain
  process-local and are not a live UI feed.
- A later desktop inspector may consume host history DTOs; it must not
  revive the loopback HTTP viewer.
