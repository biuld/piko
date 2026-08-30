# V-64: Agent control plane — lifecycle and control evidence

> Date: 2026-08-31
> Feature: [F-51](../features/F-51-agent-control-plane.md)
> Design: [D-68](../design/D-68-agent-control-plane.md)
> Environment: Rust workspace tests on the development host

## Reproduction

```text
cargo fmt --all -- --check
cargo build -p piko-e2e --bin piko-e2e-hostd
cargo test -p piko-protocol -p piko-session-store -p piko-orchd \
  -p piko-client-core -p piko-tui
PIKO_DEV_SOURCE_ROOT=/Users/biu/Projects/piko \
  cargo test -p piko-hostd --test otel_end_to_end
PIKO_DEV_SOURCE_ROOT=/Users/biu/Projects/piko \
  cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The tests cover canonical AgentInput admission, idempotency, durable queue and
steer dispositions, root-bound ModelStep application, crash interruption
recovery, host observation races, and the TUI steer/queue round trips. The
cross-process TUI fixture is rebuilt with `cargo build -p piko-e2e
--bin piko-e2e-hostd` before the PTY tests so protocol changes cannot be hidden
by a stale helper binary. Desktop remains out of scope.

## Result

- `piko-session-store` journal/recovery cases passed, including torn-tail
  recovery and replay of the published `AgentWorkSnapshot`.
- `piko-tui` unit tests passed; both PTY cases passed after rebuilding the
  scripted host helper: `queue_round_trips_from_tui_through_hostd_to_orchd`
  and `steer_round_trips_from_tui_through_hostd_to_orchd`.
- `piko-hostd` observability E2E passed with `piko.turn.duration_ms`, model-step
  metrics, and a LogRecord carrying `root_input_id`.
- The two-client persistence case
  `two_clients_reconcile_the_same_authoritative_work_projection` passed: an
  independent HostServer (restart) and a second client read identical queued
  input state from the same journal projection.
- Workspace clippy is clean with `-D warnings`; Cargo reports only the existing
  future-incompatibility notice for third-party `block 0.1.6`.
- The complete `PIKO_DEV_SOURCE_ROOT=... cargo test --workspace` run passed;
  the session-store journal group is the long pole (25 tests, about 322 s).

## Invariants

- Clients interrupt by Session and AgentInstance; no Execution identity is
  exposed on the wire.
- Detached and user-origin work share the same AgentInput admission and
  interruption path; no synthetic Turn is required.
- Idle interrupt and terminal races are benign (`accepted: false`) and cannot
  affect a successor root.
- Steers retain the active `root_input_id` captured at admission and are applied
  to one reserved ModelStep; they cannot retarget a later root.
- Queue order and cancellation are journal facts projected by hostd. A restart
  or independent second client therefore sees the same input IDs, dispositions,
  and foreground state.
- The TUI targets the viewed AgentInstance and derives queue/steer feedback only
  from `AgentWorkSnapshot`.
