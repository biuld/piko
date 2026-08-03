# D-23: MCP resources, approval templates, and prewarm

> Status: accepted
> Implements: [F-13](../features/F-13-mcp-integration.md)

## Goal

Complete the F-13 MCP integration slice: expose MCP server *resources*
(list/search/read) through a built-in `mcp_resource` tool, let operators
replace the generic approval question for specific MCP tools with
`[mcp.approval-templates]` prompt text, and make session-start warm-up
explicit and bounded (`[mcp] connect-timeout-ms`, per-server `timeout-ms`)
so one slow or broken server can never block the session or its siblings.

## Constraints and non-goals

- hostd stays authoritative: settings (`McpSettings`), server connections,
  resource discovery, and approval-template resolution live in hostd. orchd
  only reports which provider a tool call routes to (`provider_id` on the
  approval request).
- Protocol stays MCP 2024-11-05 (`resources/list`, `resources/templates/list`,
  `resources/read`; no `resources/search` RPC). "Search" is a client-side
  `query` filter over the discovered catalog.
- Templates are presentation text: they change what the user reads in the
  approval dialog, never the approval flow or its decisions.
- Non-goals: HTTP/SSE transports, reconnect/refresh, pagination cursors,
  blob resource content, resource writes, lazy connect, `tools/search`.

## Proposed design

### 1. Settings model (`piko-hostd`)

`HostSettings` gains `mcp: Option<McpSettings>` (kebab-case section `[mcp]`)
and `McpServerConfig` gains `timeout_ms: Option<u64>`:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct McpSettings {
    pub connect_timeout_ms: Option<u64>,
    #[serde(default)]
    pub approval_templates: HashMap<String, String>,
}

pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}
```

`McpSettings` merges wholesale across layers (override replaces the base
section), matching the existing `mcp-servers` merge. Effective per-server
connect timeout = `config.timeout_ms` → `mcp.connect_timeout_ms` → 10000 ms.

### 2. Provider resources (`piko-hostd` `infra/mcp/provider.rs`)

`McpProvider` gains a cached resource catalog and read support:

```rust
#[derive(Debug, Clone)]
pub(super) struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: Option<String>,
}

pub struct McpProvider {
    // existing fields…
    resources: Vec<McpResource>,
    templates: Vec<McpResourceTemplate>,
}
```

- After `tools/list`, connect runs best-effort `resources/list` and
  `resources/templates/list`. A server that errors on either contributes an
  empty catalog and still connects (tools work; resources are absent).
- `McpResource` normalizes the spec `{uri, name, description?, mimeType?}`;
  `McpResourceTemplate` normalizes `{uriTemplate, name, description?,
  mimeType?}`. Pagination cursors are ignored (first page only; documented).
- Public methods: `list_resources(query) -> serde_json::Value` (resources +
  templates, filtered client-side over uri/name/description when `query` is
  non-empty) and `read_resource(uri) -> Result<serde_json::Value, String>`
  (`resources/read`, text content only; blob → error).
- `McpProvider::connect` wraps everything after spawn in
  `tokio::time::timeout(effective_timeout, …)`; on timeout the child dies via
  `kill_on_drop` and connect returns an error naming the server.

### 3. `mcp_resource` tool (`piko-hostd` `infra/mcp/`)

New `McpResourceProvider` implements `ToolProvider` with id `mcp-host`:

```rust
pub struct McpResourceProvider {
    servers: HashMap<String, Arc<McpProvider>>,
}
```

`initialize_mcp_tools(configs, timeout, runtime)` collects successfully
connected providers into that map, registers each server provider + its
`mcp/<name>` tool set as today, then — when at least one server connected —
registers `McpResourceProvider` and a `mcp/resources` tool set referencing
`mcp_resource` by provider tool name.

Tool definition:

- `name`: `mcp_resource`, `executor.kind`: `"mcp"` (so F-18's `mcp` gate
  covers it), `executor.target`: `"mcp-host"`;
- `inputSchema`: `{server: string, uri?: string, query?: string}` with
  `server` required;
- `approval`: `Never`, `metadata.read_only: true` (read-only resource
  access; operators can tighten via tool-set policy);
- `capabilities`: `[Network]`.

Execution:

| Args | Route |
|---|---|
| `{server}` / `{server, query}` | `list_resources(query)` |
| `{server, uri}` | `read_resource(uri)` (uri wins over query) |
| unknown server | `mcp_resource_server_unknown` (non-retryable) |
| read with no text content | `mcp_resource_not_found` (non-retryable) |
| blob content | `mcp_resource_blob_unsupported` (non-retryable) |
| transport failure | `mcp_error` (retryable) |

### 4. Provider identity on approvals (`piko-orchd-api`, `piko-orchd`)

`ToolApprovalRequest` gains `provider_id: Option<String>`
(serde `providerId`). The registry sets it from the resolved catalog route's
`provider_id` when building the request (same spot that sets `agent_role`).
For MCP tools this is the server name; for every other tool it is the owning
provider id. Absent for callers that never set it (tests, older fixtures) —
template resolution degrades to bare `tool` keys.

### 5. Approval templates (`piko-hostd`)

`OrchAgentRunRunner` gains `mcp_approval_templates: HashMap<String, String>`
populated from `mcp_settings.approval_templates` at construction. In
`request_tool_approval`, when building the `ApprovalSnapshot`:

```text
key  = format!("{provider}/{tool}")  when provider_id present, else "{tool}"
text = templates.get(&format!("{provider}/{tool}"))
           .or_else(|| templates.get(tool))
