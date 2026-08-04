# Agent Runtime Roadmap

> Date: 2026-08-02
> Basis: [codex-rs Agent Core Digest](codex-agent-core-digest.md)
> Workflow: PRD-first (ADR-001) — each block splits into Feature PRDs
> (`docs/features/F-NN`), then designs (`docs/design/D-NN`), then vertical
> slices, then verification evidence (`docs/verification/V-NN`).

## 1. Roadmap shape

The codex-rs agent core splits into 13 functional blocks (A–M). piko already
carries substantial runtime infrastructure, so the roadmap is not a greenfield
port: it is organized as milestones that each make one *capability* whole,
reusing what already exists.

```text
M0 Core loop & foundations   (mostly landed; close remaining gaps)
 └─ A turn-runtime · B model-gateway · C prompt-assembly · E tool-system
    · F approvals · H persistence · I multi-agent (core)
M1 Context & memory          (make long sessions correct)
 └─ D context-management · D compaction · L observability basics
M2 Execution & sandbox       (real work execution)
 └─ G exec/sandbox
M3 Trust & safety            (human-in-the-loop at scale)
 └─ F guardian · F elicitation · F safety assessment · M permissions
M4 Ecosystem                 (skills, plugins, MCP, hooks)
 └─ J skills/plugins/MCP/hooks
M5 Multi-agent depth         (v2 collaboration surface)
 └─ I followup/interrupt/list/wait · agent roles · inter-agent fragments
M6 Observability & ops       (production readiness)
 └─ L rollout/turn-timing/diff-tracking
M7 Extension                 (optional, not core)
 └─ K realtime/multimodal
```

## 2. Milestone details

### M0 — Core loop & foundations

Goal: a turn is durable, resumable, deterministic, and can execute any tool.
Status: the largest part is landed (F-01, F-02 slice, F-06, F-07, F-09,
F-10 core, F-03 baseline). Remaining slices:

| Feature | Slice | Status |
|---|---|---|
| F-06 tool-system | parallel batch dispatch + sequential exclusivity | implemented (D-06, V-06) |
| F-01 turn-runtime | input admission; typed background tasks; turn-abort markers | implemented (D-01, V-01) |
| F-02 model-gateway | retry/backoff budget; streaming fallback; model continuity (resolved session record + prompt fragment + JSONL marker) | implemented (F-02/D-02/D-16, V-02/V-16) |
| F-03 prompt-assembly | fragment catalog breadth (world state, environments, model-switch) | implemented (F-03/D-03/V-03) |
| F-10 multi-agent | v2 collaboration tools (followup/interrupt/list/wait) | implemented (F-10/D-10/V-10) |
| F-07 tool-approvals | approval timeout/deny semantics polish | implemented (F-07/D-07/V-07) |

Exit criteria: every M0 feature has a reviewed PRD; tool batches are parallel;
turn aborts reconstruct durable history; approvals are exercised end-to-end.
All M0 exit criteria are now met.

### M1 — Context & memory

Goal: long sessions stay correct and within budget without manual intervention.

| Feature | Slice | Status |
|---|---|---|
| F-04 context-management | transcript normalization/truncation; snapshot sharing; token accounting; world-state diffing (full → diff, hostd durable baseline, cleared on compaction) | implemented (F-04/D-04/V-04; slice 2 D-17/V-17) |
| F-05 compaction | auto-compact trigger; inline compact; budget windows; remote compaction | implemented (F-05/D-05/V-05; piko-native summarizer-model override replaces provider-side remote compaction) |
| F-05 compaction | per-model growth defaults: `min_growth_tokens` derives from the resolved model window via `min-growth-fraction` when unset | implemented (slice 2, D-18/V-18) |
| F-15 observability | usage accounting per turn (baseline for budget decisions) | partial |

Dependency: M1 needs M0 F-01 (durable transcripts) to be solid before
rewriting history safely.

### M2 — Execution & sandbox

Goal: real commands run with PTY, timeouts, process groups, cancellation, and
fail-closed sandboxing.

| Feature | Slice | Status |
|---|---|---|
| F-08 exec-sandboxing | PTY/process-group lifecycle; shell snapshots; network sandbox; `bash` tool wired through the runner | implemented (slice 1, D-08/V-08) |
| F-08 exec-sandboxing | unified long-lived processes (process manager + `process` tool); environment capability selection (`environment` tool); `/ps` + `/kill` client surface (`process.list`/`process.stop`) | implemented (slice 2, D-19/V-19) |

