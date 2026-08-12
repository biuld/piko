# codex-rs Agent Core Digest

> Date: 2026-08-06
> Evidence: `/Users/biu/Projects/codex/codex-rs/core/src`
> Status: living reference — update as new blocks are distilled into PRDs

This digest is the global view of the codex-rs agent core that piko is
reimplementing PRD-first. It splits the core into functional blocks, records
the codex-rs evidence for each, and states piko's current coverage. It is
**evidence, not specification** (ADR-001): architecture and coupling are not
translated; behavior is distilled into Feature PRDs under `docs/features/`.

## 1. The agent core at a glance

```text
user input / agent message
        │
        ▼
Session + TurnContext (config, skills, plugins, MCP, env, prompt fragments)
        │
        ▼
ContextManager (transcript snapshots, token accounting, truncation)
        │
        ▼
model step ──► ModelClient (Responses API stream / retry / prewarm)
        │
        ├── no tool calls ──► turn result
        └── tool calls ──► ToolRouter (approval → parallel gate → handler)
                          │
                          ├── apply_patch / shell / unified exec / MCP / …
                          └── multi-agent v2 (spawn / message / followup /
                               interrupt / list / wait)
        │
        ▼
commit items → durable rollout → loop or finish
```

Everything runs under a per-thread `Session` owned by `ThreadManager`.
Turns are driven by state machines (`state/`), long-lived background work is a
typed `Task` with cancellation (`tasks/`), and the transcript lives in a
copy-on-write `ContextManager` that compaction rewrites when the token budget
is exceeded.

## 2. Functional blocks

### A. Turn & Agent Runtime (the core loop)

- Evidence: `session/session.rs`, `session/turn.rs`, `session/step_context.rs`,
  `session/turn_context.rs`, `session/handlers.rs`, `session/input_queue.rs`,
  `state/{service,session,turn}.rs`, `tasks/{regular,lifecycle,compact,review,user_shell}.rs`,
  `codex_thread.rs`, `codex_delegate.rs`, `user_message_admission.rs`
- Distilled behavior: turn lifecycle (submit → steps → terminal), model-step
  loop with tool dispatch, input queueing/admission, turn abort handling,
  typed background tasks with cancellation, durable turn state transitions,
  attachment/detachment of sessions.
- piko status: **implemented** — `F-01 turn-runtime` (PRD in
  `docs/features/F-01-turn-runtime.md`, design `docs/design/D-01-turn-runtime.md`,
  evidence `docs/verification/V-01-turn-runtime-slices.md`): fixed-cap
  follow-up queue with overload, durable model-visible turn-abort markers
  (live cancel + crash recovery), and typed background task infrastructure
  with session-scoped cancellation.

### B. Model Gateway & Provider Abstraction

- Evidence: `client.rs`, `client_common.rs`, `responses_retry.rs`,
  `responses_metadata.rs`, `session_startup_prewarm.rs`, `connectors.rs`
- Distilled behavior: provider registry, streaming (SSE/WebSocket), request
  retry/fallback with budgets, sticky routing, prewarm, token/usage metadata,
  auth plumbing, model catalog and thinking-level mapping.
- piko status: **partial** — `F-02 model-gateway` complete for the planned
  slices (`piko-llmd`: providers, gateway, executor, retry/backoff budget
  with capped jittered backoff, status-error peeking, per-provider stream →
  non-streaming fallback, cost/token middleware; streaming requests capture
  usage; model continuity via hostd `active_model` + durable per-session
  record, prompt model-switch fragment, JSONL `ModelChange` marker;
  F-02/D-02/D-16, V-02/V-16). Residual gaps (not on the critical path):
  prewarm and sticky routing. Native OpenAI-family protocols and the
  protocol-neutral inference boundary landed in F-25/F-26. ADR-012 keeps the
  executable model catalog locally authoritative and rejects generic remote
  discovery until a narrower availability consumer exists.

### C. Prompt Assembly & Context Injection

- Evidence: `context/*` (~30 prompt-fragment modules: user instructions,
  permissions, world state, environments, skills/plugins instructions, model
  switch, realtime start/end, inter-agent messages, token budget, rollout
  budget, guardian reminders), `session_prefix.rs`, `agents_md.rs`,
  `agents_md_manager.rs`, `session/inject.rs`, `session/turn_context.rs`,
  `mention_syntax.rs`, `prompt_debug.rs`
- Distilled behavior: a frozen per-run system prompt assembled from typed
  prompt fragments with sources, trust levels, cache scopes; AGENTS.md
  loading/merging; skills and plugin injections; world-state and
  environment-context fragments; per-run digest/versioning.
