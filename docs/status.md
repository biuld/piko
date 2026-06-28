# piko Current Status

The current host runtime direction is:

```text
host-tui <-> JSON-lines <-> hostd -> orchd -> tools/sandbox/model gateway
```

For hostd planning and known risks, use:

- [hostd Global Plan](architecture/hostd-global-plan.md)
- [TUI / Host Boundary](architecture/tui-host-boundary.md)
- [hostd / orchd Runtime Boundary Correction](architecture/hostd-orchd-runtime-boundary.md)

## Current Source Of Truth

| Area | Source |
|---|---|
| Host protocol commands/acks | `packages/protocol/src/command.rs` |
| Host protocol events/snapshots | `packages/protocol/src/event.rs` |
| hostd protocol re-export | `packages/hostd/src/api.rs` |
| TUI wire mirror | `packages/host-tui/src/client/hostd-protocol.ts` |
| hostd protocol server | `packages/hostd/src/server/mod.rs` |
| hostd JSON-lines transport | `packages/hostd/src/server/transport.rs` |
| hostd turn adapter | `packages/hostd/src/turn/runner.rs` |
| hostd runtime architecture | `packages/hostd/docs/runtime-architecture.md` |
| hostd session storage | `packages/hostd/src/session/` |
| orchd runtime core | `packages/orchd/src/orchestrator/core.rs` |

## Current Risk Summary

The old migration question was "which TypeScript host-runtime features have not
been ported yet?" That is no longer the useful framing. Many broad features now
have Rust paths, but several core contracts need hardening:

| Priority | Risk |
|---|---|
| P0 | Cancellation state and orchd task cancellation need a clear end-to-end contract. |
| P0 | `TurnSupervisor` does not yet own explicit active-turn handles and settlement state. |
| P0 | Event application/persistence still lives in command handlers rather than a dedicated event sink. |
| P1 | JSON-lines `CommandAck` currently acknowledges parsing/dispatch, not semantic command success. |
| P1 | `events_resume(after_seq)` currently behaves as snapshot recovery, not event replay. |
| P1 | Queue semantics need to distinguish delivered steer messages from pending prompts. |
| P1 | Rust/TypeScript protocol parity is manual and should be generated or tested. |

## Recently Cleaned Up

The following outdated reports were removed because their conclusions were no
longer accurate:

- `hostd-gap-analysis.md`
- `review-audit.md`
- `docs/hostd-migration-status.md`
- `docs/task-status-by-event.md`

Older design docs may still be useful for intent, but they are not current
status documents unless they explicitly point to the global plan.
