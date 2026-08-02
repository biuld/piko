# codex-rs Agent Core Digest

> Date: 2026-08-02
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
- piko status: **partial** — `F-02 model-gateway` (`piko-llmd`: providers,
  gateway, executor, retry/backoff budget with capped jittered backoff,
  status-error peeking, per-provider stream → non-streaming fallback,
  cost/token middleware; streaming requests capture usage). Model continuity
  landed (hostd `active_model` + durable per-session record driving the
  prompt model-switch fragment and JSONL `ModelChange` marker; F-02/D-16/
  V-16). Gaps: prewarm and sticky routing.

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
  Fragment-catalog breadth slice landed (world-state `state.run`,
  environment-context `environment.host`, model-switch
  `context.model-switch`; F-03/D-03/V-03). Gaps: inter-agent message
  fragments, mention-syntax parsing, cache-planning polish, world-state
  diffing (F-04).

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
  truncation; F-04/D-04/V-04) and `F-05 compaction` (hostd summarizer).
  Gaps: model-visible context tools, auto-compact budget windows, remote
  compaction.

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
- piko status: **partial** — `F-06 tool-system` (registry, providers,
  sequential executor; parallel batch dispatch is the first landed slice).
  Gaps: dynamic/extension tools, tool search, code-mode result shaping,
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
  and `F-12 safety` planned: no guardian loop, no elicitation/attestation, no
  patch-safety assessment.

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
- piko status: **partial** — `F-08 exec-sandboxing` (`piko-sandbox`: file
  policy + runner). Gaps: PTY/process-group lifecycle, shell snapshots,
  unified long-lived processes, network sandbox, environment selection.

### H. Session Persistence, Threads & Resume

- Evidence: `thread_manager.rs`, `thread_manager_tests.rs`,
  `state_db_bridge.rs`, `rollout.rs`, `thread_rollout_truncation.rs`,
  `session_startup_prewarm.rs`, external `codex_thread_store`
- Distilled behavior: durable thread store with metadata, fork/branch/resume,
  thread graph + sections, initial/resumed history modes, rollout files with
  cursor paging, interrupted-turn markers.
- piko status: **partial** — `F-09 session-persistence` (hostd schema-v3
  `session.json` + per-agent JSONL shards). Gaps: fork/branch, thread list
  cursor paging, interrupted-turn markers, session prewarm.

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
- piko status: **partial** — `F-10 multi-agent` (AgentInstance tree,
  AgentRuntime, attached/detached spawn, inbox, close/reopen, and v2
  followup/interrupt/list/wait tools landed in F-10/D-10/V-10). Gaps:
  agent roles; inter-agent notification fragments.

### J. Skills, Plugins, Hooks & MCP

- Evidence: `skills.rs`, `mcp_skill_dependencies.rs`, `plugins/*`,
  `mcp.rs`, `session/mcp*.rs` (runtime/refresh/prewarm), `mcp_tool_call.rs`,
  `mcp_tool_exposure.rs`, `mcp_openai_file.rs`,
  `mcp_tool_approval_templates.rs`, `hook_runtime.rs`
- Distilled behavior: skills loading/injection (explicit + implicit
  invocation), plugin discovery/mentions/render, MCP server lifecycle with
  tool catalog caching and prewarm, MCP tool exposure/approval templates,
  hooks for additional context and input inspection.
- piko status: **partial** — `F-13 mcp-integration` (hostd stdio MCP provider)
  and `F-14 skills-plugins` (skills loader + prompt injection). Gaps: MCP
  resource/search/approval templates, plugin system, hooks.

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
- piko status: **partial** — `F-15 observability` (llmd usage/cost middleware,
  orchd event lanes + deltas). Gaps: turn timing metrics, rollout recorder,
  diff tracker.

### M. Configuration & Permissions

- Evidence: `config/*` (config layers, permissions, permission-profile
  catalog, managed features, agent roles, schema), `config_lock.rs`,
  `environment_selection.rs`
- Distilled behavior: layered config (CLI > session > project > user > bundled),
  permission profiles with materialized file/network/command policies,
  managed-feature gating, agent-role layers, environment selection.
- piko status: **partial** — settings ownership design exists (hostd
  `settings.toml`). Gaps: permission-profile materialization, managed
  features, agent roles.

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

| Block | F-ID | piko status | First concrete slice |
|---|---|---|---|
| A Turn & Agent Runtime | F-01 | implemented (F-01/D-01/V-01) | — |
| B Model Gateway | F-02 | partial (retry/backoff + streaming fallback landed) | prewarm, sticky routing |
| C Prompt Assembly | F-03 | partial | fragment catalog breadth, world state |
| D Context & Compaction | F-04, F-05 | partial (F-04 slice landed) | model-view truncation + snapshots landed; budget windows next |
| E Tool System | F-06 | partial (in progress) | parallel batch dispatch |
| F Approvals & Safety | F-07, F-11, F-12 | partial | guardian loop, elicitation |
| G Exec & Sandbox | F-08 | partial | PTY/process lifecycle, unified exec |
| H Persistence & Resume | F-09 | partial | fork/branch, interrupted markers |
| I Multi-Agent | F-10 | partial (v2 tools landed) | agent roles, inter-agent fragments |
| J Skills/Plugins/MCP | F-13, F-14 | partial | MCP resources, hooks |
| K Realtime/Multimodal | F-16 | not started | deferred extension |
| L Observability | F-15 | partial | turn timing, rollout recorder |
| M Config & Permissions | F-12 part | partial | permission profiles, roles |

## 5. Reading codex-rs per feature

When writing a Feature PRD for block X, start from the evidence paths listed
above, then read the corresponding `*_tests.rs` for acceptance fixtures. Tests
encode behavior that prose misses; they are the differential-validation
fixtures for `docs/verification/`.