Dependency: M2 builds on F-06 tool routes (shell tool) and M0 F-01
cancellation.

### M3 — Trust & safety

Goal: on-request approvals scale without weakening safety.

| Feature | Slice | Status |
|---|---|---|
| F-11 guardian | auto-review loop (host-owned bounded review, strict JSON, fail-closed timeout/malformed); per-session circuit breaker with user escalation/reset | implemented (F-11/D-11/V-11) |
| F-12 safety | patch-safety assessment; elicitation pause; attestation | implemented (slice 1: deterministic write-safety gate, F-12/D-12/V-12); elicitation pause deferred until a consumer; attestation rejected (no piko consumer) |
| F-17 permission-profiles | named profiles (file/network policy → sandbox policy; command allow/deny prefix rules → approval gateway) | implemented (F-17/D-20/V-20) |
| M-config | managed features (F-18/D-21/V-21); agent roles (per-role profile selection) | implemented (F-18/D-21/V-21; F-19/D-22/V-22) |

### M4 — Ecosystem

Goal: MCP, skills, plugins, and hooks are first-class capabilities.

| Feature | Slice | Status |
|---|---|---|
| F-13 mcp-integration | stdio lifecycle + tools; MCP resources/search (`mcp_resource` tool); approval templates (`[mcp.approval-templates]`); prewarm (bounded eager connect); TUI `/mcp` status command + panel | implemented (F-13/D-23/V-23; TUI surface D-24/V-24) |
| F-14 skills-plugins | implicit skill invocation (skills loader + prompt injection) | partial — loader + injection landed; plugin system deferred (no piko consumer) |
| F-14 / new | hooks (additional context, input inspection) | deferred — no piko consumer; not scheduled |

### M5 — Multi-agent depth

Goal: parent agents can supervise and steer children (codex-rs v2 surface).

| Feature | Slice | Status |
|---|---|---|
| F-10 multi-agent | `followup_task`, `interrupt_agent`, `list_agents`, `wait_agent` | implemented (F-10/D-10/V-10) |
| F-10 multi-agent | agent roles (role config layers) | implemented for permission profiles (F-19/D-22/V-22); further role prompt/model layers deferred |
| F-03 / F-20 | inter-agent notification / completion fragments | implemented (F-20/D-25/V-25) |

### M6 — Observability & ops

| Feature | Slice | Status |
|---|---|---|
| F-15 observability | end-to-end tracing (OTel spans turn → agent → model → tool; OTLP HTTP + JSON fallback) + TTFT/TTFM metrics; rollout recorder; turn-diff tracking; prompt debugging | implemented (F-15/D-15/V-15; tracing + metrics slice) |

### M7 — Extension (optional)

| Feature | Slice | Status |
|---|---|---|
| F-16 realtime (new) | realtime sessions, multimodal prep | not started — deferred |

## 3. Per-block feature decomposition

Each block splits into Feature PRDs as follows. The `F-NN` IDs are the stable
identifiers used by `docs/features/`; one PRD per row below (row = one
behavior contract with its own acceptance criteria).

| Block | F-ID | Decomposed features (PRDs) |
|---|---|---|
| A Turn & Agent Runtime | F-01 | turn lifecycle; input admission/queueing; background tasks; turn-abort reconstruction |
| B Model Gateway | F-02 | provider registry; streaming + retry; usage/token metadata; model continuity |
| C Prompt Assembly | F-03 | fragment system; AGENTS.md; skills/plugins injection; world-state/env fragments |
| D Context & Compaction | F-04 / F-05 | transcript manager; truncation; auto-compact; remote compact |
| E Tool System | F-06 | registry/routing; batch dispatch (parallel); approvals hook; dynamic tools; tool search |
| F Approvals & Safety | F-07 / F-11 / F-12 | approval tiers; network approval; guardian; elicitation; safety assessment |
| G Exec & Sandbox | F-08 | process lifecycle; shell; unified exec; platform sandboxes; network policy |
| H Persistence & Resume | F-09 | thread store; fork/resume; rollout files; interrupted markers |
| I Multi-Agent | F-10 | agent tree; v2 tools; roles; inter-agent messages |
| J Skills/Plugins/MCP | F-13 / F-14 | skills; plugins; MCP servers/resources; hooks |
| K Realtime | F-16 | realtime sessions; multimodal prep (deferred) |
| L Observability | F-15 | turn timing; rollout recorder; diff tracking |
| M Config & Permissions | F-12 part | config layers; permission profiles; managed features; agent roles |

