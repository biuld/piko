# Agent Runtime Roadmap

> Date: 2026-08-06
> Basis: [codex-rs Agent Core Digest](codex-agent-core-digest.md)
> Workflow: PRD-first (ADR-001) — each block splits into Feature PRDs
> (`docs/features/F-NN`), then designs (`docs/design/D-NN`), then vertical
> slices, then verification evidence (`docs/verification/V-NN`).

## 1. Status model

This roadmap tracks the scope piko intentionally commits to, not parity with
codex-rs. Status labels mean:

- **complete** — every committed slice for the milestone has landed and has
  verification evidence;
- **complete for committed scope** — planned behavior has landed, while named
  optional or consumer-triggered residue remains deferred;
- **partial** — at least one committed capability in the milestone goal has
  not landed;
- **deferred** — no implementation is scheduled.

The [Agent Core Digest](codex-agent-core-digest.md) separately records
reference coverage. A digest block may remain `partial` after its roadmap
milestone is complete when piko explicitly rejects or defers reference
behavior.

## 2. Roadmap shape

The codex-rs agent core splits into 13 functional blocks (A–M). piko already
carries substantial runtime infrastructure, so this is a capability roadmap,
not a greenfield port.

```text
M0 Core loop & foundations   complete for committed scope
 └─ A turn-runtime · B model-gateway · C prompt-assembly · E tool-system
    · F approvals · H persistence · I multi-agent (core)
M1 Context & memory          complete
 └─ D context-management · D compaction · L usage accounting
M2 Execution & sandbox       complete
 └─ G exec/sandbox
M3 Trust & safety            complete for committed scope
 └─ F guardian · F safety assessment · M permissions/config
M4 Ecosystem                 partial
 └─ J skills/plugins/MCP/hooks
M5 Multi-agent depth         complete for committed scope
 └─ I followup/interrupt/list/wait · agent roles · fragments
M6 Observability & ops       partial
 └─ L tracing/usage · rollout · diff-tracking · prompt debugging
M7 Extension                 deferred
 └─ K realtime/multimodal
```

## 3. Milestone details

### M0 — Core loop & foundations

Goal: a turn is durable, resumable, deterministic, and can execute the piko
tool catalog.

Status: **complete for committed scope**.

| Feature | Landed scope | Evidence |
|---|---|---|
| F-01 turn-runtime | input admission, typed background tasks, durable turn-abort markers | D-01/V-01 |
| F-02 model-gateway | retry/backoff budget, streaming fallback, usage, model continuity | D-02/D-16, V-02/V-16 |
| F-03 prompt-assembly | fragment catalog, mention syntax, cache planning | D-03/D-27/D-28, V-03/V-27/V-28 |
| F-06 tool-system | registry/routing, parallel batches, sequential exclusivity | D-06/V-06 |
| F-07 approvals | bounded approval timeout with distinct deny/expiry semantics | D-07/V-07 |
| F-09 persistence | schema-v3 store, resume/recovery, full clone, branch-point fork | D-26/V-26 |
| F-10 multi-agent core | agent tree plus v2 followup/interrupt/list/wait tools | D-10/V-10 |

Deferred residue: model prewarm/sticky routing; dynamic/extension tools and
tool search; session-list cursor paging and session prewarm. These are not on
the current critical path.

### M1 — Context & memory

Goal: long sessions stay correct and within budget without manual
intervention.

Status: **complete**.

| Feature | Landed scope | Evidence |
|---|---|---|
| F-04 context-management | normalization, snapshots, token accounting, tool-output truncation, durable world-state full→diff | D-04/D-17, V-04/V-17 |
| F-05 compaction | automatic and inline compaction, budget windows, context tools, summarizer-model override | D-05/V-05 |
| F-05 model defaults | resolved-model growth threshold via `min-growth-fraction` | D-18/V-18 |
| F-15 usage accounting | durable per-step facts plus hostd turn/session ledger | D-29/V-29 |

Provider-side remote compaction and token-budget prompt fragments are rejected
for this roadmap; piko-native summarization and model-visible context tools
cover the chosen behavior.

### M2 — Execution & sandbox

Goal: real commands run with PTY, timeouts, process groups, cancellation, and
fail-closed sandboxing.

Status: **complete**.

| Feature | Landed scope | Evidence |
|---|---|---|
| F-08 slice 1 | PTY/process-group lifecycle, shell snapshots, network sandbox, `bash` runner | D-08/V-08 |
| F-08 slice 2 | long-lived process manager/tool, environment discovery, client process controls | D-19/V-19 |

### M3 — Trust & safety

Goal: on-request approvals scale without weakening safety.

Status: **complete for committed scope**.