- piko status: **partial** — `F-03 prompt-assembly` (hostd `domain/prompts`
  structured blocks + orchd frozen `AgentRunPrompt`; skills loader exists).
  Fragment-catalog breadth slice landed (world-state facts — originally a
  `state.run` block, since moved to a retained transcript message —
  environment-context `environment.host`, model-switch
  `context.model-switch`; F-03/D-03/V-03). World-state diffing landed in
  F-04 slice 2 (D-17/V-17): the run facts became a retained transcript
  Context message (full first, diff afterwards) with a hostd-owned durable
  baseline cleared on compaction. Inter-agent completion fragments landed in
  F-20/D-25/V-25 (unread detached reports → retained Context on the parent’s
  next run). Gaps: none on the F-03 critical path (mention-syntax D-27;
  cache-planning polish D-28 landed).

### D. Context Management & Compaction

- Evidence: `context_manager/{history,normalize,updates}.rs`,
  `compact.rs`, `compact_token_budget.rs`, `compact_model_fallback.rs`,
  `compact_remote*.rs`, `session/token_budget.rs`, `session/rollout_budget.rs`,
  `state/auto_compact_window.rs`, `thread_rollout_truncation.rs`,
  `tools/handlers/{get_context_remaining,new_context_window}.rs`
- Distilled behavior: copy-on-write transcript with snapshots and token
  accounting; function-output truncation; pre-sampling auto-compact, inline
  previous-model compact, remote/cloud compaction v2 with budget windows;
  token-budget context injection; model-visible "context remaining" tools.
- piko status: **partial** — `F-04 context-management` (orchd per-message
  token accounting, copy-on-write snapshots, model-view tool-output
  truncation, world-state full→diff retained Context; F-04/D-04/D-17,
  V-04/V-17) and `F-05 compaction` (F-05/D-05/D-18, V-05/V-18):
  budget-window auto-compact with hysteresis and pending guard, inline
  new-context-window compact, model-visible `get_context_remaining` /
  `new_context_window` tools, per-model `min-growth-fraction` defaults, and
  a piko-native summarizer-model override with default-model fallback
  (provider-side remote compaction rejected). Remaining gap: token-budget
  context fragments are **rejected** for this roadmap (F-05 fusion;
  model-visible tools cover the need).

### E. Tool System

- Evidence: `tools/{registry,router,parallel,context,events,lifecycle,orchestrator,spec_plan,hosted_spec,tool_dispatch_trace}.rs`,
  `tools/executed_tool_calls.rs`, `tools/handlers/*` (apply_patch, shell,
  unified_exec, mcp, request_user_input, request_permissions, view_image,
  tool_search, sleep, test_sync, current_time, dynamic, extension_tools,
  plan, wait_for_environment, get_context_remaining, new_context_window,
  list/request plugins, mcp_resource)
- Distilled behavior: tool registry/specs, routing, parallel gate with
  sequential exclusivity, approval hooks, tool-result truncation, code-mode
  results, executed-tool-call metadata, dynamic/extension tools, tool search.
- piko status: **partial** — `F-06 tool-system` core landed (registry,
  providers, sequential + parallel batch dispatch with sequential
  exclusivity; D-06/V-06). Residual gaps (not scheduled):
  dynamic/extension tools, tool search, code-mode result shaping,
  executed-call metadata.

### F. Approvals, Safety & Guardian

- Evidence: `tools/approvals.rs`, `tools/network_approval.rs`,
  `tools/sandboxing.rs` (ApprovalStore), `elicitation.rs`, `attestation.rs`,
  `safety.rs`, `exec_policy.rs`, `network_policy_decision.rs`,
  `guardian/*` (compact transcript → guardian review session → strict JSON
  allow/deny; circuit breaker), `tools/handlers/request_permissions.rs`
- Distilled behavior: tiered approval (never / on-request / always) with
  timeout, network-approval decisioning, command allow/deny prefix rules,
  patch-safety assessment, OpenAI-Form elicitation pause, attestation, and the
  guardian auto-review loop that converts on-request approvals into
  auto-approvals with a fail-closed review transcript.
- piko status: **partial** — `F-07 tool-approvals` slice 1 landed (approval
  gateway + hostd pending approvals + bounded deadlines with fail-closed
  expiry and distinct deny/expire semantics, F-07/D-07/V-07). `F-11 guardian`
  slice 1 landed (F-11/D-11/V-11): host-owned bounded review over a bounded
  session-transcript slice with strict-JSON allow/deny, one-shot allows
  (no store grants), fail-closed timeout/malformed, and a per-session circuit
  breaker escalating to the user. `F-12 safety` slice 1 landed
  (`F-12/D-12/V-12`): deterministic patch-safety assessment in the approval
  gateway (constrained writes auto-approve one-shot; out-of-roots writes
  fail closed with `safety_rejected`). Elicitation pause deferred until a
  piko consumer exists; attestation rejected (no piko consumer).

