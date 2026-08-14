# ADR-017: Use a bounded blocking boundary for session storage

> Status: accepted
> Date: 2026-08-15

## Context

The schema-v4 journal is intentionally synchronous: a commit holds one ordered
critical section across reducer preflight, append, synchronization, rollover,
and live aggregate advancement. Replay, recovery, fork, and import likewise
perform ordered regular-file operations. Tokio offers async APIs for regular
files, but these operations are still commonly dispatched to blocking workers,
and directory synchronization and filesystem locking remain blocking calls.

Hostd application handlers are async. Calling the synchronous repository and
session-store query ports directly from those handlers can block Tokio runtime
workers during disk latency or long replay/import work. Unbounded use of
`spawn_blocking` would avoid runtime-worker blocking but could allow excessive
cross-session filesystem concurrency.

## Decision

- `piko-session-store` remains a synchronous, Tokio-independent transaction
  boundary using `std::fs`, filesystem locks, and explicit synchronization.
- Hostd's application-facing session repository and session-store ports are
  async.
- Filesystem adapters execute each complete port operation through one shared
  process-wide `StorageBlockingPool`.
- The pool acquires a Tokio semaphore permit before `spawn_blocking` and holds
  that permit inside the blocking closure until the operation completes.
- The initial process-wide concurrency limit is eight operations. Per-session
  locks remain responsible for ordering conflicting work.
- Once a blocking storage operation starts, cancellation of its async waiter
  does not interrupt the synchronous transaction. Replay and stable identities
  reconcile any result whose acknowledgement was not observed.
- Snapshot serialization may continue to use its existing disposable
  background thread; it is not an authoritative journal transaction.

## Consequences

- Tokio runtime workers do not perform session filesystem operations directly.
- Cross-session recovery, import, and fork concurrency is bounded independently
  from Tokio's global blocking-thread capacity.
- Port callers must await storage and must avoid holding unrelated application
  locks longer than required.
- The low-level journal keeps straightforward synchronous lock and crash-safety
  reasoning instead of introducing `.await` inside durability-critical
  sections.
- A future native asynchronous regular-file backend can be evaluated behind
  the adapter without changing journal semantics or application ports.
