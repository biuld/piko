# V-24: F-13 TUI MCP status surface acceptance evidence

> Date: 2026-08-04
> Fixture: protocol command round-trip (`packages/protocol/src/command.rs`),
> hostd runner status port (`adapters/agent_runner/orch_runner/tests.rs`), TUI
> slash/event/panel (`packages/tui/src/app/{tests,command,dispatch,event}.rs`,
> `features/mcp/mod.rs`); full workspace suite
> Environment: macOS (arm64), `cargo test --workspace`,
> `cargo clippy --workspace --all-targets -- -D warnings`,
> `cargo fmt --all`

## Reproduction

```bash
cargo test -p piko-protocol --lib -- mcp
cargo test -p piko-hostd --lib -- mcp_statuses_reports_configured_servers
cargo test -p piko-hostd --lib -- domain::commands
cargo test -p piko-tui --bin piko-tui mcp
cargo test --workspace --exclude piko-llmd
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

## Result

All F-13 TUI-surface acceptance criteria pass:

- **Protocol**: `mcp_status_and_server_info_round_trip` proves
  `Command::McpStatus` (wire `mcp_status`) deserializes and `McpServerInfo`
  round-trips with camelCase fields, skips absent `error`, and serializes a
  failed server's error string.
- **Hostd**: `mcp_statuses_reports_configured_servers` proves the runner
  reports a configured server that failed to connect (`connected: false`,
  error present) through the `AgentRunRunner::mcp_statuses` port;
  `catalog_ids_are_stable_and_unique` asserts the neutral `mcp.status` id is
  advertised. The dispatch arm mirrors the `process.list` path
  (`Command::McpStatus` → `CommandResult::McpStatusListed`).
- **TUI**: `mcp_slash_command_sends_mcp_status` proves `/mcp` is
  slash-addressable once hostd advertises `mcp.status` and sends
  `Command::McpStatus`; `mcp_status_listed_event_opens_panel` proves the
  result populates the panel (1 connected / 2 total), opens `AppMode::Mcp`,
  sets the status line, and posts a notification naming the connected
  server; `connected_server_line_renders_counts`,
  `disconnected_server_line_shows_error`, and `panel_tracks_connected_count`
  cover the panel formatting and count projection; the merged slash catalog
  lists `/mcp`.
- **Regression**: `cargo test --workspace --exclude piko-llmd` green across
  all packages (hostd lib 117/117, orchd 104/104, tui 100/100, protocol
  24/24, client-core, sandbox). `piko-llmd tests/gateway_retry` remains
  the only in-sandbox failure (binds a local TCP listener; unrelated to
  F-13). `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo fmt --all` clean.

## Invariants

- Status is a read-only snapshot: `/mcp` never connects, reconnects, or
  mutates MCP state; hostd remains the only authority for connection state.
- No MCP configuration and a disabled `mcp` feature are both visible states
  (empty panel / `feature 'mcp' is disabled`), never crashes.
- The panel is TUI-local presentation; the wire carries only the neutral
  `McpServerInfo` DTO.