### G. Command Execution & Sandboxing

- Evidence: `exec.rs`, `spawn.rs`, `shell.rs`, `shell_snapshot.rs`,
  `user_shell_command.rs`, `command_canonicalization.rs`, `exec_env.rs`,
  `environment_selection.rs`, `sandboxing/`, `sandbox_tags.rs`,
  `windows_sandbox*.rs`, `unified_exec/*` (process manager, background
  processes, write_stdin), `network_proxy`
- Distilled behavior: process spawning with PTY, timeouts, process groups,
  cancellation grace; shell selection/snapshot for reuse; fail-closed
  filesystem/network/command policy; platform sandboxes; unified exec with
  long-lived processes; environment capability discovery.
- piko status: **redesign implemented** — `F-08
  exec-sandboxing` completed PTY/process-group lifecycle, shell snapshots,
  platform sandbox, long-lived processes, and environment capability
  selection (`D-08`/`D-19`, `V-08`/`V-19`). `F-23`/`D-35`/`ADR-005`
  replaces the static restricted-shell/command-whitelist path with
  full-shell unified exec, enforced containment, host-owned authorization,
  constrained additional permissions/elevation, and typed process results.

### H. Session Persistence, Threads & Resume

- Evidence: `thread_manager.rs`, `thread_manager_tests.rs`,
  `state_db_bridge.rs`, `rollout.rs`, `thread_rollout_truncation.rs`,
  `session_startup_prewarm.rs`, external `codex_thread_store`
- Distilled behavior: durable thread store with metadata, fork/branch/resume,
  thread graph + sections, initial/resumed history modes, rollout files with
  cursor paging, interrupted-turn markers.
- piko status: **implemented for durable storage/resume** —
  `F-31/D-43/ADR-015/V-42` provides the host-owned schema-v4 canonical event
  journal, append-only commits, deterministic replay/snapshots, private agent
  transcripts, branching, accounting, fork, and interrupted-execution
  recovery. Residual F-09 scope: thread-list cursor paging and session prewarm.

### I. Multi-Agent & Inter-Agent Communication

- Evidence: `agent/{registry,control,role,status,agent_resolver}.rs`,
  `tools/handlers/multi_agents*.rs` and `multi_agents_v2/*` (spawn_agent,
  send_message, followup_task, interrupt_agent, list_agents, wait_agent),
  `agent_communication.rs`, `session/multi_agents.rs`, `session/review.rs`,
  `context/{subagent_notification,inter_agent_message,realtime_delegation,multi_agent_mode_instructions}.rs`
- Distilled behavior: thread-as-agent tree with spawn depth limits, role
  config layers, agent status lifecycle, queue-only messaging with delivery
  modes, follow-up tasks, interrupts, wait with timeout, completion
  notification fragments, inter-agent telemetry.
-   piko status: **partial** — `F-10 multi-agent` (AgentInstance tree,
  AgentRuntime, attached/detached spawn, inbox, close/reopen, and v2
  followup/interrupt/list/wait tools landed in F-10/D-10/V-10). Inter-agent
  completion fragments landed in F-20/D-25/V-25 (unread detached reports
  inject retained Context on the parent’s next run). Gaps: optional role
  prompt/model layers beyond F-19 permission profiles.

### J. Skills, Plugins, Hooks & MCP

- Evidence: `skills.rs`, `mcp_skill_dependencies.rs`, `plugins/*`,
  `mcp.rs`, `session/mcp*.rs` (runtime/refresh/prewarm), `mcp_tool_call.rs`,
  `mcp_tool_exposure.rs`, `mcp_openai_file.rs`,
  `mcp_tool_approval_templates.rs`, `hook_runtime.rs`
- Distilled behavior: skills loading/injection (explicit + implicit
  invocation), plugin discovery/mentions/render, MCP server lifecycle with
  tool catalog caching and prewarm, MCP tool exposure/approval templates,
  hooks for additional context and input inspection.
- piko status: **partial** — `F-13 mcp-integration` landed
  (`F-13/D-23/V-23`): stdio provider, tool discovery/execution, resources
  (`mcp_resource` list/search/read), approval templates, and bounded
  prewarm, plus a TUI `/mcp` status command + panel (`D-24/V-24`).
  `F-14 skills-plugins`: the skills discovery/catalog and `$skill` baseline
  landed (`F-14/D-14/V-14`); plugin system and hooks are deferred because no
  piko consumer is scheduled.

### K. Realtime & Multimodal (extension)

- Evidence: `realtime_conversation.rs`, `realtime_context.rs`,
  `realtime_prompt.rs`, `audio_preparation.rs`, `image_preparation.rs`,
  `original_image_detail.rs`