```

The chosen template is rendered with `{server}`, `{tool}`, `{args}` (compact
JSON) substituted and carried as `ApprovalSnapshot.prompt` (`Option<String>`).
No match → `prompt` absent → existing generic rendering, unchanged.

### 6. Wire + client rendering (`piko-protocol`, `client-core`, `tui`, `gui`)

- `ApprovalSnapshot` gains `prompt: Option<String>` (camelCase `prompt`,
  skipped when absent).
- `client-core::state::PendingApproval` gains `prompt: Option<String>`,
  populated from `ApprovalSnapshot.prompt` in both the snapshot reconcile
  (`state.rs`) and the `ApprovalRequested` event path (`update/host/events.rs`).
- TUI `PendingApproval` gains `prompt`; `approval_question` returns the
  rendered prompt verbatim when present (tests updated).
- GUI `render_approval_body` renders the prompt text block above the
  arguments when present.

### 7. Feature gate + docs

- `mcp_resource`'s `executor.kind == "mcp"` means the existing F-18 gate
  (`disabled_feature_for_tool_name`) covers it without new code; when `mcp`
  is disabled hostd never connects or registers the tool.
- `resources/settings.default.toml` documents `[mcp]`, `[mcp.approval-templates]`,
  and `timeout-ms` on `mcp-servers`.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | `ApprovalSnapshot.prompt`; no behavior change for existing consumers (skipped when absent) |
| `piko-orchd-api` | `ToolApprovalRequest.provider_id` |
| `piko-orchd` | registry stamps `provider_id` on approval requests |
| `piko-hostd` | `McpSettings` + `timeout_ms`; `McpProvider` resources + connect timeout; `McpResourceProvider` + `mcp_resource`; gateway template resolution + snapshot prompt; `new_with_mcp` threading |
| `piko-client-core` | `PendingApproval.prompt` propagation |
| `piko-tui` | approval question renders prompt when present |
| `piko-gui` | approval body renders prompt when present |
| `piko-llmd` / `piko-sandbox` | none |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Connect timeout / handshake error: that server is skipped with a warning;
  already-registered servers and their resources are unaffected. Child
  processes are `kill_on_drop`, so a timed-out server is reaped when the
  provider drops.
- `resources/list` failure: empty catalog for that server; `mcp_resource`
  still lists tools-independent data (or an empty array) and never fails the
  session.
- Resource read failure: non-retryable distinct errors for unknown
  server/URI/blob; retryable `mcp_error` only for transport faults.
- Template substitution is best-effort; malformed placeholders are left as
  literal text (no user-facing failure).
- Approval cancellation/timeout semantics are unchanged (F-07); the prompt
  is presentational only.

## Verification

- Settings: `[mcp]` + `timeout-ms` deserialize/merge; defaults documented.
- Provider: fake MCP server script covering `resources/list`,
  `resources/templates/list`, `resources/read`, and a server that errors on
  `resources/list` (still connects); connect-timeout skip with other servers
  registering.
- Registry: `provider_id` stamped on the approval request.
- Gateway: `server/tool` beats bare `tool`; bare `tool` fallback; no template
  → no prompt; `{args}`/`{tool}` substitution.
- Clients: TUI question uses the prompt; absent prompt keeps generic text.
- Gate: `mcp_resource` routes to the `mcp` feature (executor kind).
- Full workspace suite + clippy + fmt.

## Alternatives considered

- **Namespace MCP tools as `mcp/<server>/<tool>`** — rejected: breaking
  change to existing tool names and catalog routes; `provider_id` on the
  approval request achieves unambiguous template keys without renaming.
- **New `mcp.search` RPC via protocol 2025-03-26** — rejected: protocol
  remains 2024-11-05; client-side filtering covers the need.
- **Templates evaluated in orchd** — rejected: hostd is authoritative for
  user-visible approval content; orchd only reports provider identity.
- **Lazy connect (`prewarm = false`)** — rejected: adds a second connection
  path for no consumer; eager bounded connect is simpler and deterministic.

## Rollout

One vertical slice landing with the PRD, this design, implementation,
verification evidence (V-23), roadmap/digest/settings-default updates, and
`cargo fmt`/`clippy`/`test` green.
