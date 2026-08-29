# D-33: Multi-agent model tool surface

> Status: accepted (implemented for orchd tool surface slices A–C)
> Implements: [F-21](../features/F-21-multi-agent-tool-surface.md)
> Extended by: [D-66](D-66-agent-delegation-modes.md) for supervisor/worker
> capability filtering and runtime child-creation enforcement
> Decisions: [ADR-002](../decisions/ADR-002-codex-modeling-reference.md) (codex is
> modeling reference only); product decisions live in F-21

## Goal

Ship the F-21 model-facing multi-agent tool API:

1. First-class **AgentSpec catalog** discovery for the model.
2. **Spec vs instance** vocabulary in every multi-agent tool description and
   result shape.
3. Spawn **default + invalid-id recovery** without changing AgentRuntime tree
   or durability semantics.
4. One model-facing messaging tool **`message_agent`** with schema field
   **`when: "queue" | "steer"`** (default `queue`), replacing dual
   `followup_task` / `send_agent_message` in the catalog.

Vertical slice stays inside orchd tool discovery/execution, with a thin
registry list API on the runtime services the provider already shares.

## Constraints and non-goals

- **No** session schema, durable command, or mailbox redesign (F-10/D-10).
- **No** F-19/F-20 behavior change.
- **No** second agent registry: catalog reads the same `ExecutionServices`
  map hostd already seeds via `register_agent`.
- Tool result errors remain `ToolExecResult { ok: false, error }` so transcript
  and TUI tool cards keep working; enrich **message/value** for recovery, not
  a new protocol event family.
- File size: keep `multi_agent_provider.rs` under the 500-line ceiling by
  splitting catalog/spawn helpers if the rewrite exceeds ~400 lines.
- codex-rs multi_agents is evidence for collaboration verbs only—not schema
  parity (ADR-002).

## Proposed design

### 1. Ownership

```text
hostd load_agents(cwd)
  → register_agent(spec) × N into orchd ExecutionServices
       │
       ├─ create_agent resolves agent_spec_id (existing)
       └─ MultiAgentToolProvider
            ├─ list_agent_specs  → list from ExecutionServices
            ├─ spawn_*           → resolve default / validate / create+run
            └─ list_agents, message_agent, … (F-10 runtime, F-21 surface)
```

| Concern | Owner |
|---------|--------|
| Spec files / built-ins | hostd agent loader (unchanged) |
| In-process registry | `ExecutionServices` agent_specs map (unchanged source) |
| Model tool defs + execute | `MultiAgentToolProvider` |
| Instance tree / authorize | `AgentRuntime` (unchanged) |
| Hostd `AgentSpecList` command | unchanged TUI path; optional later alignment of field set |

### 2. Registry list API (orchd)

Add a read-only list on the path the tool provider can call:

```text
ExecutionServices::list_agent_specs() -> Vec<AgentSpec>
  // snapshot, sorted by id ascending for stable model output
```

Expose to the tool layer either:

- **Preferred:** `AgentRuntimeApi::list_agent_specs()` that delegates to
  `execution.services().list_agent_specs()`, or
- **Minimal:** give `MultiAgentToolProvider` an `Arc<ExecutionServices>` (or a
  narrow `AgentSpecCatalog` trait) in addition to `AgentRuntimeApi`.

**Decision:** add `list_agent_specs` to `AgentRuntimeApi` (and `AgentRuntime`
impl) so the provider keeps a single dependency and tests can stub one trait.

No hostd wire change required for the model path: registry is already loaded
into orchd before tools run.

### 3. Model tool set (names and roles)

| Tool name | Role | Change vs today |
|-----------|------|-----------------|
| `list_agent_specs` | **New.** Spawnable templates | — |
| `list_agents` | Live instances only | Description + field emphasis |
| `spawn_agent` | Attached spawn | Optional `agent_spec_id`, default, recovery errors |
| `spawn_agent_detached` | Detached spawn | Same identity rules |
| `message_agent` | **New unified messaging** | Replaces catalog exposure of `followup_task` + `send_agent_message` |
| `interrupt_agent` | Cancel active run | Description only |
| `wait_agent` | Bounded wait | Description only |
| `collect_agent_reports` | Consume inbox | Description only |
| `close_agent` / `reopen_agent` | Lifecycle | Description only |

