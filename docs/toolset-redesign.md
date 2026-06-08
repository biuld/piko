# piko Toolset Redesign Plan

## Summary

当前 `piko-engine-native` 的内置工具是一个文件操作工具箱:

- `read`
- `write`
- `edit`
- `bash`
- `grep`
- `find`
- `ls`

这套工具能跑通最小 coding loop,但长期方向不应该继续补 `copy` / `move` / `delete` / `stat` / `tree` 这类细粒度文件 CRUD。参考 `/Users/biu/Projects/codex` 后,更合适的方向是 Codex-like 的少量高杠杆 primitive:

- shell/exec: 通用命令执行入口
- apply_patch: 专用、可审计的代码编辑入口
- view_image: 本地图片查看入口
- update_plan: Host-visible 计划状态入口
- request_user_input: Host-mediated 交互输入入口
- tool_search: 延迟工具发现入口
- MCP/dynamic/namespace tools: 外部能力扩展入口

本设计文档给 deepseek 或后续实现者一个可落地迁移方案:保留兼容,逐步收敛工具面,避免继续扩大 legacy file tools。

## Reference From Codex

本地 Codex 参考路径:

- `/Users/biu/Projects/codex/codex-rs/tools/src/tool_spec.rs`
- `/Users/biu/Projects/codex/codex-rs/tools/src/tool_executor.rs`
- `/Users/biu/Projects/codex/codex-rs/tools/src/tool_discovery.rs`
- `/Users/biu/Projects/codex/codex-rs/core/src/tools/handlers/mod.rs`
- `/Users/biu/Projects/codex/codex-rs/prompts/templates/apply_patch_tool_instructions.md`
- `/Users/biu/Projects/codex/codex-rs/protocol/src/plan_tool.rs`

Codex 的重要设计点:

1. 工具暴露是分层的。
   `ToolExposure` 有 `Direct` / `Deferred` / `DirectModelOnly` / `Hidden`。不是所有工具都直接塞进模型上下文。

2. 工具定义和执行器绑定。
   `ToolExecutor` 同时提供 `tool_name()`、`spec()`、`exposure()`、`handle()`。这避免 spec/runtime 分离后漂移。

3. 编辑文件通过 `apply_patch`。
   Codex 没有把 `copy` / `move` / `delete` 这类操作作为主模型工具堆给模型,而是让模型用 patch 表达代码变更。

4. Shell 是通用探索与验证入口。
   文件读取、搜索、构建、测试、git 检查都可以走 shell,再配合 permission/sandbox policy 控制风险。

5. Tool discovery 是一等能力。
   `tool_search` 用于延迟发现插件/MCP/connector 工具,避免上下文被长尾工具污染。

## Current Problems In piko

### 1. 工具面过细

`read` / `grep` / `find` / `ls` / `write` / `edit` 这种组合容易继续膨胀出更多文件工具。短期看方便,长期会带来:

- 工具 schema 越来越多,占用模型上下文。
- 多个工具表达同一件事,模型选择成本上升。
- approval policy 被分散到很多小工具。
- Host/Engine 边界会被文件系统细节拖宽。

### 2. `bash` 命名与语义不够通用

当前工具名 `bash` 暗示 Unix bash,但目标 CLI 应该支持:

- 用户默认 shell
- macOS zsh
- Linux sh/bash
- Windows PowerShell/cmd,如果后续支持
- sandbox/approval policy
- stdout/stderr 生命周期
- long-running command handling

更合适的抽象名是 `shell` 或 `exec_command`。

### 3. 编辑工具不够可审计

`write` 和 `edit` 可以完成修改,但:

- `write` 容易整文件覆盖。
- `edit` 依赖 exact text replacement,复杂修改时容易失败或表达不清。
- 多文件修改没有统一 patch envelope。
- approval UI 难以展示完整变更意图。

应迁移到 `apply_patch`。

### 4. 缺少 Host-mediated tools

`update_plan`、`request_user_input`、`view_image` 这类工具不应该是普通 engine-native 文件工具。它们需要 Host/TUI 协作:

