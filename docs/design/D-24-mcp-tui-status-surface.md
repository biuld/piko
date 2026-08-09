# D-24: TUI MCP status surface (`/mcp` command + panel)

> Status: accepted
> Implements: [F-13](../features/F-13-mcp-integration.md) (TUI surface slice)

## Goal

Give the TUI a `/mcp` command and a status panel that shows every configured
MCP server: connection state, tool/resource/template counts, and the connect
error when a server failed or timed out at session start (or is disabled by
the F-18 `mcp` feature gate).

## Constraints and non-goals

- hostd stays authoritative for MCP connection state: the runner stores the
  status snapshot produced by `initialize_mcp_tools`; the protocol only
  carries neutral DTOs.
- The TUI owns presentation: slash name (`/mcp`), `AppMode::Mcp`, and the
  panel rendering are all TUI-local; hostd advertises the neutral id
  `mcp.status` in its command catalog.
- Non-goals: MCP *management* (connect/reconnect/refresh from the client),
  live streaming status updates, showing the model-visible tool
  catalog itself (covered by `/status` tool state).

## Proposed design

### 1. Protocol (`piko-protocol`)

- `command::McpServerInfo` (`name`, `connected`, `tool_count`,
  `resource_count`, `template_count`, optional `error`) — one entry per
  configured server.
- `Command::McpStatus { command_id }` (wire `mcp_status`).
- `CommandResult::McpStatusListed { servers, timestamp }`.

### 2. Hostd (`piko-hostd`)

- `infra/mcp/init.rs`: `initialize_mcp_tools` now returns
  `Vec<McpServerInfo>` covering **all** configured servers — connected
  entries carry the discovered counts; failed/timed-out entries carry the
  connect error.
- `OrchAgentRunRunner` stores `mcp_server_statuses`; when the `mcp` feature
  is disabled (F-18), configured servers are still reported with
  `connected: false` and error `feature 'mcp' is disabled` so the panel is
  honest instead of empty.
- `AgentRunRunner::mcp_statuses()` port method (default empty) implemented
  on the runner.
- `protocol/dispatch.rs`: `Command::McpStatus` → `McpStatusListed`.
- `domain/commands.rs`: neutral catalog id `mcp.status` ("MCP servers",
  Runtime group).

### 3. TUI (`piko-tui`)

- `command.rs`: `HOST_SLASH_TABLE` maps `/mcp` → `mcp.status`;
  `action_for_host_command` routes it to `SlashAction::ListMcpStatus`.
- `dispatch.rs`: `ListMcpStatus` sends `Command::McpStatus` and closes any
  open surface (same pattern as `/ps`).
- `event.rs`: `McpStatusListed` stores statuses into `McpPanel`, opens
  `AppMode::Mcp`, sets the status line ("N MCP server(s) connected"), and
  posts a notification naming the connected servers.
- `features/mcp/mod.rs`: read-only `McpPanel` rendering one row per server
  (`name  connected  N tools · M resources · K templates`, or
  `name  disconnected: <error>`); empty state text when no servers are
  configured.
- `app/mod.rs`: `AppMode::Mcp` (Partial placement) + panel field.
- `render/mod.rs`: `AppMode::Mcp` renders the panel as a partial overlay.
- `input/focus.rs`: `AppMode::Mcp` uses the info-panel key handling
  (Esc/q closes), same as `/status`.
- `help`: `/mcp` listed in the Surfaces section; the merged command catalog
  picks up the host descriptor automatically once hostd advertises it.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | `McpServerInfo`, `Command::McpStatus`, `CommandResult::McpStatusListed` |
| `piko-hostd` | status-producing `initialize_mcp_tools`, runner snapshot, port method, dispatch, catalog id |
| `piko-tui` | `/mcp` slash, `McpPanel`, `AppMode::Mcp`, event handling, help |
| others | none |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Failed/timed-out servers appear in the panel as `disconnected` with the
  connect error (fail-closed visibility, never a crash).
- No MCP configuration → panel shows the empty state; the command still
  returns an empty list.
- `mcp` feature disabled → configured servers show
  `feature 'mcp' is disabled` instead of vanishing.
- Command cancellation/timeout follow the existing command-response path;
  a missing response leaves the panel with the previous snapshot.

## Verification

- Protocol serde round-trip for `McpStatus` + `McpServerInfo` (connected and
  failed variants).
- Hostd: `mcp_statuses_reports_configured_servers` proves the runner reports
  a failed configured server with its error; catalog test asserts the
  `mcp.status` id.
- TUI: `/mcp` slash sends `Command::McpStatus`; `McpStatusListed` event
  stores statuses, opens `AppMode::Mcp`, and sets status/notification;
  panel line formatting tests (connected counts, disconnected error).
- Full workspace suite + clippy + fmt.

## Alternatives considered

- **Fold into `/status`** — rejected: `/status` is a compact diagnostic
  summary; a dedicated panel keeps the per-server rows readable and matches
  the requested command + panel shape.
- **Live status events (push)** — rejected: status is a session-start
  snapshot; a pull command is simpler and consistent with `/ps`.
- **Expose raw provider state over a custom channel** — rejected: the
  standard command-result path carries the same data with no new plumbing.

## Rollout

One slice landing with the F-13 PRD update, this design, implementation,
verification evidence (V-24), and roadmap update.