**Not in model catalog after F-21:** `followup_task`, `send_agent_message`
(removed from `discover()`, not left as aliases in the default tool set).

### 4. `list_agent_specs`

**Discover schema:** empty object properties, no required fields.

**Description (normative intent):**

> List spawnable agent **templates** (AgentSpec registry). Use `id` as
> `agent_spec_id` when calling `spawn_agent` or `spawn_agent_detached`.
> This is not the live agent tree; for live instances call `list_agents`.

**Success value:**

```json
{
  "specs": [
    {
      "id": "coder",
      "name": "Coder",
      "role": "developer",
      "description": "Expert software engineer…"
    }
  ],
  "default_spawn_spec_id": "general"
}
```

- `specs` sorted by `id`.
- Omit `description` key when null/empty (or send JSON null—prefer omit).
- `default_spawn_spec_id`: `"general"` if that id is present in the registry;
  otherwise `null` / omit (spawn without id then fails with recovery list).

**Empty registry:** `{ "specs": [], "default_spawn_spec_id": null }` — not an
error.

### 5. `list_agents` (live)

Keep F-10 runtime call. Update description:

> List **live** AgentInstances in this session (parents before children).
> Fields use `agent_instance_id` for messaging and `agent_spec_id` only as the
> template that instance was created from. To discover spawnable templates,
> call `list_agent_specs`.

Result shape (clarify keys; keep existing data):

```json
{
  "agents": [
    {
      "agent_instance_id": "agent_…",
      "agent_spec_id": "coder",
      "parent_agent_instance_id": "…",
      "lifecycle": "…",
      "activity": "idle|running|…",
      "unread_report_count": 0,
      "latest_report_summary": null
    }
  ]
}
```

Do not rename the tool in slice 1 (compatibility). Alias optional later.

### 5b. `message_agent` (unified send / followup)

**Replace** model-visible `followup_task` and `send_agent_message` with one tool.

#### Schema (JSON Schema for ToolDef)

```json
{
  "type": "object",
  "properties": {
    "agent_instance_id": {
      "type": "string",
      "description": "Live AgentInstance id from list_agents (not a template/spec id)."
    },
    "message": {
      "type": "string",
      "description": "Task text (when=queue) or mid-turn steer text (when=steer)."
    },
    "when": {
      "type": "string",
      "enum": ["queue", "steer"],
      "description": "queue (default): start a turn if idle, or durable-queue if busy. steer: inject into the active turn only; fails if the agent is idle."
    }
  },
  "required": ["agent_instance_id", "message"]
}
```

- `when` is **not** in `required`; omit → treat as `"queue"`.
- Invalid `when` string → fail closed (`invalid_argument` / InputRejected).

#### Description (normative intent)

> Send work to a **live** child AgentInstance. Default `when=queue`: if idle,
> start a new turn; if busy, queue the task until the current turn finishes.
> Use `when=steer` only to redirect an **already running** turn; if the agent
> is idle, the call fails — use queue instead. Get ids from `list_agents`.

#### Delivery mapping (runtime unchanged)

| Resolved `when` | Agent busy? | `AgentInputDelivery` | Notes |
|-----------------|-------------|----------------------|--------|
| `queue` | no | `FollowUp` | Starts execution (same idle path as today followup) |
| `queue` | yes | `FollowUp` | Durable enqueue (F-10) |
| `steer` | yes | `SteerActive` | Prefer explicit steer; do **not** use `Auto` (Auto also starts when idle) |
| `steer` | no | — | **Do not call runtime**; return tool error `agent_not_running` |

