# hostd (Rust) vs host-runtime (TypeScript) — Gap Analysis

## Overview

| | hostd (Rust) | host-runtime (TS) |
|---|---|---|
| Lines of code | ~1,500 | ~10,000+ |
| Architecture | Pure library, linked by hostd binary | Standalone process with PikoHost class |
| Orchestrator | Links orchd directly via `OrchCore` | JSON-RPC via `OrchdRpcClient` |
| Protocol | `HostEvent` (21 variants, snake_case) in `api/mod.rs` | `HostEvent` (21 variants) in `orchd/protocol/host-event.ts` |
| TUI connection | JSON-lines over stdio | Direct in-process (EventStream) + JSON-lines client |

---

## 1. Session Management

| Feature | hostd | host-runtime | Notes |
|---------|-------|-------------|-------|
| Create session | ✅ `state.create_session()` | ✅ `newSession()`, `SessionManager.create()` | |
| Open/restore session | ⚠️ Partial (`SessionOpen` command stubs) | ✅ `openSession()`, `restoreFromSession()` | hostd doesn't actually load from JSONL yet |
| List sessions | ❌ | ✅ `listSessions()` | |
| Delete session | ✅ `state.delete_session()` | ✅ `deleteSession()` | |
| Rename session | ❌ | ✅ `renameSession()` | |
| Fork session | ❌ (repo has `fork()` but not wired) | ✅ `forkSession()` | hostd's `JsonlSessionRepository` has `fork()` but server doesn't expose it |
| Navigate entries | ❌ | ✅ `navigateToEntry()` | |
| Import session | ❌ (repo has `import()` but not wired) | ✅ `importSession()` | |
| Clone session | ❌ | ✅ `cloneSession()` | |
| Switch session | ❌ | ✅ `switchSession()` | |
| Get session tree | ❌ | ✅ `getTreeEntries()`, `getBranchEntries()` | |
| Get branch summary | ❌ | ✅ `branchToEntry()`, `branchToEntryWithSummary()` | |

**Gap**: Session operations (list, rename, fork, navigate, import, clone, switch, tree) are mostly missing from hostd's server command handler. The `JsonlSessionRepository` has the storage primitives but the `HostServer::apply_command()` doesn't route these commands.

---

## 2. Session Persistence (JSONL)

| Feature | hostd | host-runtime | Notes |
|---------|-------|-------------|-------|
| JSONL read/write | ✅ `JsonlSessionRepository` | ✅ `SessionManager` + `piko-session` | |
| Session header format | ✅ (pi-compatible v3) | ✅ (same) | |
| Message persistence | ✅ `append_message()` | ✅ `saveMessages()` | |
| Fork persistence | ✅ `fork()` | ✅ `SessionManager.fork()` | |
| Import persistence | ✅ `import()` | ✅ | |
| Navigate (leaf entries) | ✅ `navigate()` | ✅ | |
| Agent sub-sessions | ❌ | ✅ `PikoSessionRuntime` with sidecar | |
| Compaction entries | ❌ | ✅ `CompactionEntry` in JSONL | |
| Branch summary entries | ❌ | ✅ `BranchSummaryEntry` in JSONL | |
| Model change entries | ❌ | ✅ `ModelChangeEntry` in JSONL | |
| Thinking level entries | ❌ | ✅ `ThinkingLevelChangeEntry` in JSONL | |
| Active tools entries | ❌ | ✅ `ActiveToolsChangeEntry` in JSONL | |
| Session info entries | ✅ `append_session_info()` | ✅ `SessionInfoEntry` | |

**Gap**: The JSONL storage layer exists in both, but hostd doesn't write model/thinking/compaction/branch metadata entries.

---

## 3. Turn Execution

| Feature | hostd | host-runtime | Notes |
|---------|-------|-------------|-------|
| Submit prompt | ✅ `TurnSubmit` command | ✅ `streamPrompt()` / `run()` | |
| Cancel turn | ✅ `TurnCancel` command | ✅ `cancelRun()` | |
| Streaming events | ✅ via EventBus + channel | ✅ via EventStream | |
| System prompt builder | ✅ `build_system_prompt()` | ✅ `buildSystemPrompt()` | |
| Prompt template expansion | ✅ `expand_prompt_template()` | ✅ `expandPromptTemplate()` | |
| Context file loading | ✅ `load_context_files()` | ✅ `loadContextFiles()` | |
| Multi-agent support | ❌ | ✅ `Orchestrator` with `dispatchDetached()` | hostd uses single agent per turn |
| Task steering (inter-agent) | ❌ | ✅ `steer_task` tool via orchestrator | |
| Abort handling | ❌ | ✅ `AbortController` | |