- `update_plan`: 更新 UI 可见状态,不是 transcript 文件操作。
- `request_user_input`: 暂停 turn 等用户输入。
- `view_image`: 需要 Host 或 runtime 读取图片并转成模型/客户端可展示事件。

### 5. 缺少 deferred tool model

当前 `EngineTool[]` 都是直接传给 provider。没有区分:

- 直接暴露给模型的工具
- 仅注册但通过搜索发现的工具
- hidden/internal 工具
- model-only 工具

这限制了 MCP/plugin 扩展。

## Target Tool Surface

目标工具面分成四类。

### Core Direct Tools

默认直接暴露给模型,覆盖常规 coding agent 主路径。

#### `shell`

通用命令执行工具。替代或别名当前 `bash`。

Schema:

```ts
{
  command: string;
  timeout?: number;
  cwd?: string;
  login?: boolean;
}
```

第一版可只支持:

- `command`
- `timeout`

后续再加:

- `cwd`
- `login`
- `shell`
- long-running session id
- stdin/write/resize

Execution:

- executor target: `shell`
- default `executionMode: "sequential"`
- default `requiresApproval: true`,除非 approval policy 明确允许
- output must include:
  - command
  - exitCode
  - stdout
  - stderr
  - durationMs
  - timedOut
  - truncated flags

Migration:

- 保留 `bash` 作为 alias 一段时间。
- prompt 中停止推荐 `bash`,改推荐 `shell`。
- 内部实现可先复用现有 `bashTool`。

#### `apply_patch`

专用代码编辑工具。目标是替代 `write` 和 `edit` 成为默认编辑路径。

建议使用 Codex patch grammar:

```text
*** Begin Patch
*** Add File: path
+content
*** Update File: path
@@
-old
+new
*** Delete File: path
*** End Patch
```

Schema 方案有两个选择。

Option A: freeform/custom tool,最接近 Codex:

```ts
{
  patch: string;
}
```

Option B: function tool:

```ts
{
  patch: string;
}
```

piko 当前 `EngineTool.inputSchema` 是普通 JSON schema,因此第一阶段建议使用 Option B。后续如果 provider 支持 freeform tool,再迁移。

Execution:

- executor target: `apply_patch`
- default `executionMode: "sequential"`
- default `requiresApproval: true`
- patch paths must be relative to workspace cwd
- reject absolute paths
- reject path traversal outside cwd
- return structured summary:
  - filesAdded
  - filesUpdated
  - filesDeleted
  - filesMoved
  - hunksApplied
  - errors

Implementation options:

1. Port minimal parser from Codex apply-patch.
2. Implement a strict subset first:
   - Add File
   - Delete File
   - Update File with hunks
   - Move to
3. Avoid shelling out to external `apply_patch` executable.

Recommendation:

- Implement parser in TypeScript under `packages/engine-native/src/tools/apply-patch/`.
- Keep grammar strict.
- Add golden tests before enabling by default.

#### `update_plan`

Plan/status tool. This should not mutate files.

Schema based on Codex `plan_tool.rs`:

```ts
{
  explanation?: string;
  plan: Array<{
    step: string;
    status: "pending" | "in_progress" | "completed";
  }>;
}
```

Rules:

- At most one `in_progress` item.
- A plan must have at least one item.
- This tool should emit a Host lifecycle event, not just a tool result string.

Implementation boundary:

- Do not implement this as a pure native file tool.
- Add a Host-mediated executor kind or callback path.
- Engine may expose the tool spec, but Host should own visible plan state.

Suggested first implementation:

- Add `executor.kind: "host"` to protocol, or use existing `remote` temporarily with target `host:update_plan`.
- Host injects a native executor that updates session/UI state and returns a concise result.

#### `view_image`

Local image inspection tool.

Schema:

```ts
{
  path: string;
  detail?: "high" | "original";
}
```

Rules:

- read-only
- no approval by default
- only image MIME/file extensions allowed
- paths resolved relative to cwd unless absolute paths are explicitly permitted by Host policy