Provider must **preflight idle+steer** using a snapshot (e.g. existing status /
list path or a lightweight snapshot read) before `send_agent_input`, so idle
steer never becomes a silent new turn.

#### Success value

```json
{
  "agent_instance_id": "agent_…",
  "when": "queue",
  "disposition": "accepted"
}
```

- `when`: resolved value (`queue` or `steer`).
- `disposition`: from `AgentInputReceipt` (`accepted` | `queued`, etc.). For
  steer success use receipt disposition or normalize to `"steered"` if the
  receipt already distinguishes—prefer pass-through of runtime disposition
  and rely on `when` for mode.

#### Errors (model-facing codes)

| Case | `error.code` (suggested) | Message intent |
|------|--------------------------|----------------|
| Missing instance / message | `invalid_argument` | field required |
| Unknown instance | existing / `agent_not_found` | as today |
| Unauthorized | existing | as today |
| `when=steer` while idle | `agent_not_running` | use when=queue |
| Invalid `when` | `invalid_argument` | must be queue or steer |

#### Migration

| Old tool | New usage |
|----------|-----------|
| `followup_task` | `message_agent` with omit/`when=queue` |
| `send_agent_message` while busy | `message_agent` with `when=steer` |
| `send_agent_message` while idle | was Auto-start; now **`when=queue`** (do not map old Auto idle to steer) |

No durable transcript migration: old tool call names in history stay as
historical strings; only the live catalog changes.

### 6. Spawn resolution

```text
resolve_spawn_spec_id(args, catalog):
  if args.agent_spec_id present and non-empty:
    id = trim(args.agent_spec_id)
  else if catalog contains "general":
    id = "general"
  else:
    return MissingSpec { available_ids }
  if catalog has id:
    return Ok(id)
  return UnknownSpec { id, available_ids }
```

- `available_ids`: sorted list of all registry ids (same as list_agent_specs).
- Required JSON schema: `prompt` required; `agent_spec_id` **not** in
  `required` array (F-21 defaulting).

**Success value (attached)** — extend existing report JSON:

```json
{
  "agent_instance_id": "…",
  "agent_spec_id": "coder",
  "attached": true,
  /* existing report fields from report_value */
}
```

**Success value (detached):**

```json
{
  "agent_instance_id": "…",
  "agent_spec_id": "coder",
  "attached": false,
  "status": "accepted"
}
```

### 7. Error recovery for the model

Today `AgentSpecNotFound` becomes:

```json
{ "ok": false, "error": { "code": "agent_runtime_error", "message": "agent specification not found" } }
```

F-21 needs recoverable content. Prefer **structured tool error message** plus
optional `value` when the framework allows non-ok with value; if `ToolExecResult`
forbids value on failure, pack recovery into `message` as stable JSON text:

```json
{
  "ok": false,
  "error": {
    "code": "agent_spec_not_found",
    "message": "Unknown agent_spec_id \"agents/main\". Valid ids: coder, general, main, scout. Call list_agent_specs for details.",
    "retryable": false
  }
}
```

Similarly for missing id when no default:

```text
code: agent_spec_required
message: agent_spec_id is required (no default). Valid ids: … Call list_agent_specs.
```

**Implementation detail:**

- Map `AgentApiError::AgentSpecNotFound` in the provider **before** generic
  mapping when the spawn path already computed `available_ids`.
- Prefer dedicated codes: `agent_spec_not_found`, `agent_spec_required` (not
  only `agent_runtime_error`) so models and tests can branch.

Optional slice-2: put `available_spec_ids: string[]` in a machine-readable
extension field if tool error DTO gains `details`—not required if message
lists ids.

### 8. Tool descriptions (rewrite pack)

Centralize description strings next to `tools()` so discover() stays pure.

Minimum description contracts (paraphrase OK; meaning fixed):

| Tool | Must say |
|------|----------|
| `list_agent_specs` | templates for spawn; not live tree |
| `list_agents` | live instances; use instance id for message_agent/wait |
| `spawn_agent` | waits for first report; `agent_spec_id` from list_agent_specs; default general when omitted |
| `spawn_agent_detached` | returns immediately; reports via inbox / wait |
| `message_agent` | single messaging tool; default when=queue; steer only if running |
| others | instance id from list_agents |

