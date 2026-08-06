# F-13: MCP integration (resources, approval templates, prewarm)

> Status: implemented (F-13/D-23/V-23; TUI surface slice D-24/V-24)
> Priority: P1
> Source evidence: codex-rs `core/src/session/mcp*.rs` (runtime/refresh/
> prewarm), `mcp_tool_call.rs`, `mcp_tool_exposure.rs`, `mcp_resource.rs`,
> `mcp_tool_approval_templates.rs`, digest Block J (Skills, Plugins, Hooks &
> MCP), MCP spec 2024-11-05 (`initialize`, `tools/list`, `tools/call`,
> `resources/list`, `resources/templates/list`, `resources/read`)

## Summary

piko connects to Model Context Protocol (MCP) servers over stdio JSON-RPC 2.0,
discovers their tools, and registers those tools on the agent runtime exactly
like any other tool: they appear in the model-visible catalog, run through the
approval gateway, and are gated by the `mcp` managed feature. This PRD also
lands the remaining F-13 slices: MCP **resources** are discovered at connect
and reachable through a built-in `mcp_resource` tool (list with optional
search filter, read by URI); **approval templates** let operators replace the
generic approval question for specific MCP tools with purpose-written prompt
text; and **prewarm** makes server warm-up explicit — every configured server
connects eagerly at session start under a bounded per-server timeout, so one
slow or broken server can never block the session or the other servers.

## Problem

1. **MCP servers are black-box tools today.** The provider speaks only
   `tools/list` and `tools/call`. A server's *resources* (files, notes,
   database rows, config — the data the tools operate on) are invisible to
   the model, so the agent cannot discover what data a server holds or read
   it directly. codex-rs exposes `mcp_resource`; piko has no equivalent.
2. **Generic approval questions are uninformative for MCP tools.** An MCP
   tool named `create_issue` with JSON args prompts "Run create_issue with
   args {...}?". Operators who want "This creates a GitHub issue in repo X"
   have no settings-native way to say so; they must interpolate the meaning
   themselves every time.
3. **Session start can hang on a misbehaving server.** `initialize_mcp_tools`
   connects every configured server sequentially and waits indefinitely on
   each handshake. One server that never responds blocks the whole session,
   and there is no per-server timeout or documented warm-up contract.

## User journeys

1. An operator configures a filesystem MCP server in settings. At session
   start hostd connects it, discovers its tools **and** resources, and the
   agent can call `mcp_resource` with `{"server": "filesystem"}` to list the
   server's resources (optionally filtering with `"query"`), then
   `{"server": "filesystem", "uri": "file:///tmp/notes.md"}` to read one.
2. The operator maps
   `[mcp.approval-templates] "github/create_issue" = "This creates a GitHub
   issue in the configured repository."`. When the agent calls that MCP tool,
   the approval dialog shows the operator's text instead of the generic
   "Run create_issue with args {...}?" question; every other MCP tool keeps
   the generic question.
3. An operator configures three MCP servers, one of which never responds to
   `initialize`. hostd gives that server `connect-timeout-ms` (default
   10000), logs a warning naming it, and connects the other two normally —
   the session starts with the healthy servers' tools and resources.
4. An operator sets `[features] mcp = false` (F-18). hostd skips all MCP
   server connections and does not register `mcp_resource`; a direct call to
   an MCP tool fails closed with `feature_disabled`.
5. In the TUI, the operator runs `/mcp`. A panel opens listing every
   configured server: connected servers show their tool/resource/template
   counts; servers that failed or timed out at session start show
   `disconnected` with the connect error; servers disabled by the `mcp`
   feature gate show `feature 'mcp' is disabled`.

## In scope

- **Lifecycle (landed, preserved)**: stdio JSON-RPC 2.0 transport,
  `initialize` handshake (protocol 2024-11-05) + `notifications/initialized`,
  `tools/list` discovery, `tools/call` execution with `isError` mapping to a
  non-retryable `mcp_tool_error`, child cleanup on drop, per-server connect
  isolation (one failed server never blocks the others), registration as
  `ToolProvider` + `ToolSet` (`mcp/<name>`), and the `mcp` feature gate.
- **Resources**: at connect, best-effort `resources/list` and
  `resources/templates/list` discovery (a server that does not implement
  them contributes an empty catalog, not a failure); a built-in
  `mcp_resource` tool registered for every session with at least one
  connected server:
  - `{"server": <name>}` — list the server's resources and resource
    templates (URI, name, description, MIME type);
  - `{"server": <name>, "query": <substring>}` — list filtered client-side
    over URI/name/description ("search" behavior; MCP 2024-11-05 has no
    `resources/search` RPC);
  - `{"server": <name>, "uri": <uri>}` — read the resource and return its
    text content.
  Unknown server names and missing URIs fail closed with distinct
  non-retryable errors; blob content is reported as unsupported.
