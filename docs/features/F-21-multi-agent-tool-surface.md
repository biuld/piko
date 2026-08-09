# F-21: Multi-agent model tool surface

> Status: implemented (slice A/B/C — orchd tool surface)
> Priority: P0
> Design: [D-33](../design/D-33-multi-agent-tool-surface.md)
> Source evidence: codex-rs `multi_agents*` / `multi_agents_v2/*` (collaboration
> verbs only); digest Block I; piko F-10/D-10 (runtime + v2 tools), F-19
> (roles), F-20 (completion fragments); product incident: model calls
> `list_agents` then invents `agent_spec_id` because no catalog tool exists

## Summary

Redesign the **model-facing multi-agent tool API** so a supervising agent can
discover available agent templates (specs), spawn children with valid ids,
supervise live instances, and communicate without guessing configuration or
confusing templates with instances.

This feature does **not** redesign AgentInstance trees, mailboxes, durability,
permissions, or hostd session authority. It redefines what tools the model
sees, what each tool returns, and the discovery/default/error contracts those
tools must satisfy.

## Problem

F-10 landed a capable multi-agent **control plane** (spawn attached/detached,
message, followup, interrupt, list live agents, wait, collect reports,
close/reopen). Operators and the TUI can list named agent specs via host
catalog APIs. The **model** still cannot:

1. **Discover spawnable templates.** `spawn_agent` requires `agent_spec_id`,
   but no model tool lists registry specs (`coder`, `scout`, `general`, …).
2. **Tell template from instance.** `list_agents` returns live
   `agent_instance_id`s. Models treat it as a role catalog and then invent
   ids such as `agents/main`.
3. **Recover from bad ids.** Failed spawns do not return the set of valid
   spec ids, so the model cannot self-correct in one turn.
4. **Choose tools without ambiguity.** Dual messaging verbs
   (`send_agent_message` vs `followup_task`) share an idle path but diverge
   when busy; attached vs detached spawn also need clearer contracts.

The result is wasted tool rounds, sandbox workarounds, and failed spawns even
when the runtime and registry are healthy.

## User journeys

### J1 — Spawn with discovery (happy path)

1. Parent agent needs a specialized child (e.g. write code, research).
2. Parent calls **list agent specs** (or reads an equivalent catalog surface
   always visible to the model).
3. Parent receives stable ids plus human-readable name, role, and description
   for each spawnable template.
4. Parent calls **spawn** with a listed `agent_spec_id` and a task prompt.
5. Parent receives a child `agent_instance_id` and either a completion report
   (attached) or an acceptance status (detached).

### J2 — Supervise live tree

1. Parent has one or more children already running or idle.
2. Parent calls **list live agents**.
3. Parent sees the session tree ordered parents-before-children, with
   lifecycle, activity, and unread report signals.
4. Parent **messages** (queue or steer), **interrupts**, or **waits** using
   instance ids from that list—not spec ids.

### J3 — Invalid spec recovery

1. Parent calls spawn with an unknown `agent_spec_id`.
2. Tool fails closed with a clear error **and** the current valid spec id
   list (or a pointer to re-list specs).
3. Parent retries with a valid id without inventing paths or reading config
   files.

### J4 — Default template (optional path)

1. Parent wants a general-purpose child and omits `agent_spec_id` **if** the
   product enables a default template.
2. Spawn succeeds using the configured default (e.g. `general`).
3. The result still returns the resolved `agent_spec_id` so the model learns
   what was used.

### J5 — Detached completion awareness

1. Parent spawns detached and continues other work.
2. Child finishes; durable inbox + F-20 fragment rules still apply.
3. Parent may **wait** or **collect reports** using existing semantics; this
   PRD does not change F-20 fragment injection.

## In scope

- The full **model-visible** multi-agent tool set: names, parameters,
  required/optional fields, descriptions, success/error result shapes, and
  preferred usage order.
- A **first-class catalog discovery** tool or equivalent model-visible catalog
  for AgentSpec templates (id, name, role, description at minimum).
- Clear separation of **spec id** vs **instance id** in every tool description
  and result field name.
- Spawn defaults and fail-closed recovery when ids are missing or invalid.
- A single model-visible messaging tool with explicit `when` (`queue` vs
  `steer`) replacing dual send/followup tools in the catalog.
- Optional short **delegation hint** content (catalog summary) available to
  the model through the tool surface and/or prompt-assembly inputs already
  owned by hostd—without inventing a second registry.
- Differential acceptance relative to F-10 runtime behavior (same tree,
  mailbox, and authorization rules).

## Out of scope

- Redesigning AgentRuntime, durable agent commands, session schema, or
  parent-child authorization policy.
- New permission-profile or role systems (F-17/F-19 remain authoritative).
- Changing F-20 completion-fragment injection rules.
- TUI multi-agent dashboards (the client may later consume the same host
  catalog; not required for this PRD).
- 1:1 parity with codex-rs tool schemas or parameter names.
- Requiring models to read workspace agent TOML files as the primary discovery
  path.
