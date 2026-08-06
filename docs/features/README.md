# Feature PRDs

Feature PRDs define what users or agents can observe and what counts as
complete. They must remain independent of Rust crates, concurrency choices,
data structures, and file paths.

Create new documents from [`_TEMPLATE.md`](_TEMPLATE.md). Use stable
identifiers such as `F-01` so designs, tests, and commits can link back to
product intent.

Feature work is sequenced by the [Agent Runtime Roadmap](../agent-runtime-roadmap.md),
based on the [codex-rs Agent Core Digest](../codex-agent-core-digest.md).

## Feature index

| ID | Feature | Scope | Status | Source (codex-rs) |
|---|---|---|---|---|
| F-01 | turn-runtime | Turn → execution → model step → tool execution lifecycle, input admission, cancellation, turn-abort markers, typed background tasks, durable commit points | implemented (F-01/D-01/V-01) | core/src/session |
| F-02 | model-gateway | Provider registry, streaming events, usage, retry/backoff, responses + chat-completions wire formats; model continuity (resolved per-session record, prompt model-switch, JSONL marker) | implemented (F-02/D-02/D-16, V-02/V-16) | core/src/client.rs, responses_retry.rs, context/model_switch.rs |
| F-03 | prompt-assembly | Frozen per-run prompt: system prompt, AGENTS.md, skills, world-state/environment/model-switch fragments; `@path`/`$skill` mentions; cache-planning tiers | implemented (F-03/D-03/V-03; mention D-27/V-27; cache D-28/V-28) | prompts/, context/world_state.rs, mention_syntax.rs |
| F-04 | context-management | Transcript handling, per-message token accounting, copy-on-write snapshots, model-view truncation | implemented (F-04/D-04/V-04; accounting + snapshots + truncation slice) | core/src/context_manager/ |
| F-05 | compaction | Summarize/compact conversation when over budget | implemented (F-05/D-05/V-05) | core/src/compact*.rs |
| F-06 | tool-system | Tool registry, schemas, routing, parallel batches, output handling | implemented (D-06, V-06) | core/src/tools/ |
| F-07 | tool-approvals | Human approval flows with bounded deadlines, fail-closed expiry, distinct deny/expire semantics | implemented (F-07/D-07/V-07) | core/src/tools/approvals.rs |
| F-08 | exec-sandboxing | Fail-closed filesystem/process/network policy, per-OS sandbox, shell snapshots; long-lived processes + environment selection | implemented (F-08/D-08/D-19, V-08/V-19) | core/src/exec*.rs, sandboxing/, unified_exec/ |
| F-09 | session-persistence | Thread store, resume, branch-point fork, message durability, interrupted-turn finalization | partial (F-09/D-26/V-26 branch-point fork landed; list paging / prewarm deferred) | core/src/thread_manager.rs |
| F-10 | multi-agent | Subagent spawn, supervision, result collection; v2 collaboration tools | implemented (F-10/D-10/V-10; F-20 completion fragments) | core/src/agent/, tools/handlers/multi_agents* |
| F-11 | guardian | Automatic approval review loop | implemented (F-11/D-11/V-11) | core/src/guardian/ |
| F-12 | safety | Deterministic write-safety gate (patch-safety assessment); elicitation pause deferred; attestation rejected | implemented (F-12/D-12/V-12; patch-safety slice 1) | core/src/safety.rs, elicitation.rs, attestation.rs |
| F-13 | mcp-integration | MCP client/server, resource and tool exposure, approval templates, prewarm, TUI `/mcp` surface | implemented (F-13/D-23/D-24, V-23/V-24) | core/src/mcp*.rs |
| F-14 | skills-plugins | Skills discovery/catalog and `$skill` injection; plugin install/list and hooks | partial (skills baseline F-14/D-14/V-14 implemented); plugin system and hooks deferred | core/src/skills.rs, plugins/ |
| F-15 | observability | End-to-end tracing (turn → agent → model step → tool → child agent), OTLP HTTP export + metrics, stderr fallback, per-turn usage accounting (hostd ledger + OTel projection) | implemented for the two defined slices (F-15/D-15/V-15 + D-29/V-29); rollout/diff/prompt-debug slices not started | core/src/otel_init.rs, rollout*.rs, turn_timing.rs |
| F-16 | realtime (extension) | Realtime audio/text sessions, multimodal preparation | planned (deferred) | core/src/realtime_*, audio_preparation.rs, image_preparation.rs |
| F-17 | permission-profiles | Named permission profiles (file/network policy materialized into the sandbox policy; command allow/deny prefix rules materialized into the approval gateway) | implemented (F-17/D-20/V-20) | core/src/config/* (permissions, permission-profile catalog) |
| F-18 | managed-features | Settings-declared tool-family feature flags with operator pins: disabled features are removed from the catalog, direct calls fail closed with `feature_disabled`, and `[features] managed` pins win over every layer | implemented (F-18/D-21/V-21) | features/src/*, core/src/config/managed_features.rs |
| F-19 | agent-roles | Named role → F-17 permission-profile selection for multi-agent policies | implemented (F-19/D-22/V-22) | core/src/config/* (agent roles) |
| F-20 | inter-agent-fragments | Detached child completion → parent retained Context on next run | implemented (F-20/D-25/V-25) | context/inter_agent_completion_message.rs, subagent_notification.rs |
