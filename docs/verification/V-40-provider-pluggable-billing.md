# V-40: Provider-pluggable billing verification

> Status: verified
> Feature: [F-29](../features/F-29-provider-pluggable-billing.md)
> Design: [D-41](../design/D-41-provider-pluggable-billing.md)
> Environment: Rust workspace tests on macOS

## Evidence

- Registry unit tests register custom usage adapters and pricing policies and
  dispatch them by target plan ID.
- Standard policy tests cover cached/uncached/write/output token components,
  long-context multipliers, USD and CNY.
- Protocol tests cover component-map accumulation and named usage-unit
  serialization.
- Provider loader tests assert OpenAI and DeepSeek resolve standard billing
  plans with their prior currency and basis behavior.
- `cargo test --workspace` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.

## Result

F-29 acceptance criteria are satisfied. Provider-specific future billing code
can register outside the generic cost middleware and use open billable units
and ledger components.
