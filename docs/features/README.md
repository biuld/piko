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
| F-08 | exec-sandboxing | Original sandbox/process lifecycle baseline; lifecycle retained by F-23 | superseded by F-23 | core/src/exec*.rs, sandboxing/, unified_exec/ |
| F-09 | session-persistence | Thread store, resume, branch-point fork, message durability, interrupted-turn finalization | partial (F-09/D-26/V-26 branch-point fork landed; list paging / prewarm deferred) | core/src/thread_manager.rs |
| F-10 | multi-agent | Subagent spawn, supervision, result collection; v2 collaboration tools | implemented (F-10/D-10/V-10; F-20 completion fragments) | core/src/agent/, tools/handlers/multi_agents* |
| F-11 | guardian | Automatic approval review loop | implemented (F-11/D-11/V-11) | core/src/guardian/ |
| F-12 | safety | Deterministic write-safety gate (patch-safety assessment); elicitation pause deferred; attestation rejected | implemented (F-12/D-12/V-12; patch-safety slice 1) | core/src/safety.rs, elicitation.rs, attestation.rs |
| F-13 | mcp-integration | MCP client/server, resource and tool exposure, approval templates, prewarm, TUI `/mcp` surface | implemented (F-13/D-23/D-24, V-23/V-24) | core/src/mcp*.rs |
| F-14 | skills-plugins | Skills discovery/catalog and `$skill` injection; plugin install/list and hooks | partial (skills baseline F-14/D-14/V-14 implemented); plugin system and hooks deferred | core/src/skills.rs, plugins/ |
| F-15 | observability | End-to-end tracing, usage accounting, durable rollout paging, exact turn diffs, and prompt/model-input debugging | implemented for committed scope (D-15/V-15, D-29–D-32/V-29–V-32) | core/src/otel_init.rs, rollout*.rs, turn_timing.rs, turn_diff_tracker.rs, prompt_debug.rs |
| F-16 | realtime (extension) | Realtime audio/text sessions, multimodal preparation | planned (deferred) | core/src/realtime_*, audio_preparation.rs, image_preparation.rs |
| F-17 | permission-profiles | Named permission profiles; selection retained while command semantics are superseded by F-23 | superseded in part by F-23 | core/src/config/* (permissions, permission-profile catalog) |
| F-18 | managed-features | Settings-declared tool-family feature flags with operator pins: disabled features are removed from the catalog, direct calls fail closed with `feature_disabled`, and `[features] managed` pins win over every layer | implemented (F-18/D-21/V-21) | features/src/*, core/src/config/managed_features.rs |
| F-19 | agent-roles | Named role → F-17 permission-profile selection for multi-agent policies | implemented (F-19/D-22/V-22) | core/src/config/* (agent roles) |
| F-20 | inter-agent-fragments | Detached child completion → parent retained Context on next run | implemented (F-20/D-25/V-25) | context/inter_agent_completion_message.rs, subagent_notification.rs |
| F-21 | multi-agent-tool-surface | Model-facing multi-agent tools: spec catalog discovery, spawn defaults/errors, clear spec vs instance vocabulary | implemented (F-21/D-33 slices A–C; prompt hint deferred) | multi_agents* (verbs only; piko-native catalog) |
| F-22 | client-agent-projection | Canonical host-authored agent timeline projection, stream convergence, usage, and client foreground state | implemented (F-22/D-34 slices 1–3b; Slice 4 product-gated) | ACP v1/v2 modeling reference; piko-owned wire |
| F-23 | command-execution-authority | Full-shell unified exec, enforced containment, host-owned authorization/elevation, typed process results | implemented (F-23/D-35/V-35/ADR-005) | core tools/orchestrator/exec-policy/unified-exec/sandboxing |
| F-24 | provider-authentication | Typed API-key/OAuth semantics, provider-owned refresh and request materialization, dynamic client capabilities | partial (typed OAuth/device-code slice D-36/V-36/ADR-007; explicit cancellation/browser login deferred) | OpenAI auth product behavior; codex-rs auth modeling reference |
| F-25 | native-openai-model-protocols | piko-owned semantic model gateway with native Responses and Chat Completions adapters; explicit protocol selection and removal of genai | implemented (F-25/D-37/V-37) | OpenAI Responses and Chat Completions API contracts |
| F-26 | protocol-neutral-inference | One general semantic inference contract; llmd-owned replay/resume planning; opaque durable checkpoints; structured model capabilities | implemented (F-26/D-38/V-38) | piko product direction; F-02/F-25 foundation |