| Feature | Landed scope | Evidence |
|---|---|---|
| F-11 guardian | bounded host-owned review, strict JSON, fail-closed errors, circuit breaker | D-11/V-11 |
| F-12 safety | deterministic write-safety assessment before guardian/user approval | D-12/V-12 |
| F-17 permissions | named file/network/command profiles | D-20/V-20 |
| F-18 managed features | operator-pinned tool-family gates | D-21/V-21 |
| F-19 agent roles | role-to-permission-profile selection | D-22/V-22 |

Elicitation pause remains deferred until a concrete consumer exists.
Attestation is rejected because piko has no host-integration consumer for it.

### M4 — Ecosystem

Goal: MCP, skills, plugins, and hooks are first-class capabilities.

Status: **partial**.

| Feature | Scope | Status |
|---|---|---|
| F-13 MCP | stdio lifecycle/tools, resources, approval templates, bounded prewarm, TUI `/mcp` status | implemented (D-23/D-24, V-23/V-24) |
| F-14 skills | discovery, metadata catalog, on-demand loading, `$skill` injection | implemented baseline (F-14/D-14/V-14) |
| F-14 plugins | discovery/install/list and mentions | deferred — no current consumer |
| F-14 hooks | additional context and input inspection | deferred — no current consumer |

Future plugin, hook, or skill runtime-override behavior requires a new F-14
slice before implementation.

### M5 — Multi-agent depth

Goal: parent agents can supervise and steer children.

Status: **complete for committed scope**.

| Feature | Landed scope | Evidence |
|---|---|---|
| F-10 | `followup_task`, `interrupt_agent`, `list_agents`, `wait_agent` | D-10/V-10 |
| F-19 | permission-profile selection by agent role | D-22/V-22 |
| F-20 | detached-child completion fragment on the parent's next run | D-25/V-25 |

Role-specific prompt/model layers remain optional and consumer-triggered.

### M6 — Observability & ops

Goal: production-ready tracing, accounting, rollout inspection, and debugging.

Status: **partial**.

| Capability | Status | Evidence / next artifact |
|---|---|---|
| End-to-end spans, OTLP HTTP logs/metrics, stderr fallback, TTFT/TTFM | implemented | F-15/D-15/V-15 |
| Per-turn and cumulative usage accounting | implemented | D-29/V-29 |
| Rollout recorder/paging polish | not started | new F-15 slice required |
| Turn-diff tracking | not started | new F-15 slice required |
| Prompt debugging | not started | new F-15 slice required |

There is no JSON/file-log fallback: when OTel export is disabled, hostd logs
to stderr.

### M7 — Extension

Status: **deferred**. Realtime sessions and multimodal preparation are
optional extensions and have no scheduled implementation.

## 4. Per-block feature decomposition

| Block | F-ID | Decomposed behavior |
|---|---|---|
| A Turn & Agent Runtime | F-01 | lifecycle, admission/queueing, tasks, abort reconstruction |
| B Model Gateway | F-02 | providers, streaming/retry, usage, model continuity |
| C Prompt Assembly | F-03 | fragments, AGENTS.md, skills, world-state/environment context |
| D Context & Compaction | F-04/F-05 | transcript model, truncation, accounting, compaction |
| E Tool System | F-06 | registry/routing, batch dispatch, approvals hook |
| F Approvals & Safety | F-07/F-11/F-12 | approvals, guardian, elicitation decision, safety assessment |
| G Exec & Sandbox | F-08 | process lifecycle, shell, unified processes, platform sandbox |
| H Persistence & Resume | F-09 | store, fork/resume, recovery, interrupted markers |
| I Multi-Agent | F-10/F-19/F-20 | agent tree, v2 tools, role permissions, completion fragments |
| J Skills/Plugins/MCP | F-13/F-14 | skills, plugins, MCP, hooks |
| K Realtime | F-16 | realtime and multimodal preparation (deferred) |
| L Observability | F-15 | tracing, timing, usage, rollout/diff/debugging slices |
| M Config & Permissions | F-17/F-18/F-19 | permission profiles, managed features, agent roles |

## 5. Sequencing principles

1. **PRD before design, design before code.** Each vertical slice has
   verification evidence tied to its acceptance criteria.
2. **Reference behavior is selected, not inherited.** Deferred and rejected
   codex-rs behavior does not prevent a piko milestone from completing its
   committed scope.
3. **Keep hostd authoritative.** User-visible state such as sessions, turns,
   approvals, and usage ledgers remains in hostd; orchd owns transient agent
   execution.
4. **Activate deferred work only with a consumer.** Plugins/hooks,
   elicitation, session paging/prewarm, and role prompt/model layers remain
   unscheduled until a concrete piko journey needs them.

## 6. Current next steps

1. Decide whether production readiness is the next product goal. If yes,
   write the next F-15 PRD/design for one M6 residue before implementation.
2. Keep plugins/hooks, M7, and other consumer-triggered residue deferred.