## 4. Sequencing principles

1. **Finish M0 before M1+**: context rewriting and compaction assume durable,
   deterministic transcripts (F-01) and complete tool semantics (F-06).
2. **PRD before design, design before code** — one vertical slice per PRD,
   with `docs/verification/V-NN` evidence tied to acceptance criteria.
3. **Differential validation against codex-rs tests**: each PRD's acceptance
   criteria cite the codex-rs fixtures in the digest block.
4. **Keep hostd authoritative**: user-visible state (sessions, turns,
   approvals) stays in hostd; orchd remains transient execution.

## 5. Next step

**M0 is complete** and the M1 entry slices landed. **F-05 compaction**
`F-05/D-05/V-05` complete: budget-window auto-compact with hysteresis and a
pending guard, inline compact (`session.compact { mode: new-context-window }`
without summarization), model-visible `get_context_remaining` /
`new_context_window` tools, a `[transcript] max-tool-output-tokens` setting
wired into the F-04 model view, and a piko-native summarizer-model override
with default-model fallback (provider-side remote compaction rejected per
ADR-002). **F-04 world-state diffing** `D-17/V-17` complete: the `state.run`
facts moved to a retained transcript Context message — full on the first
run, diff across runs — with a hostd-owned durable baseline cleared on
compaction. **M1 remainder complete** (`F-05` slice 2, `D-18/V-18`): the
hysteresis guard derives from the resolved model's context window as a
fraction (`[compaction] min-growth-fraction`, default `0.125`) when
`min_growth_tokens` is unset, with the explicit setting and the constant
windowless fallback preserved. Token-budget prompt fragments stay rejected
(F-05 Fusion decisions: the model-visible tools cover the need). **M2 entry
slice landed** (`F-08` slice 1, `D-08/V-08`): the `bash` tool now runs
through a PTY-backed `piko-sandbox` runner — the shell is the
session/process-group leader, combined output is bounded, timeout and
cancellation escalate SIGTERM → SIGKILL to the whole group, the shell
snapshot resolves once (`shell_path` → `$SHELL` → default), and network
allow/deny is explicit on macOS seatbelt and Linux bwrap (including
`--share-net`). The blocking `runner::exec` path was refactored onto the
same wrapper builder so the two execution paths cannot drift. Remaining
sequencing: **M2 is complete** — `F-08` slice 2 landed (`D-19/V-19`): a
`ProcessManager` in `piko-sandbox` owns PTY processes across tool calls
(`process` tool: start with cwd/env overrides, `write_stdin`, incremental
output reads, group stop, list), the workspace provider owns the manager and
cleans up on drop, and environment capability discovery (usable shell
resolution, PATH normalization, common-tool probing) is exposed through a
read-only `environment` tool.

1. **Landed:** F-09 session-persistence branch-point fork
   (`SessionFork` with `entry_id`; F-09/D-26/V-26). Full clone, resume, and
   interrupted finalization remain under the F-09 baseline. Residual F-09:
   session list cursor paging, session prewarm (deferred).
2. Follow-on M0 gaps: F-03 mention-syntax and cache-planning polish both
   landed (D-27/V-27, D-28/V-28).
3. M1 residue worth tracking under M6: F-15 per-turn usage accounting
   (baseline for budget decisions) remains partial.