- Realtime multi-agent UI events beyond existing host/orch surfaces.

## Behavior and states

### Identity vocabulary (normative)

| Term | Meaning | Used by |
|------|---------|---------|
| **Agent spec id** | Registry template identity (e.g. `coder`, `scout`) | spawn, list specs |
| **Agent instance id** | Live runtime identity in a session | list live agents, message_agent, interrupt, wait, close, reopen, collect |

Tools must not use ambiguous field names such as bare `agent_id` when both
kinds could apply. Results that refer to both must label both.

### Catalog: list agent specs

**Purpose:** Answer “what can I spawn?”

**Inputs:** none required.

**Success result (conceptual):**

- Ordered list of spawnable templates available to this session/workspace.
- Each entry includes at least: `id`, `name`, `role`, and `description` when
  present.
- Does not include live instance state.

**Empty:** returns an empty list (not an error) when no specs are registered;
spawn remains impossible until specs exist.

**Errors:** only environmental failures (registry unavailable). Not used for
“no live agents.”

### Live tree: list agents

**Purpose:** Answer “who is alive in this session?”

**Behavior:** preserved from F-10—every live agent, parents before children,
lifecycle, activity, unread report count, latest report summary when present.

**Description contract:** must state explicitly that this lists **instances**,
not spawn templates, and that spawn uses **spec ids** from the catalog tool.

### Spawn (attached)

**Purpose:** Create a child instance and wait for its first execution report.

**Inputs:**

- `agent_spec_id` — optional if a product default is configured; otherwise
  required.
- `prompt` — required task text.

**Success:** returns child `agent_instance_id`, resolved `agent_spec_id`, and
the attached run report (existing F-10 shape, field names clarified).

**Invalid spec:** fail closed; error message identifies the unknown id and
includes the current valid spec ids (or instructs a single catalog re-list).

**Missing spec when required:** fail closed with the same recovery aids as
invalid spec—not a silent guess of `main` unless defaulting is explicitly
enabled.

**Defaulting (product decision):** when enabled, omitted `agent_spec_id`
resolves to a configured default template (recommended built-in: `general`,
never implied as the session root unless configured). The resolved id is
always echoed in the result.

**Spawning root-like templates:** allowed only if the catalog exposes them;
descriptions should discourage re-spawning the root template for ordinary
delegation.

### Spawn detached

Same identity and validation rules as attached spawn. Result is acceptance
oriented (instance id + accepted status), not a full wait-for-report payload.
Description must contrast with attached spawn in one sentence each.

### Communication — single tool + `when` (schema)

The model sees **one** messaging tool (canonical name: `message_agent`).
Legacy names `followup_task` and `send_agent_message` are **not** advertised
in the model tool catalog after this feature lands (internal mapping or
removal is a design detail).

**Inputs (schema):**

| Field | Required | Values / rules |
|-------|----------|----------------|
| `agent_instance_id` | yes | Live instance id from `list_agents` |
| `message` | yes | Task or steer text |
| `when` | no | Enum: `"queue"` \| `"steer"`. **Default `"queue"`** when omitted |

**Semantics:**

| `when` | Idle target | Busy target |
|--------|-------------|-------------|
| `queue` (default) | Start a new turn immediately | Durably queue; runs after the current turn (F-10 FollowUp) |
| `steer` | **Fail closed** — agent is not running; use `when=queue` | Inject into the **active** turn (runtime steer / SteerActive) |

**Success result (conceptual):** includes at least `agent_instance_id`,
`when` (resolved), and a disposition such as `accepted` | `queued` |
`steered` as appropriate.

**Errors:**

- Unknown / unauthorized instance: fail closed as today.
- `when=steer` while idle: fail closed with a clear code/message (e.g.
  `agent_not_running`); do **not** silently start a new turn.
- Invalid `when`: fail closed.

**Non-goals for this tool:** spawn, list specs, interrupt, wait, collect.

### Interrupt, wait, collect, close, reopen

Behavioral semantics remain F-10. This PRD requires:

- Parameters and results use **instance** ids only.
- Descriptions reference list-live-agents for id discovery.
- No requirement to call list-specs for these tools.

### Model-visible guidance

The multi-agent tool family as a whole must make the following order obvious
from names + descriptions alone (no external docs required):

1. List specs → pick template  
2. Spawn → receive instance id  
3. List agents / wait / `message_agent` (default queue) / interrupt as needed  
4. Collect reports when durable inbox consumption is required  

Optional: a short catalog summary may be injected into the parent run’s
prompt assembly from the same host-authoritative registry (single source of
truth). Injection is additive; tools remain the authoritative interactive
discovery path.

### Authorization and failures

- Parent-child authorization and depth limits remain fail-closed as today.
- Tool-level failures return structured, model-readable errors (unknown
  instance, not authorized, invalid spec, cancelled, timed out).
- Invalid-spec errors must not require the model to open workspace files.

### Cancellation

