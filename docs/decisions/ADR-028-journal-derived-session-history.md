# ADR-028: Derive session history from journal facts

> Status: proposed
> Date: 2026-09-04

## Context

F-36 introduced a content-rich trajectory and a web viewer before the durable
Agent work model stabilized. Trajectory events are optional and best-effort,
but the viewer organizes execution primarily from those observations. F-48 and
F-51 subsequently made required AgentInput, ModelStep, message, usage, action,
interrupt, and outcome facts the authoritative model and removed Turn, Run,
and Execution as product scopes.

A historical product surface must not turn the older observational model into
a second authority. It must also preserve the journal's commit/revision order
and atomic boundaries, which cannot be recovered reliably by merging separate
trajectory and message arrays by timestamp.

## Decision

Historical session inspection is derived from required journal facts and uses
the current domain grains:

```text
Session → AgentInstance → root AgentInput causal closure → ModelStep
```

Messages, tool declarations/results, usage, pending actions, interrupts,
reports, transcript ancestry, branches, and terminal outcomes attach through
their persisted identifiers and journal revision. Turn, Run, and Execution are
not reintroduced as display or protocol identities.

Optional trajectory records may enrich a fact-backed item with prompt
assembly, provider metadata, retries, fallbacks, timing, and intermediate tool
status. They never determine authoritative existence, ordering, status,
terminal outcome, accounting, or causality. The product surface labels fact
versus diagnostic provenance independently from relation/detail availability.

The query surface is served by a durable write-time read model in accordance
with F-37. Ordinary history reads do not replay or scan journal segments. Any
join across current state, history, and trajectory projections must use the
same published revision and checksum.

## Consequences

- Session History remains correct and useful when every trajectory record is
  absent.
- The history projector preserves revision and within-commit event order in
  addition to semantic causal indexes.
- The TUI consumes host-authored history DTOs and never reads session files or
  calls the loopback trajectory HTTP server.
- Some diagnostic details remain unavailable by design after a dropped
  optional record.
- Legacy child-agent histories may show parentage without an exact spawning
  work/tool origin. New exact causation requires an explicit durable relation;
  it is never inferred from timestamps.
- F-36 remains the capture contract for diagnostic enrichment, but its
  trajectory is no longer described as the authoritative history model.