Output:

- tool result should include a short textual acknowledgement
- lifecycle event should carry image payload/reference for UI/model path

Implementation boundary:

- If provider supports image tool result blocks, engine can return image content.
- If not, Host/TUI should at least render image preview metadata.

### Core Deferred Tools

Registered but not directly exposed unless discovered.

#### `tool_search`

Search deferred tools.

Schema:

```ts
{
  query: string;
  limit?: number;
}
```

Output:

```ts
{
  tools: Array<{
    name: string;
    description?: string;
    source?: string;
    toolType?: "builtin" | "mcp" | "plugin" | "dynamic";
  }>;
}
```

Purpose:

- Let piko register many optional tools without polluting the prompt.
- Support MCP/plugin expansion later.

First implementation:

- Add `ToolExposure` to `EngineTool`.
- `tool_search` searches only deferred `EngineTool` definitions.
- Return matching tools by name/description using simple substring scoring first.
- Do not implement BM25 initially.

### Compatibility Tools

These exist today and should not disappear abruptly:

- `read`
- `write`
- `edit`
- `grep`
- `find`
- `ls`
- `bash`

Target status:

- `bash`: alias to `shell`, then deprecate.
- `write` / `edit`: legacy compatibility, prompt should prefer `apply_patch`.
- `read` / `grep` / `find` / `ls`: optional compatibility set. Keep for now because tests and current prompts likely depend on them.

Do not add:

- `copy`
- `move`
- `delete`
- `stat`
- `tree`
- `read_many`
- `replace`

These should be done with `shell` or `apply_patch`.

### Extension Tools

Future-facing categories:

- MCP namespace tools
- dynamic tools
- plugin install/request tools
- web search
- image generation

Do not implement all now. Design protocol so they fit later.

## Protocol Changes

### Add Tool Exposure

Current:

```ts
export interface EngineTool {
  name: string;
  description: string;
  inputSchema: unknown;
  executor: EngineToolExecutorRef;
  executionMode?: "sequential" | "parallel";
  metadata?: Record<string, unknown>;
}
```

Proposed:

```ts
export type EngineToolExposure =
  | "direct"
  | "deferred"
  | "direct_model_only"
  | "hidden";

export interface EngineTool {
  name: string;
  description: string;
  inputSchema: unknown;
  executor: EngineToolExecutorRef;
  executionMode?: "sequential" | "parallel";
  exposure?: EngineToolExposure;
  metadata?: Record<string, unknown>;
}
```

Behavior:

- undefined exposure means `"direct"` for backward compatibility.
- provider runner sends only direct/direct_model_only tools.
- engine registry may execute hidden/deferred tools if explicitly routed internally.
- `tool_search` sees deferred tools.

### Add Host Executor Kind

Current:

```ts
kind: "native" | "remote" | "sandbox" | "mcp";
```

Proposed:

```ts
kind: "native" | "host" | "remote" | "sandbox" | "mcp";
```

Why:

- `update_plan`, `request_user_input`, and maybe `view_image` are Host-mediated.
- Engine-native should not own TUI state.

Alternative:

- Keep protocol unchanged and inject these as `native` executors from Host.
- This is faster but less explicit.

Recommendation:

- Add `"host"` once Host-mediated tool execution is implemented.
- Until then, do not expose `update_plan`.

### Strengthen Tool Metadata

Move common metadata out of generic `Record<string, unknown>` over time:

```ts
export interface EngineToolMetadata {
  requiresApproval?: boolean;
  readOnly?: boolean;
  destructive?: boolean;
  title?: string;
  legacy?: boolean;
}
```

Near-term compatibility:

- Continue reading `metadata.requiresApproval`.
- New code may write typed metadata.

## Engine-Native Directory Design

Current relevant structure:

```text
packages/engine-native/src/
  engine.ts
  provider-runner.ts
  tool-runner.ts
  tools/
  state/
```

Recommended structure:

```text
packages/engine-native/src/
  engine.ts
  provider-runner.ts
  runtime-limits.ts
  state/
    approval-resolution.ts
    continuation-state.ts
    step-state-machine.ts
    transcript-delta.ts
  tools/
    builtin/
      legacy-file-tools.ts
      shell.ts
      apply-patch/
        parser.ts
        apply.ts
        types.ts
        index.ts
      view-image.ts
      tool-search.ts
    registry.ts
    executor-dispatcher.ts
    exposure.ts
```

Near-term minimal change:

- Keep existing `tools/*.ts` files.
- Add new files:
  - `tools/shell.ts`
  - `tools/apply-patch.ts` or `tools/apply-patch/index.ts`
  - `tools/tool-search.ts`
  - `tools/exposure.ts`
- Later move legacy tools into `tools/legacy/`.

Avoid doing a large file move in the same commit as behavior changes.

## Execution Dispatcher

Current `tool-runner.ts` directly uses:

```ts
registry[toolDef.executor.target]
```

This blocks `host`, `mcp`, `sandbox`, and `remote` kinds from becoming real.

Add:

```ts
export interface ToolExecutorDispatcher {
  execute(
    tool: EngineTool,
    args: Record<string, unknown>,
    context: ToolExecutionContext,
  ): Promise<unknown>;
}

export interface ToolExecutionContext {
  signal?: AbortSignal;
  callId: string;
  emit: (event: EngineEvent) => void;
}
```

First implementation:

```ts
class NativeToolExecutorDispatcher implements ToolExecutorDispatcher {
  constructor(private registry: NativeToolRegistry) {}

  async execute(tool, args) {
    if (tool.executor.kind !== "native") {
      throw new Error(`Unsupported executor kind: ${tool.executor.kind}`);
    }
    const fn = this.registry[tool.executor.target];
    if (!fn) throw new Error(`No executor registered for target: ${tool.executor.target}`);
    return fn(args);
  }
}
```

Then `tool-runner.ts` depends on dispatcher, not registry.

This is prerequisite for Host-mediated tools and MCP.

## Provider Exposure Rules

Provider runner currently maps all active tools to provider tools.

Change provider selection:

```ts
function isDirectModelTool(tool: EngineTool): boolean {
  const exposure = tool.exposure ?? "direct";
  return exposure === "direct" || exposure === "direct_model_only";
}
```

Rules:

- `direct`: visible to model and executable.
- `direct_model_only`: visible to model but not nested code-mode later.
- `deferred`: not in initial provider request; searchable via `tool_search`.
- `hidden`: not visible; executable only by internal engine/host logic.

For piko first pass:

- Add exposure type.
- Filter provider tools.
- No code mode support yet.

## Builtin Tool Set Target

### Phase 1 Default Builtins

Default direct tools:

- `shell`
- `apply_patch`

Default direct or host-mediated tools, only if implemented:

- `view_image`
- `update_plan`

Default deferred tools:

- `read`
- `grep`
- `find`
- `ls`

Legacy direct tools during migration:

- Keep current direct exposure for `read`, `grep`, `find`, `ls` until prompts and tests are adjusted.
- Mark `write`, `edit`, `bash` as `metadata.legacy = true`.

Recommended first actual default:

```text
direct:
  shell
  apply_patch
  read
  grep
  find
  ls
legacy direct:
  bash
  write
  edit
```

After prompt migration:

```text
direct:
  shell
  apply_patch
  read
  grep
  find
  ls
deferred:
  write
  edit
hidden/alias:
  bash
```

Long-term:

```text
direct:
  shell
  apply_patch
  update_plan
  view_image
  tool_search
deferred:
  read
  grep
  find
  ls
legacy hidden:
  write
  edit
  bash
```

## Prompt Changes

Update native system prompt generation so tool guidance matches target.

Current prompt likely says tools exist generically. New guidance should say:

- Use `shell` for exploration, search, running tests, and inspecting repository state.
- Use `apply_patch` for file edits.
- Prefer `apply_patch` over `write` or `edit`.
- Do not use `write` to rewrite whole files unless explicitly necessary.
- Use `update_plan` for multi-step work when available.
- Use `tool_search` when a needed specialized tool may exist but is not visible.

Do not mention unavailable tools.

Implementation file:

- `packages/engine-native/src/system-prompt.ts`

## Approval Policy

Suggested defaults:

| Tool | Approval | Reason |
|---|---:|---|
| `shell` | required by default | arbitrary command execution |
| `apply_patch` | required by default | file mutation |
| `view_image` | not required | read-only local resource |
| `update_plan` | not required | UI/session state only |
| `tool_search` | not required | read-only metadata |
| `read` | not required | read-only |
| `grep` | not required | read-only |
| `find` | not required | read-only |
| `ls` | not required | read-only |
| `write` | required | file mutation |
| `edit` | required | file mutation |
| `bash` | required | alias to shell |

Important:

- `acceptForSession` should be interpreted by Host policy, not stored in engine-native hidden memory.
- Approval state must remain serializable in `EngineContinuationState`.

## Migration Plan

### Phase 0: Design Commit Only

Create this document. No tool behavior changes.

### Phase 1: Protocol And Registry Foundations

Files:

- `packages/engine-protocol/src/engine.ts`
- `packages/engine-native/src/tools/registry.ts`
- `packages/engine-native/src/provider-runner.ts`
- `packages/engine-native/src/tool-runner.ts`

Tasks:

1. Add `EngineToolExposure`.
2. Add optional `exposure` to `EngineTool`.
3. Filter provider-visible tools by exposure.
4. Add typed metadata interface or helper accessors.
5. Add tests:
   - deferred tool is not sent to provider
   - direct tool is sent to provider
   - hidden tool is not sent

Acceptance:

- Existing behavior unchanged when exposure is omitted.
- `bun run check` passes.
- `bun run test` passes.

### Phase 2: Shell Rename/Alias

Files:

- `packages/engine-native/src/tools/bash.ts`
- `packages/engine-native/src/tools/registry.ts`
- `packages/engine-native/test/builtin-tools.test.ts`

Tasks:

1. Add `shell` tool definition.
2. Reuse existing `bashTool` implementation or rename implementation to `shellTool`.
3. Keep `bash` as alias with `metadata.legacy = true`.
4. Update descriptions:
   - `shell`: execute a shell command in the workspace.
   - `bash`: legacy alias for shell.
5. Add test that `shell` executes command.
6. Keep existing `bash` test.

Acceptance:

- `shell` and `bash` both work.
- Approval metadata remains true.

### Phase 3: Apply Patch Tool

Files:

- `packages/engine-native/src/tools/apply-patch/`
- `packages/engine-native/src/tools/registry.ts`
- `packages/engine-native/test/apply-patch.test.ts`
- `packages/engine-native/src/system-prompt.ts`

Tasks:

1. Implement parser for Codex patch grammar subset.
2. Implement apply engine with cwd-bound path validation.
3. Reject absolute paths.
4. Reject paths escaping cwd.
5. Support:
   - Add File
   - Delete File
   - Update File
   - Move to
6. Return structured summary.
7. Add tool definition with `requiresApproval: true`.
8. Update prompt to prefer `apply_patch`.

Tests:

- add file
- update file
- delete file
- move file
- reject absolute path
- reject `../` escape
- reject malformed patch
- reject duplicate ambiguous hunk
- summary includes changed files

Acceptance:

- `apply_patch` is direct by default.
- `write`/`edit` still work.
- Tests cover safe failure modes.

### Phase 4: Tool Search / Deferred Tools

Files:

- `packages/engine-native/src/tools/tool-search.ts`
- `packages/engine-native/src/tools/registry.ts`
- `packages/engine-native/src/provider-runner.ts`
- `packages/engine-native/test/tool-search.test.ts`

Tasks:

1. Add deferred exposure support.
2. Add `tool_search` direct tool.
3. Build search index from deferred tools.
4. Match query against name and description.
5. Return max `limit`, default 8.