- **Approval templates**: `[mcp.approval-templates]` maps `"server/tool"` or
  bare `"tool"` keys to prompt text; placeholders `{server}`, `{tool}`, and
  `{args}` are substituted. The approval gateway resolves the template
  (exact `server/tool` first, then bare `tool`) — scoped to MCP tools only,
  so a bare `tool` key can never hijack a non-MCP tool's question — and
  carries it on the approval snapshot as `prompt`; TUI and GUI render it in
  place of the generic question. Templates are operator text — they never
  loosen the approval flow or bypass the gateway.
- **Prewarm**: eager connect at session start (existing behavior, now
  explicit and bounded): `[mcp] connect-timeout-ms` (default 10000) with
  per-server `timeout-ms` override; a timed-out server fails closed (logged,
  skipped) without affecting other servers; per-provider tool/resource
  catalog caching.
- **TUI status surface**: a `/mcp` command and status panel backed by the
  neutral `mcp.status` host command (`Command::McpStatus` →
  `CommandResult::McpStatusListed`), showing per-server connection state,
  counts, and errors (including configured-but-disabled servers).
- **Settings/docs**: `McpSettings` (`[mcp]`) added to the settings model with
  merge semantics and documented in `resources/settings.default.toml`;
  `ToolApprovalRequest` carries the executing provider id so templates can be
  resolved per server; `ApprovalSnapshot` carries the optional rendered
  prompt.

## Out of scope

- **Non-stdio transports** (HTTP/SSE MCP servers) — deferred until a consumer
  needs them; `McpServerConfig` remains `command`/`args`/`env`.
- **Paginated `resources/list` cursors** — the first page is used and
  `nextCursor` is ignored; noted for a later slice if a server exposes large
  catalogs.
- **Server restart / mid-session reconnection** — connections are made once
  per session start; a crashed server's tools fail with retryable
  `mcp_error` (existing). Refresh-on-reconnect is deferred.
- **Blob resource content** — text content only; blobs return a non-retryable
  `mcp_resource_blob_unsupported` error.
- **Resource *writes* / prompts** — MCP has no standard resource-write RPC in
  protocol 2024-11-05 and no consumer exists; not added.
- **Lazy connect** — `[mcp] prewarm = false` is not a knob in this slice;
  connections are always made at session start.
- **MCP `tools/search`** (protocol 2025-03-26) — the negotiated protocol is
  2024-11-05; the client-side `query` filter on `mcp_resource` covers the
  search need.

## Behavior and states

### Settings

```toml
[mcp]
connect-timeout-ms = 10000

[mcp.approval-templates]
"github/create_issue" = "This creates a GitHub issue in the configured repository."
"delete_resource" = "This permanently deletes data on the MCP server."

[[mcp-servers]]
name = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
timeout-ms = 5000
```

`McpServerConfig.timeout-ms` overrides the section default for one server.
The `[mcp]` section replaces wholesale across settings layers (same pattern
as `mcp-servers`); template keys resolve as `server/tool` first, bare `tool`
second.

### Connect + prewarm

`initialize_mcp_tools` runs at runner construction (session start) for every
configured server when the `mcp` feature is enabled:

1. spawn the child process;
2. `initialize` handshake + `notifications/initialized`;
3. `tools/list`; then best-effort `resources/list` + `resources/templates/list`;
4. register the provider (`ToolProvider` id = server name) and its
   `mcp/<name>` tool set;
5. after all servers, register `mcp_resource` (provider id `mcp-host`,
   tool set `mcp/resources`) if at least one server connected.

The whole sequence is bounded by the effective timeout (per-server override,
then `[mcp] connect-timeout-ms`, then 10 s). A timeout or any handshake error
fails that server only: warn naming the server and continue. A server with no
`resources/list` support contributes an empty resource catalog. With the
`mcp` feature disabled, no process is spawned and `mcp_resource` is not
registered.

### `mcp_resource` tool

`inputSchema`: `server` (string, required), `uri` (string, optional), `query`
(string, optional).

| Args | Behavior |
|---|---|
| `{server}` | List resources + resource templates (uri/name/description/mimeType) |
| `{server, query}` | Same, filtered client-side over uri/name/description |
| `{server, uri}` | `resources/read`; text content returned |
| `{server, uri, query}` | `uri` wins (read) |

Errors: unknown server → `mcp_resource_server_unknown` (non-retryable);
read with no content or missing URI → `mcp_resource_not_found`
(non-retryable); blob content → `mcp_resource_blob_unsupported`
(non-retryable); transport failure → `mcp_error` (retryable). The tool is
read-only (`read_only: true`, `approval: Never` by default; operators can
tighten it with tool-set policy).

### Approval templates

orchd stamps `provider_id` (the MCP server name) on every
`ToolApprovalRequest`. The hostd gateway resolves the template when building
the `ApprovalSnapshot`:

```text
provider is a configured MCP server   (else: no template)
prompt = templates.get("{provider}/{tool}").or(templates.get("{tool}"))
```

`{server}`, `{tool}`, and `{args}` (compact JSON) are substituted. The prompt
is carried as `ApprovalSnapshot.prompt`; TUI and GUI render it instead of the
generic question. No template → no `prompt` field → current generic rendering
(no behavior change). Templates are never evaluated as policy.

### Failure modes