3. **M3 entry slice landed** (`F-11` slice 1, `D-11/V-11`): the guardian
   auto-review loop turns on-request tool approvals into fail-closed model
   decisions when `[guardian] enabled = true` — a host-owned bounded review
   call (guardian model override with default-model fallback) over a bounded
   slice of the durable session transcript must answer strict JSON; allow is
   one-shot (no store grant), deny fails closed with `guardian_denied`
   (reason surfaced), timeout/malformed/model error fails closed with
   `guardian_unavailable`, and a per-session circuit breaker
   (`max-consecutive-denials`, default 3) escalates to the user flow with any
   user decision resetting the loop. **F-12 slice 1 landed** (`F-12/D-12/V-12`):
   a deterministic, host-owned write-safety gate runs in the approval
   gateway before the guardian/user flows — `edit`/`write` requests whose
   targets are fully inside the sandbox writable roots auto-approve one-shot
   (no prompt, no store grant), out-of-roots targets fail closed with
   `safety_rejected`, and unassessable requests keep the existing flow
   (`[safety] auto-approve-workspace-writes`, default `true`). Elicitation
   pause is deferred to an F-12 slice 2 until a piko consumer exists
   (blocking process-output waits or MCP auth elicitation); attestation is
   rejected (no piko consumer; OpenAI host-integration specific). **F-17
   permission profiles landed** (`F-17/D-20/V-20`): `[permissions]`
   settings carry named profiles merged across global/project/override that
   materialize file/network policy (read/write roots, deny paths, network
   allow) into the sandbox policy and command policy (token-boundary prefix
   rules) into the approval gateway — denied prefixes fail closed with
   `permission_denied` before grants/prompts, allowed prefixes accept
   one-shot without a store grant, the built-in `default` profile never
   materializes (no behavior change), and `[sandbox] policy-path` files
   still win for the sandbox policy. **F-18 managed features landed**
   (`F-18/D-21/V-21`): a `[features]` settings section gates tool families
   by stable keys (`workspace`, `bash`, `process`, `environment`,
   `context`, `todo`, `multi-agent`, `user-interaction`, `mcp`) with all
   features enabled by default; disabled features are removed from the
   orchd catalog (discovery + routes) and direct calls fail closed with a
   non-retryable `feature_disabled` error; `[features] managed` pins are the
   final authority over `enabled` in every layer (conflicting explicit
   values log a warning and the pin wins, fail-closed); unknown keys warn
   and are ignored; and with `mcp` disabled hostd skips MCP server
   connections entirely. **F-19 agent roles landed** (`F-19/D-22/V-22`): a
   `[permissions.roles]` map attaches F-17 profiles to agent roles — the
   approval gateway evaluates commands with the executing role's command
   policy (denied prefixes fail closed with `permission_denied`, allowed
   prefixes one-shot accept) and workspace tools run under the role's
   materialized file/network sandbox policy, with unmapped roles inheriting
   the session profile and role layers unable to loosen below it. Next in
   M3: F-12 slice 2 (elicitation pause, still deferred until a consumer
   exists). **M4 entry slice landed** (`F-13/D-23/V-23`): MCP resources are
   discovered at connect (`resources/list` + `resources/templates/list`) and
   exposed through a built-in `mcp_resource` tool (list with a client-side
   `query` search filter, read by URI, text content only, distinct
   non-retryable errors for unknown server/missing URI/blob); `[mcp]
   approval-templates` maps `server/tool` or bare `tool` keys to
   operator-authored prompts that replace the generic approval question —
   orchd stamps the route `provider_id` on `ToolApprovalRequest`, hostd
   resolves the template (MCP servers only, `server/tool` wins) into
   `ApprovalSnapshot.prompt`, and TUI/GUI render it when present; prewarm is
   explicit and bounded (`[mcp] connect-timeout-ms` default 10000, per-server
   `timeout-ms` override) so one slow/broken server is skipped with a warning
   while siblings connect, and the F-18 `mcp` gate covers `mcp_resource` via
   its executor kind. **TUI MCP surface landed** (`D-24/V-24`): a `/mcp`
   slash command (neutral `mcp.status` catalog id) opens a status panel
   showing every configured server's connection state, tool/resource/
   template counts, and connect errors — including servers disabled by the
   `mcp` feature gate — fed by a hostd-owned snapshot from
   `initialize_mcp_tools`. M4 status: F-13 is complete and the F-14 skills
   slice (loader + prompt injection) is landed. The plugin system and hooks
   (additional context, input inspection) are **deferred** — no piko
   consumer today, so they are not scheduled. **M5 entry slice landed**
   (`F-20/D-25/V-25`): when a detached child report is durable in a parent
   inbox, the parent's **next** run injects a retained, data-only Context
   completion fragment (`source.kind = agent.completion`, stable message id
   per `report_id`) into the durable transcript chain after any F-04
   world-state Context and before the run input — model-visible outcome and
   bounded summary without consuming the inbox. Collect-first still skips
   injection; mid-run parents keep using `wait_agent` /
   `collect_agent_reports`. MESSAGE/NEW_TASK envelopes, status-only
   notifications, and auto-triggered parent turns are rejected for this
   slice. Next in M5: optional role prompt/model layers beyond F-19 if a
   consumer appears; residual F-09 session fork/branch remains separate.