**Gap**: Multi-agent sub-task spawning and inter-task steering are missing from hostd.

---

## 4. Queue Management (Steer / Follow-up / Next-turn)

| Feature | hostd | host-runtime | Notes |
|---------|-------|-------------|-------|
| Steering queue | ❌ | ✅ `HostQueueController.steer()` | |
| Follow-up queue | ❌ | ✅ `HostQueueController.followUp()` | |
| Next-turn queue | ❌ | ✅ `HostQueueController.nextTurn()` | |
| Queue state query | ❌ | ✅ `getQueueState()` | |
| Dequeue | ❌ | ✅ `dequeue()` | |
| PromptBehavior routing | ❌ | ✅ `prompt()` with auto/steer/followUp routing | |
| QueueUpdate events | ❌ | ✅ `QueueUpdate` event type | |

**Gap**: Entirely missing. hostd treats every prompt as a fresh turn. No support for injecting messages into running turns or queuing follow-up commands.

---

## 5. Skills, Prompts, Templates

| Feature | hostd | host-runtime | Notes |
|---------|-------|-------------|-------|
| Skill loading | ✅ `load_skills()` | ✅ `loadSkills()` | |
| Skill formatting | ⚠️ Partial | ✅ `formatSkillsForPrompt()` | hostd skills lack model_override/thinking_level/tool override support in prompt |
| Skill frontmatter parsing | ⚠️ Basic | ✅ Full YAML frontmatter | |
| Prompt template loading | ✅ `load_prompt_templates()` | ✅ `loadPromptTemplates()` | |
| Template expansion | ✅ `expand_prompt_template()` | ✅ `expandPromptTemplate()` | |
| Context files loading | ✅ `load_context_files()` | ✅ `loadContextFiles()` | |
| Skill execution (runSkill) | ❌ | ✅ `runSkill() / streamSkill()` | Executes a skill as a full agent run |
| Prompt template execution | ❌ | ✅ `runPromptTemplate()` | |

**Gap**: Skill execution (treating a skill as a full prompt) and advanced frontmatter parsing (YAML with model/thinking overrides) are missing.

---

## 6. Model & Auth

| Feature | hostd | host-runtime | Notes |
|---------|-------|-------------|-------|
| Model registry | ✅ `ModelRegistry` | ✅ `ModelRegistry` | |
| Model resolution | ✅ `resolve()` | ✅ `resolve()` | |
| Provider listing | ✅ | ✅ | |
| Auth storage | ✅ `AuthStorage` with file/memory | ✅ `AuthStorage` with file/memory | |
| OAuth flow | ❌ | ✅ Device code flow, PKCE, browser auth | hostd has `OAuth` credential type but no OAuth flow implementation |
| API key auth | ✅ | ✅ | |
| Runtime model switching | ❌ (requires server restart) | ✅ `setConfig()` at runtime | |
| Thinking level switch | ⚠️ `ConfigSet` command updates settings | ✅ `setThinkingLevel()` | hostd rebuilds turn runner on config change |
| Active tool switching | ❌ | ✅ `setActiveToolNames()` | |
| Model change events | ❌ | ✅ `ModelConfigChanged` event | |

**Gap**: OAuth flows, runtime model/tool switching without restart, and model change event emission.

---

## 7. Compaction

| Feature | hostd | host-runtime | Notes |
|---------|-------|-------------|-------|
| Estimate tokens | ✅ `estimate_tokens()` | ✅ `estimateTokens()` | |
| Estimate context tokens | ✅ `estimate_context_tokens()` | ✅ `estimateContextTokens()` | |
| Compaction settings | ✅ `CompactionSettings` | ✅ `CompactionSettings` | |
| Automatic compaction trigger | ❌ | ✅ `shouldCompact()`, `maybeCompact()` | |
| Cut-point finding | ❌ | ✅ `findCutPoint()` | |
| Summary generation | ❌ | ✅ `generateSummary()` using LLM | |
| Compaction execution | ❌ | ✅ `compact()` with LLM | |
| Branch auto-summary | ❌ | ✅ `generateAutoBranchSummary()` | |
| Compaction entries in JSONL | ❌ | ✅ | |

**Gap**: hostd only has the skeleton types. No actual compaction execution, no LLM-based summarization, no auto-trigger logic.

---

## 8. Approval Handling