Also set multi_agent **ToolSet** description to mention catalog discovery
(not only “creation, reuse, and status”).

### 9. Schema property descriptions

JSON Schema for spawn:

```json
{
  "type": "object",
  "properties": {
    "agent_spec_id": {
      "type": "string",
      "description": "Registry template id from list_agent_specs (e.g. coder, scout, general). Not an agent_instance_id."
    },
    "prompt": {
      "type": "string",
      "description": "Initial task for the child agent."
    }
  },
  "required": ["prompt"]
}
```

### 10. Optional prompt injection (slice 2 / open question)

F-21 allows additive catalog hints in prompt assembly. **Slice 1 does not
block on this.**

If implemented later:

- hostd assembler reads the same registered specs (or re-load agents for cwd).
- Inject a short bullet list under a fixed fragment title, e.g. “Spawnable
  agents: coder — …; scout — …”.
- Single source: no second config file.

### 11. Data flow (spawn happy path)

```text
Model → spawn_agent{prompt, agent_spec_id?}
  → MultiAgentToolProvider::spawn
      → list_agent_specs() for default/validate
      → resolve id
      → AgentRuntime::create_agent(CreateAgentRequest{ agent_spec_id })
      → run_agent / send_agent_input_detached (existing)
  → ToolExecResult ok + agent_instance_id + agent_spec_id + report|status
```

### 12. Compatibility

| Concern | Approach |
|---------|----------|
| Existing sessions | No storage migration |
| Existing tool names | Keep `list_agents`, spawn names |
| Clients calling AgentSpecList | Unchanged |
| Tests fixing required agent_spec_id | Update schemas + add catalog tests |
| Models trained on old required field | Still valid to pass agent_spec_id explicitly |

## Package impact

| Package | Change |
|---|---|
| `piko-orchd-api` | `AgentRuntimeApi::list_agent_specs`; maybe no new error variants if recovery is provider-local |
| `piko-orchd` | `ExecutionServices::list_agent_specs`; `AgentRuntime` impl; `MultiAgentToolProvider` tools/spawn/list_agent_specs/errors/descriptions; unit/integration tests |
| `piko-protocol` | None required for slice 1 (tool JSON only) |
| `piko-hostd` | None for slice 1 (registry already registered); optional later: align AgentSpecList summary fields with tool |
| `piko-llmd` | None |
| `piko-sandbox` | None |
| `piko-tui` | None required (optional help text later) |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

| Case | Behavior |
|------|----------|
| Unknown `agent_spec_id` | Fail closed; `agent_spec_not_found` + valid ids in message |
| Omitted id, `general` missing | Fail closed; `agent_spec_required` + valid ids |
| Omitted id, `general` present | Spawn as `general`; echo in result |
| `AgentNotFound` / unauthorized / depth | Existing F-10 errors; keep codes/messages |
| Attached spawn cancelled | Existing `Cancelled` path |
| Empty catalog | list_agent_specs succeeds empty; spawn always fails with required/not found recovery |
| Registry mutation mid-session | In-process map is authoritative for the process; no hot-reload required in slice 1 |

## Verification

### Unit

- `resolve_spawn_spec_id`: default general, explicit id, unknown id, empty
  catalog.
- `list_agent_specs` JSON: sorted ids, default field presence/absence.
- Error message contains offered ids for unknown/missing.
- Tool discover() includes `list_agent_specs` and spawn without required
  `agent_spec_id`.

### Integration (orchd)

- Register `coder`+`general`; `list_agent_specs` returns both.
- `spawn_agent` with only `prompt` creates child with `agent_spec_id=general`.
- `spawn_agent` with bad id fails with recovery list including `coder`.
- `list_agents` still returns instance tree after spawn; instance carries
  resolved `agent_spec_id`.