- Distilled behavior: WebSocket realtime sessions (audio/text), realtime
  delegation fragments, image/audio preparation with token estimation.
- piko status: **not started** — `F-16 realtime (extension)` is planned and
  deferred; not part of the core runtime roadmap.

### L. Observability & Telemetry

- Evidence: `rollout.rs`, `rollout_budget.rs`, `turn_timing.rs`,
  `turn_metadata.rs`, `responses_metadata.rs`, `event_mapping.rs`,
  `turn_diff_tracker.rs`, `stream_events_utils.rs`, `prompt_debug.rs`,
  `memory_usage.rs`, `otel_init.rs`, `installation_id.rs`
- Distilled behavior: turn TTFT/TTFM metrics, usage accounting, event
  mapping/dedup, turn-diff tracking for UI deltas, rollout budget tracking,
  prompt debugging, rollout recording with cursor paging.
- piko status: **implemented for committed scope** — tracing/metrics
  (D-15/V-15), hostd-owned usage accounting (D-29/V-29), real assembly plus
  llmd model-input debugging (D-30/V-30), durable v3 rollout paging
  (D-31/V-31), and exact built-in-mutation turn diffs (D-32/V-32). Piko does
  not build a parallel ephemeral debug session or claim adapter-private HTTP
  wire payloads.

### M. Configuration & Permissions

- Evidence: `config/*` (config layers, permissions, permission-profile
  catalog, managed features, agent roles, schema), `config_lock.rs`,
  `environment_selection.rs`
- Distilled behavior: layered config (CLI > session > project > user > bundled),
  permission profiles with materialized file/network/command policies,
  managed-feature gating, agent-role layers, environment selection.
- piko status: **implemented** for the planned slices — F-17 permission
  profiles (D-20/V-20), F-18 managed features (D-21/V-21), F-19 agent roles
  mapped to profiles (D-22/V-22). Layered settings ownership remains in
  hostd (`settings.toml` global/project).

## 3. Cross-cutting invariants

- **Durability before visibility**: committed state precedes live projection;
  approvals, tool results, and agent reports are durable before delivered.
- **Deterministic transcripts**: commit order follows call/item order, not
  completion order; ContextManager snapshots make history copy-on-write.
- **Fail-closed defaults**: sandbox denies by default, approvals deny by
  default, guardian review fails closed on timeout/malformed output.
- **Cancellation everywhere**: every long-lived task is an abortable,
  cancellable future; turn aborts reconstruct durable history markers.
- **Evidence over architecture**: codex-rs module boundaries are not piko
  boundaries; only behaviors intentionally kept by a Feature PRD enter piko.

## 4. piko coverage matrix

This matrix measures coverage of the distilled codex-rs reference block. It
does not override roadmap completion for piko's intentionally committed
scope; rejected or consumer-triggered behavior remains visible as residue.

| Block | F-ID | piko status | First concrete slice / residue |
|---|---|---|---|
| A Turn & Agent Runtime | F-01 | implemented (F-01/D-01/V-01) | — |
| B Model Gateway | F-02 | partial (F-02/D-02/D-16 landed) | prewarm, sticky routing |
| C Prompt Assembly | F-03 | implemented (fragments + mentions D-27 + cache D-28) | — |
| D Context & Compaction | F-04, F-05 | partial (F-04/D-04/D-17 + F-05/D-05/D-18) | token-budget fragments rejected |
| E Tool System | F-06 | partial (core + parallel batch D-06) | dynamic tools / tool search (unscheduled) |
| F Approvals & Safety | F-07, F-11, F-12 | partial (slices landed) | F-12 elicitation deferred until consumer |
| G Exec & Sandbox | F-08/F-23 | implemented (D-35/ADR-005, V-35) | platform differential maintenance |
| H Persistence & Resume | F-31 | implemented (D-43/V-42; schema-v4 journal) | list paging, prewarm |
| I Multi-Agent | F-10, F-20 | partial (v2 tools + F-20 completions) | optional role prompt/model layers |
| J Skills/Plugins/MCP | F-13, F-14 | partial | F-13 complete; F-14/D-14/V-14 skills baseline landed; plugins/hooks deferred |
| K Realtime/Multimodal | F-16 | not started | deferred extension |
| L Observability | F-15 | implemented for committed scope (D-15, D-29–D-32) | — |
| M Config & Permissions | F-17, F-18, F-19 | implemented | — |

## 5. Reading codex-rs per feature

When writing a Feature PRD for block X, start from the evidence paths listed
above, then read the corresponding `*_tests.rs` for acceptance fixtures. Tests
encode behavior that prose misses; they are the differential-validation
fixtures for `docs/verification/`.