| Feature | hostd | host-runtime | Notes |
|---------|-------|-------------|-------|
| Approval request | ✅ via `OrchEvent::RequestApproval` | ✅ via `ApprovalRequested` | |
| Approval resolve | ✅ `ApprovalRespond` command | ✅ `resolveApproval()` | |
| Auto-accept/decline | ❌ | ✅ `createAutoAcceptHandler()` | |
| Session-level approval | ❌ | ✅ `accept_session` scope | |
| Workspace-level approval | ❌ | ✅ `accept_workspace` scope | |

**Gap**: Auto-accept handlers and approval scoping (session/workspace permanent) are missing.

---

## 9. Resource Discovery & Context Files

| Feature | hostd | host-runtime | Notes |
|---------|-------|-------------|-------|
| Resource discovery | ❌ | ✅ `discoverResources()` | Scans for skills, prompts, themes, context files |
| Theme loading | ❌ | ✅ Theme YAML loading | |
| Context file listing | ✅ `load_context_files()` | ✅ `getContextFiles()` | |

**Gap**: `discoverResources()` scans the filesystem and returns structured diagnostics. hostd loads resources individually.

---

## 10. MCP Tool Providers

| Feature | hostd | host-runtime | Notes |
|---------|-------|-------------|-------|
| MCP server management | ❌ | ✅ `McpServerManager` | Auto-start/stop MCP servers |
| MCP tool discovery | ❌ | ✅ `McpToolProvider.discover()` | |
| MCP tool execution | ❌ | ✅ `McpToolProvider.execute()` | |

**Gap**: Entirely missing. host-runtime has a full MCP client integration (158 lines). This is needed for IDE/editor integrations.

---

## 11. Other Features

| Feature | hostd | host-runtime | Notes |
|---------|-------|-------------|-------|
| HTML export | ❌ | ✅ `exportToHtml()` | |
| Debug tracing | ❌ | ✅ `DebugTrace` + file trace | |
| Image processing | ❌ | ✅ `ImageAttachment`, resize, dimensions | |
| Git integration | ❌ | ✅ `getGitBranch()` | |
| File argument processing | ❌ | ✅ `processFileArguments()` | |
| Editor draft replacement | ❌ | ✅ `EditorDraftReplacement` type | |
| Timing/telemetry | ❌ | ✅ `Timings` utility | |
| Settings change notifications | ❌ | ✅ `SettingsManager.onChange()` | |
| Orchestrator graph | ❌ | ✅ `getOrchestratorGraph()` | |
| LLM call (raw completion) | ❌ | ✅ `llmCall()` | |

---

## 12. Summary

### ✅ Features already done in hostd
- Basic session create/open/delete
- Turn submit + cancel
- Streaming events (TextDelta, ThinkingDelta, ToolStart/End, Approval)
- System prompt builder with skills, context files, templates
- Skills loading + prompt template loading
- Model registry + auth storage (API keys)
- JSONL persistence (create, open, list, fork, import, navigate)
- Compaction types (no execution yet)

### ❌ Critical features missing in hostd
1. **Session operations**: List, rename, fork, navigate, import, clone, switch — need to wire `JsonlSessionRepository` methods into `HostServer::apply_command()`
2. **Queue management**: Steer/follow-up/next-turn queues, `PromptBehavior` routing
3. **Compaction execution**: LLM-based summarization, auto-trigger, cut-point logic
4. **Multi-agent**: Sub-task spawning, inter-task steering
5. **OAuth**: Device code flow, PKCE, browser-based auth
6. **MCP tool providers**: MCP server management and tool integration
7. **Runtime model switching**: Change model/tools without restart
8. **Session metadata entries**: Write model_change, thinking_level_change, compaction entries to JSONL

### ⚠️ Partial features
- Approval (missing scoping and auto-handlers)
- Skills (missing model_override/thinking_level override in prompt formatting)
- Session persistence (storage layer exists but not fully wired to server commands)

### Features NOT needed in hostd (architectural difference)
- `OrchdRpcClient` — hostd links orchd directly
- `EventStream` — hostd uses channels + JSON-lines
- `OrchWireEvent` / `HostRuntimeEvent` — already deleted, hostd uses `HostEvent` directly
- `HostLifecycleEvent` — already deleted, absorbed into `HostEvent`
- `projectHostEvent()` — already deleted
- `PikoHost.create()` factory — replaced by `HostServer` struct
- In-process TUI integration — TUI connects via JSON-lines
- `discoverResources()` as a separate scan — hostd loads each resource type individually
- `EditorDraftReplacement` / Editor integration — TUI concern
- HTML export — not needed in daemon
