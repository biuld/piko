# V-63: Agent delegation modes

> Date: 2026-08-29
> Feature: [F-49](../features/F-49-agent-delegation-modes.md)
> Design: [D-66](../design/D-66-agent-delegation-modes.md)
> Environment: Rust workspace tests on the development host

## Reproduction

```text
cargo test -p piko-protocol -p piko-orchd -p piko-hostd
cargo clippy --workspace --all-targets -- -D warnings
```

The tests cover protocol compatibility, host TOML defaults and built-in mode
assignments, worker catalog and route filtering, direct provider defense,
runtime child-creation admission, and recovery from the durable AgentSpec
snapshot.

## Result

- piko-hostd: 182 unit tests passed, plus all hostd integration tests.
- piko-orchd: 148 unit tests and 53 AgentRuntime integration tests passed.
- piko-protocol: 64 unit tests passed.
- Workspace clippy completed with `-D warnings` and no diagnostics.

## Invariants

- `main`, `general`, and `coder` are supervisors; `scout` is a worker.
- New TOML specs without `kind` default to worker, while pre-F-49 durable
  AgentSpec JSON without `kind` decodes with legacy supervisor behavior.
- Existing `kind = "leaf"` values decode as `worker`, while new serialization
  emits `kind = "worker"`.
- A worker receives no delegation-capable tool definition or route, even when a
  stale/custom tool set declares the delegation provider.
- A worker parent cannot create a child through the runtime API or direct
  multi-agent provider path, and the failed attempt emits no durable create.
- Recovery uses the stored AgentSpec snapshot, so a registry change cannot
  promote an existing worker instance.