- `message_agent` queue on idle starts a run; queue on busy returns queued.
- `message_agent` steer on busy steers; steer on idle fails with
  `agent_not_running`.
- Discover catalog contains `message_agent` and does not contain
  `followup_task` / `send_agent_message`.

### Acceptance mapping (F-21)

| F-21 criterion | Test |
|----------------|------|
| Spec discovery without list_agents | list_agent_specs unit/integration |
| list_agents not a spawn catalog | description fixture / doc assert optional; field names instance-centric |
| Spawn listed id | integration spawn coder |
| Unknown id recovery | unit/integration error text |
| Default general | integration omit id |
| single message_agent + when | unit/integration message_agent matrix |
| steer idle fails closed | unit/integration |
| F-10 tools still work | existing multi-agent cases remain green |

No codex-rs differential required for catalog (piko-native).

## Alternatives considered

| Alternative | Why not (slice 1) |
|-------------|-------------------|
| Only improve tool descriptions, no list tool | Fails F-21 acceptance; models still invent ids |
| Enum all ids in JSON Schema | Registry is dynamic (workspace agents); list tool is accurate |
| Force hostd Command through model | Models use tools, not host commands; orchd registry already hot |
| Keep dual followup + send tools with description only | Rejected: idle paths alias each other; busy fork is silent if model picks wrong tool |
| Two renamed tools (`followup_task` + `steer_agent`) | Rejected in favor of single schema + `when` (F-21 product choice A) |
| `when=steer` maps to `Auto` | Rejected: Auto starts a turn when idle; steer must fail closed when idle |
| Rename `list_agents` → `list_agent_instances` | Optional later with alias; not required for P0 |
| Put available ids only in system prompt | Stale risk; tools are authoritative per F-21 |

## Rollout

### Slice A — Catalog + resolve (P0)

1. `ExecutionServices::list_agent_specs` + `AgentRuntimeApi::list_agent_specs`.
2. Tool `list_agent_specs` + discover registration.
3. Spawn resolve default / validate + recovery errors + result echo
   `agent_spec_id`.
4. Unit + integration tests above.

### Slice B — `message_agent` schema + catalog swap (P0, same PR OK)

1. Add `message_agent` tool def + execute with `when` resolve and idle-steer
   preflight.
2. Remove `followup_task` and `send_agent_message` from `discover()`.
3. Map queue → `FollowUp`, busy steer → `SteerActive`.
4. Tests: queue idle/busy, steer busy, steer idle fails, invalid when.

### Slice C — Description pack (P0, same PR OK)

1. Rewrite remaining multi_agent tool (+ tool set) descriptions per §8–9.
2. Schema property descriptions for spawn and message_agent.
3. Assert catalog no longer lists legacy message tool names.

### Slice D — Optional prompt hint (P1 / open)

1. Hostd prompt fragment from registered specs.
2. Feature flag or always-on small bullet list.
3. Verify single registry source (no duplicate load logic if avoidable).

### Slice E — Optional cleanup (later)

1. Compatibility alias rename for list live agents.
2. Structured `details.available_spec_ids` on tool errors if DTO expands.

## Implementation notes (file map)

| Area | Likely touch |
|------|----------------|
| `orchd` `ExecutionServices` | `list_agent_specs` |
| `orchd-api` `AgentRuntimeApi` | new method |
| `orchd` `AgentRuntime` | impl list |
| `orchd` `multi_agent_provider.rs` | tools, spawn, errors; split module if large |
| `orchd` tests | multi_agent / agent_runtime cases |

## Open design resolutions (from F-21)

| F-21 open question | This design |
|--------------------|-------------|
| Catalog tool name | **`list_agent_specs`** |
| send vs followup | **`message_agent` + `when: queue\|steer`**, default queue; steer idle → `agent_not_running`; catalog drops old tools |
| Prompt injection | **Slice D**, not P0 gate |
| Steer delivery | **`SteerActive`** when busy (not `Auto`) |