- Server handshake timeout → warn + skip that server; others connect.
- All servers fail → session starts with no MCP tools (existing behavior),
  `mcp_resource` not registered.
- Template references a tool that does not exist → harmless; the entry only
  matches if the tool appears in the catalog.
- Two servers expose the same tool name with a bare-`tool` template → the
  template applies to both (exact `server/tool` keys disambiguate).

## Acceptance criteria

- [x] `McpServerConfig` deserializes `timeout-ms`; `[mcp]` settings
      deserialize/merge across layers; `resources/settings.default.toml`
      documents `[mcp]` and `[mcp.approval-templates]`.
- [x] A live MCP provider discovers `resources/list` + `resources/templates/list`
      at connect and caches them; a server that errors on `resources/list`
      still connects (empty resource catalog, tools registered).
- [x] `mcp_resource` lists (with optional `query` filter) and reads resources
      through a registered provider; unknown server / missing URI / blob
      content fail closed with the distinct non-retryable errors; the tool's
      executor kind is `mcp` so the F-18 gate covers it.
- [x] Connect is bounded: a server whose handshake exceeds the effective
      timeout is skipped with a warning naming it, and other servers still
      register; a per-server `timeout-ms` overrides the section default.
- [x] The registry stamps `provider_id` on `ToolApprovalRequest`; the gateway
      resolves `server/tool` before bare `tool` and carries the rendered
      prompt on `ApprovalSnapshot`; no matching template leaves `prompt`
      absent.
- [x] TUI and GUI render `ApprovalSnapshot.prompt` when present and keep the
      generic question otherwise (existing rendering unchanged when absent).
- [x] `[features] mcp = false` skips all connections and does not register
      `mcp_resource` (F-18 regression).
- [x] The TUI `/mcp` slash command sends `Command::McpStatus`; the
      `McpStatusListed` result populates the MCP panel and opens it; the
      panel renders connected counts and disconnected errors; configured
      servers disabled by the `mcp` gate are shown with
      `feature 'mcp' is disabled`; no MCP configuration renders the empty
      state.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| How is "search" implemented? | Client-side `query` filter over the discovered resource catalog on `mcp_resource` | MCP 2024-11-05 has no `resources/search` RPC; client-side filtering is honest to the negotiated protocol and testable |
| Where do approval templates live? | `[mcp.approval-templates]` in settings, resolved by hostd into `ApprovalSnapshot.prompt` | hostd stays authoritative for user-visible approval content; templates are presentation text, never policy |
| Template key format | `server/tool` exact, then bare `tool` | Disambiguates same-named tools across servers while keeping a convenient global fallback |
| Resource tool approval | Read-only, `approval: Never` by default; tighten via tool-set policy | Reads are low-risk; operators retain full control through policy |
| Prewarm contract | Eager connect at session start under a bounded per-server timeout | Deterministic warm-up; one bad server cannot block the session or siblings |
| `[mcp]` merge semantics | Section replaces wholesale across layers (like `mcp-servers`) | Consistent with the existing settings-layer pattern; template maps merge per key only within a single file |
| MCP client surface | Neutral `mcp.status` command + TUI `/mcp` panel (read-only snapshot) | hostd stays authoritative for connection state; clients own presentation; management/refresh stays out of scope until a consumer asks |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| MCP server lifecycle + prewarm | kept (adapted) | stdio connect at session start with a bounded per-server timeout and catalog caching; no reconnect/refresh yet (deferred until a consumer) |
| `mcp_resource` tool | kept | built-in read-only tool listing/reading server resources, with a client-side `query` filter standing in for search |
| Tool catalog caching | kept | per-provider cached `tools` + cached resources at connect (registry already caches across catalog builds) |
| `mcp_tool_approval_templates` | kept (adapted) | settings map rendered into `ApprovalSnapshot.prompt` for TUI/GUI; hostd resolves, orchd reports `provider_id` |
| Tool approval tier changes per MCP tool | rejected (deferred) | F-07 approval tiers + tool-set policy already cover this; no new tier mechanism |
| HTTP/SSE transports, server refresh/reconnect | rejected (deferred) | no piko consumer; stdio-only this slice |

## Open questions

1. Whether a later slice should add server reconnect/refresh (tool catalog
   re-list on demand) or paginated `resources/list` cursors; both deferred
   until a consumer exists.

## Reference evidence

- codex-rs `core/src/session/mcp*.rs` (runtime/refresh/prewarm),
  `core/src/mcp/mcp_resource.rs`, `core/src/mcp/mcp_tool_call.rs`,
  `core/src/mcp/mcp_tool_exposure.rs`,
  `core/src/mcp/mcp_tool_approval_templates.rs`.
- MCP spec 2024-11-05 (`initialize`, `tools/list`, `tools/call`,
  `resources/list`, `resources/templates/list`, `resources/read`).
- piko existing F-13 stdio provider (`packages/hostd/src/infra/mcp/`), F-07
  approvals, F-12 safety, F-17/F-19 permissions, F-18 managed features.
- Digest Block J (Skills, Plugins, Hooks & MCP) and roadmap M4 row.
