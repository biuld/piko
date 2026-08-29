# V-64: Agent control plane Slice 1 — interrupt

> Date: 2026-08-29
> Feature: [F-51](../features/F-51-agent-control-plane.md)
> Design: [D-68](../design/D-68-agent-control-plane.md)
> Environment: Rust workspace tests on the development host

## Reproduction

```text
cargo fmt --all -- --check
cargo test -p piko-protocol -p piko-hostd -p piko-tui
cargo clippy --workspace --all-targets -- -D warnings
```

The tests cover wire compatibility, host Turn-backed cancellation authority,
detached forwarding, benign idle races, viewed-agent TUI routing, and separation
of agent-running interruption from the still-legacy host-Turn steer delivery.
They verify only F-51 Slice 1; later slices replace that steer/queue split.

## Result

- piko-protocol: 65 unit tests passed.
- piko-hostd: 182 unit tests plus all hostd integration tests passed.
- piko-tui: 459 unit tests plus terminal E2E and PTY tests passed.
- Workspace clippy completed with `-D warnings` and no clippy diagnostics. Cargo
  reported only the pre-existing future-incompatibility notice for third-party
  `block 0.1.6`.

## Invariants

- Clients interrupt by Session and AgentInstance; Execution identity remains
  private to orchd.
- A Turn-backed interrupt retains hostd's Turn cancelling and terminal
  authority.
- Detached child work is interruptible without a synthetic Turn.
- Idle races return `accepted: false` instead of failing.
- `agent.running` controls interruption while `turn.running` controls steer and
  queue behavior; detached activity cannot masquerade as a host Turn.
- The TUI captures the viewed AgentInstance, so concurrent non-viewed agents
  are not targeted.