Open design question:

- After finding a deferred tool, how does the model call it?

Options:

1. Return names and let next step include active tools changed by Host.
2. Use Responses API deferred loading if provider supports it.
3. Keep piko v1 simple: tool_search is informational only until Host can activate tools.

Recommendation:

- Implement exposure and search first.
- Defer activation mechanics unless needed.

### Phase 5: Host-Mediated Tools

Files:

- `packages/engine-protocol/src/engine.ts`
- `packages/host-runtime/src/`
- `packages/engine-native/src/tool-runner.ts`

Tasks:

1. Decide whether to add `executor.kind = "host"`.
2. Add Host executor registry or callback.
3. Implement `update_plan`.
4. Implement `request_user_input` only after TUI flow is designed.
5. Add lifecycle events for plan update.

Acceptance:

- `update_plan` updates Host-visible state.
- It does not write transcript as plain text only.
- TUI can render plan separately from assistant text.

### Phase 6: View Image

Files:

- `packages/engine-native/src/tools/view-image.ts`
- `packages/host-runtime/src/`
- `packages/host-tui/src/`

Tasks:

1. Add tool spec.
2. Validate path and image type.
3. Decide output representation:
   - image content block in transcript
   - host lifecycle event
   - both
4. Add TUI renderer support if missing.

Acceptance:

- model/tool result can reference image safely.
- TUI shows image metadata or preview path.

## Testing Strategy

### Protocol Tests

Add tests around:

- direct/deferred/hidden exposure filtering
- executor kind handling
- metadata helper behavior

### Native Tool Tests

Add tests around:

- shell alias
- apply_patch parser
- apply_patch filesystem mutation
- tool_search results

### Host Tests

Add tests around:

- update_plan lifecycle event
- plan state persistence, if intended
- request_user_input pause/resume, later

### Regression Tests

Keep existing tests for:

- approval resume
- multiple approval tools
- runtime limits
- active tools
- lifecycle events

## Compatibility Rules

Do not break current sessions immediately.

Rules:

1. Existing tool names remain valid for at least one migration cycle.
2. New prompt guidance may de-emphasize legacy tools before removing them.
3. Session transcripts containing `bash`, `write`, or `edit` tool calls must still render.
4. Engine must handle missing new tools gracefully when old sessions resume.

## Explicit Non-Goals

Do not implement these as builtin tools:

- `copy`
- `move`
- `delete`
- `stat`
- `tree`
- `read_many`
- `replace`

Rationale:

- `shell` covers inspection and file operations.
- `apply_patch` covers code edits and file moves/deletes in a more auditable way.
- More small tools increase prompt/tool selection overhead.

Do not implement MCP fully in this redesign.

Rationale:

- MCP needs separate connection/session lifecycle.
- The immediate prerequisite is executor dispatch + exposure model.

Do not implement web search as engine-native filesystem tool.

Rationale:

- Web search belongs to provider/native Responses tool or remote connector layer.

## Recommended Implementation Order For Deepseek

1. Add `EngineToolExposure` and provider filtering.
2. Add `shell` alias while keeping `bash`.
3. Add `apply_patch` parser and executor.
4. Update system prompt to prefer `shell` + `apply_patch`.
5. Add `tool_search` and deferred exposure.
6. Design Host executor kind for `update_plan`.
7. Implement `update_plan`.
8. Implement `view_image`.

Stop after each phase and run:

```bash
bun run check
bun run test
```

## Acceptance Criteria For The Redesign

The redesign is complete when:

1. The default model-visible editing path is `apply_patch`, not `write`/`edit`.
2. The default model-visible command path is `shell`, with `bash` only as compatibility alias.
3. Tool exposure supports at least direct/deferred/hidden.
4. Provider runner does not expose deferred/hidden tools by default.
5. `tool_search` can list deferred tools.
6. `update_plan` exists as Host-visible state, not just a text-only native tool.
7. Legacy tools still pass existing tests.
8. Full suite passes with `bun run test`.

