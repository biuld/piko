# V-23: F-13 MCP resources, approval templates, and prewarm acceptance evidence

> Date: 2026-08-04
> Fixture: hostd MCP provider + fixture JSON-RPC servers
> (`infra/mcp/*`, `tests/fixtures/mcp_server.sh`,
> `tests/fixtures/mcp_server_no_resources.sh`), settings model
> (`domain/config/settings.rs`), approval gateway
> (`adapters/turns/orch_runner/approval_gateway.rs`), orchd registry
> (`adapters/tools/registry.rs`, `registry_tests.rs`), client-core + TUI
> prompt propagation; full workspace suite
> Environment: macOS (arm64), `cargo test --workspace`,
> `cargo clippy --workspace --all-targets -- -D warnings`,
> `cargo fmt --all`

## Reproduction

```bash
cargo test -p piko-hostd --lib -- mcp
cargo test -p piko-hostd --lib -- mcp_approval_template_prompt_reaches_the_user_snapshot
cargo test -p piko-hostd --lib -- domain::config::settings
cargo test -p piko-orchd --lib -- approval_request_carries_executing_agent_role
cargo test -p piko-tui --bin piko-tui approval
cargo test --workspace --exclude piko-llmd
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

## Result

All F-13 acceptance criteria pass:

- **Settings**: `mcp_settings_deserialize_from_toml` proves `[mcp]`
  (`connect-timeout-ms`, `[mcp.approval-templates]`) deserializes;
  `mcp_settings_merge_wholesale_across_layers` proves the section replaces
  wholesale across layers and the base survives when the override omits it;
  `test_mcp_server_config_timeout_ms_deserialize` proves the per-server
  `timeout-ms` override; `mcp_defaults_are_documented_in_installed_settings` pins
  `resources/settings.toml`.
- **Provider + resources**: `provider_connects_and_discovers_tools_and_resources`
  runs the real stdio handshake against a fixture JSON-RPC server and proves
  `tools/list`, `resources/list`, and `resources/templates/list` discovery
  plus the client-side `query` filter (match + no-match) and `resources/read`
  text content; `provider_without_resource_support_still_connects` proves a
  server that errors on resources degrades to an empty catalog (tools still
  register); `provider_connect_times_out_fail_closed` proves a hung server is
  skipped after the connect timeout (200 ms) instead of blocking.
- **`mcp_resource` tool**: `mcp_resource_tool_is_gated_as_mcp_and_read_only`
  pins the tool's executor kind to `mcp` (F-18 gate covers it), `approval:
  Never`, and `read_only: true`;
  `mcp_resource_provider_lists_and_reads_and_fails_closed` proves list/read
  routing through the `mcp-host` provider and the distinct non-retryable
  `mcp_resource_server_unknown` error.
- **Approval templates**: `mcp_approval_template_prompt_reaches_the_user_snapshot`
  proves `server/tool` wins over bare `tool`, `{server}`/`{tool}`/`{args}`
  substitution, bare-`tool` keys never match a non-MCP provider, and the
  rendered prompt reaches the pending `ApprovalSnapshot` for TUI;
  `approval_request_carries_executing_agent_role` (extended) proves the
  registry stamps the route `provider_id` on `ToolApprovalRequest`.
- **Clients**: `operator_prompt_replaces_the_generic_question` (TUI) proves
  the prompt replaces the generic question when present, and the absence path
  keeps existing rendering; client-core propagates `prompt` on both the
  snapshot-reconcile and `ApprovalRequested` event paths.
- **Regression**: `cargo test --workspace --exclude piko-llmd` green across
  all packages (hostd lib 116/116 including the new MCP tests, orchd 104/104,
  tui 95/95, client-core, protocol, sandbox). `piko-llmd
  tests/gateway_retry` remains the only in-sandbox failure (it binds a local
  TCP listener, which the managed sandbox denies; verified green unsandboxed
  previously and unrelated to F-13). `cargo clippy --workspace --all-targets
  -- -D warnings` and `cargo fmt --all` clean.

## Invariants

- No `[mcp]` section, no templates, and no MCP servers → no behavior change:
  `mcp_resource` is only registered when at least one server connects, and
  approval snapshots carry no `prompt` unless a template matches an MCP tool.
- Templates are presentation text only: they never change the approval flow,
  grants, guardian, safety, or permission gates.
- Prewarm is bounded per server: one slow/broken server is skipped with a
  warning while siblings connect; the `mcp` feature gate still skips all
  connections and tool registration entirely.
