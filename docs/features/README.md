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
| F-02 | model-gateway | Provider registry, streaming events, usage, retry/backoff, responses + chat-completions wire formats | planned | core/src/client.rs, responses_retry.rs |
| F-03 | prompt-assembly | Frozen per-run prompt: system prompt, AGENTS.md, skills, context items | planned | prompts/, agents_md.rs |
| F-04 | context-management | Transcript handling, context items, token budget, truncation | planned | core/src/context_manager/ |
| F-05 | compaction | Summarize/compact conversation when over budget | planned | core/src/compact*.rs |
| F-06 | tool-system | Tool registry, schemas, routing, parallel batches, output handling | implemented (D-06, V-06) | core/src/tools/ |
| F-07 | tool-approvals | Human approval flows, network approval, permission requests | planned | core/src/tools/approvals.rs |
| F-08 | exec-sandboxing | Fail-closed filesystem/process/network policy, per-OS sandbox, shell snapshots | planned | core/src/exec*.rs, sandboxing/ |
| F-09 | session-persistence | Thread store, resume, branch, message durability | planned | core/src/thread_manager.rs |
| F-10 | multi-agent | Subagent spawn, supervision, result collection | planned | core/src/agent/, tools/handlers/multi_agents* |
| F-11 | guardian | Automatic approval review loop | planned | core/src/guardian/ |
| F-12 | safety | Safety rules, elicitation, attestation | planned | core/src/safety.rs, elicitation.rs |
| F-13 | mcp-integration | MCP client/server, resource and tool exposure | planned | core/src/mcp*.rs |
| F-14 | skills-plugins | Skills engine, plugin install/list | planned | core/src/skills.rs, plugins/ |
| F-15 | observability | Telemetry, rollout, turn timing, usage statistics | planned | core/src/rollout*.rs, turn_timing.rs |
| F-16 | realtime (extension) | Realtime audio/text sessions, multimodal preparation | planned (deferred) | core/src/realtime_*, audio_preparation.rs, image_preparation.rs |