- Attached spawn and wait remain cancellable with existing tool cancellation
  semantics (F-10).
- Catalog and list-live-agents are fast reads; cancellation is best-effort.

## Acceptance criteria

- [ ] A model can obtain the full set of spawnable **spec ids** without
      calling list-live-agents and without reading config files.
- [ ] List-live-agents descriptions and results cannot be reasonably
      interpreted as a spawn catalog (spec vs instance fields are distinct).
- [ ] Spawn with a listed spec id succeeds and returns both instance id and
      resolved spec id.
- [ ] Spawn with an unknown spec id fails closed and surfaces recovery
      information listing valid spec ids (or forces one catalog call that
      returns them).
- [ ] When defaulting is enabled, omitting spec id spawns the default
      template and echoes the resolved id; when disabled, omission fails
      closed with recovery information.
- [ ] Only one model-visible messaging tool exists; default `when` is `queue`
      (idle starts a turn, busy queues).
- [ ] `when=steer` on an idle agent fails closed and does not start a new turn.
- [ ] `when=steer` on a busy agent steers the active turn; success reports a
      steered disposition.
- [ ] Interrupt, wait, collect, close, reopen continue to satisfy F-10
      acceptance for instance targeting.
- [ ] No change to durable session layout or AgentInstance tree semantics is
      required for conformance.
- [ ] Differential note: codex collaboration verbs may be adapted; catalog
      discovery is piko-native and not blocked on codex parity.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Redesign runtime or only model tools? | **Model tool surface only** | Runtime (F-10) is sound; the failure mode is discovery and naming. |
| Spec catalog as a tool? | **Yes — first-class list-specs tool** (name may be `list_agent_specs`) | Interactive, cacheable, same pattern as other discovery tools; host registry already exists. |
| Inject catalog into system prompt? | **Optional additive**, same registry source | Helps first-shot spawn; tools remain authoritative for freshness. |
| Default `agent_spec_id` when omitted? | **Yes, default to `general` when that spec exists; else fail closed** | Matches “generic helper child” without promoting root `main`. Configurable later without PRD change to “must have a default.” |
| Allow spawn of `main` / root role? | **Allowed if catalog exposes it; descriptions discourage it for routine delegation** | Operators may want clones; models should prefer specialist templates. |
| Keep both send-message and followup? | **No — single tool `message_agent` with `when: queue \| steer` (default queue)** | One entry point; busy-path is explicit; idle+steer fails closed (no silent dual behavior). |
| Error includes full valid id list? | **Yes for invalid/missing spec** | Enables single-turn recovery without an extra call when the list is small. |
| Model must read `.piko/agents`? | **No** | Registry is host-authoritative; filesystem is not the tool API. |
| Rename list-live-agents? | **Optional**; if renamed, keep a compatibility alias or clear migration in design | Clarity > stability only if aliases preserve old sessions’ expectations; design records the wire name. |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| Collaboration verbs: spawn, message, followup, interrupt, list live agents, wait | **kept (adapted)** | F-10 runtime deliveries remain; model surface unifies message/followup into `message_agent` + `when`. |
| Role/agent config files for specialized children | **kept (adapted)** | piko AgentSpec registry (`main`/`coder`/`scout`/… + workspace overrides) is the source of truth. |
| Hide agent type / metadata from spawn schema (codex v2 regressions) | **rejected** | piko requires explicit, discoverable spec selection. |
| Thread-as-agent coupling and codex thread taxonomy | **rejected** | piko uses Session + AgentInstance; hostd remains authoritative. |
| Mode instructions for multi-agent | **kept (adapted)** | Optional catalog/delegation hints from the same registry; not a codex instruction port. |
| 1:1 tool names and parameter shapes | **rejected** | ADR-002: modeling reference only; piko names must encode spec vs instance. |
| Wait with timeout, non-consuming | **kept** | F-10 semantics unchanged. |

## Open questions

Resolved in [D-33](../design/D-33-multi-agent-tool-surface.md) for implementation:

1. Catalog tool name → **`list_agent_specs`**
2. send vs followup → **`message_agent` + `when: queue|steer`** (default `queue`; steer idle fails closed)
3. Catalog prompt injection → **slice C (P1)**; tools alone are P0

No further product blockers for slice A/B implementation.

## Reference evidence

- piko: [F-10 multi-agent](F-10-multi-agent.md), [D-10](../design/D-10-multi-agent-v2-tools.md),
  [F-19 agent roles](F-19-agent-roles.md), [F-20 inter-agent fragments](F-20-inter-agent-fragments.md),
  [ADR-002](../decisions/ADR-002-codex-modeling-reference.md),
  digest Block I ([codex-agent-core-digest.md](../codex-agent-core-digest.md))
- Incident: supervising model called `list_agents` then invented
  `agent_spec_id` while registry specs existed only on the host/TUI path
- codex-rs (evidence only): `tools/handlers/multi_agents*.rs`,
  `multi_agents_v2/*`, agent role/config layers; not a parity target
