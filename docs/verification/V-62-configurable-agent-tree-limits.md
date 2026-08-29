# V-62: Configurable agent-tree limits

> Date: 2026-08-29
> Feature: [F-50](../features/F-50-configurable-agent-tree-limits.md)
> Design: [D-67](../design/D-67-configurable-agent-tree-limits.md)
> Environment: Rust workspace tests on the development host

## Reproduction

```text
cargo test -p piko-protocol -p piko-hostd -p piko-orchd
cargo clippy -p piko-protocol -p piko-orchd -p piko-hostd --all-targets -- -D warnings
```

The runtime integration fixture bootstraps orchd with explicit limits and
attaches a root AgentInstance. It exercises `max-agents = 2` and
`max-depth = 2` through the real `AgentRuntimeApi::create_agent` path.

## Result

- piko-hostd: 180 unit tests passed, plus all hostd integration tests.
- piko-orchd: 146 unit tests and 50 AgentRuntime integration tests passed.
- piko-protocol: 63 unit tests passed.
- Clippy completed with `-D warnings` and no diagnostics.

## Invariants

- The resolved `[agent-runtime]` fields deserialize, merge independently, are
  visible through the host settings namespace, and map to `OrchdConfig.runtime`.
- A count limit of 2 permits root + one child and rejects the next child before
  a durable create command.
- A depth limit of 2 permits root + one child and rejects a grandchild before a
  durable create command.
- Missing protocol fields preserve the 32-agent / 8-level defaults; values
  below one are normalized to the root-only effective limit.
