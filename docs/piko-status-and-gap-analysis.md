# piko 现状与差距分析

## 项目目标

**最终目标**：piko 启动后，应获得跟 pi 一样的使用体验、一样的存储结构，只是内部采用 stateless engine 架构。

换句话说：产品层不变，runtime 层重写。

---

## 已完成

### 架构：Stateless Engine 协议

```
┌──────────────────────────────────────────┐
│ piko Host                                 │
│ session / TUI / approvals / scheduling    │
└────────────────┬─────────────────────────┘
                 │ EngineInput / EngineEvent / EngineStepResult
                 ▼
┌──────────────────────────────────────────┐
│ Stateless Engine                          │
│ provider / tool execution / approval      │
└──────────────────────────────────────────┘
```

- `engine-protocol` — 共享类型 + EventStream + StatelessEngine 接口
- `engine-native` — step state machine + provider runner + tool runner + approval state machine
- `engine-remote` — JSON-RPC client，对接远端 engine

### Host Runtime：统一生命周期

- `PikoHost` — 单一入口：`PikoHost.create()` 创建，持有 engine + PikoSessionRuntime，暴露 `run()` / `streamPrompt()`
- `PikoSessionRuntime` — session 生命周期：switch / new / clone / fork / importFromJsonl + dispose
- `SessionManager` — JSONL 持久化：branch / tree / fork / clone / rename / delete
- Hook 体系：`onBeforeInvalidate` → `onSessionReplaced` → `onAfterRebind`
- Diagnostics：启动时收集 session 诊断信息

### TUI 产品壳

- Header：provider/model + session id + message count
- Chat：Markdown 渲染对话
- Spinner：流式响应时 Braille 动画
- Footer：cwd · session name + model + message count
- StatusLine：瞬态状态消息
- Editor：多行输入 + slash command autocomplete
- **Tree Selector**：完整 ASCII 树形组件，支持搜索 / 折叠 / 过滤 / active path
- **Session Selector**：带 parent-child 树的 session 列表，支持 scope 切换 / 重命名 / 删除
- **Fork Selector**：从 user message 分支创建新 session
- **PromptOverlay**：通用的文本输入浮层

### Slash Commands

| 命令 | 功能 |
|---|---|
| `/help` | 显示所有命令 |
| `/model` | 显示当前模型 |
| `/models` | 列出可用模型 |
| `/sessions` | 列出已保存 session |
| `/name <title>` | 设置 session 名称 |
| `/tree [entry-id]` | 显示或切换分支 |
| `/fork <entry-id>` | 从 early user message 创建新 session |
| `/clone` | 复制当前分支到新 session |
| `/resume <id>` | 恢复已保存 session |
| `/session` | 显示当前 session 信息 |
| `/new` / `/clear` | 新建 session |
| `/exit` | 退出 |

---

## 差距分析

三层，由近到远。

---

### 第一层：存储兼容性

让 piko 的 JSONL 输出和 pi 完全一致，**技术上没有阻塞性的矛盾**。所有差异的根因都在一个地方：

```
pi-ai stream           → 吐出完整数据（thinking_delta、完整 usage、stopReason、responseId...）
         ↓
piko 的 LlmCaller 接口  → 只定义了 text_delta + tool_call_start
         ↓                    只透传 {input, output, total}
piko 的 transcript-builder → 想写也没数据可写
         ↓
piko 的 JSONL           → 比 pi 少了一大截
```

本质上是 **adapter 层把数据筛掉了**，不是设计冲突。

---

#### 逐字段对照：pi 的 assistant 消息 vs piko

```
pi 的 JSONL：
{
  "type": "message",
  "id": "6e38c1ad",           ← entry ID 格式不同
  "parentId": "f11afa94",
  "timestamp": "...",
  "message": {
    "role": "assistant",
    "content": [
      {"type": "thinking", "thinking": "...", "thinkingSignature": "reasoning_content"},
      {"type": "toolCall", "id": "call_00_DQ...", "name": "bash", "arguments": {...}}
    ],
    "api": "openai-completions",                          ← 缺失
    "provider": "deepseek",                               ← 缺失
    "model": "deepseek-v4-pro",                           ← 缺失
    "usage": {                                            ← 缺失
      "input": 118, "output": 115,
      "cacheRead": 61952, "cacheWrite": 0,
      "totalTokens": 62185,
      "cost": { "input": 0.00005, "output": 0.00010,
                "cacheRead": 0.00022, "cacheWrite": 0,
                "total": 0.00037 }
    },
    "stopReason": "toolUse",                              ← 缺失
    "responseId": "94eacb4e-d2a8-4829-b9ed-7ae879547cee", ← 缺失
    "timestamp": 1780081558948
  }
}
```

| 缺失字段 | 数据是否已存在 | 在哪 | 为什么没到 JSONL |
|---|---|---|---|
| `thinking` 内容块 | ✓ | pi-ai stream 的 `thinking_delta` 事件 | `LlmEvent` 没定义，`provider-runner.ts` 不处理 |
| `api` | ✓ | `EngineModel.api` | `transcript-builder.ts` 构建消息时不写入 |
| `provider` | ✓ | `EngineModel.provider` | 同上 |
| `model` | ✓ | `EngineModel.id` | 同上 |
| `usage`（完整） | ✓ | pi-ai response 的 `AssistantMessage.usage` | `LlmResult` 只取了 `{input, output, total}` |
| `stopReason` | ✓ | pi-ai response | `LlmResult` 没透传 |
| `responseId` | ✓ | pi-ai response | `LlmResult` 没透传 |

**六个缺失字段，数据源全都有。差距只在 adapter 层的过滤。**

---

#### 需要改的地方：三处

**1. `LlmCaller` 接口扩展（核心改动）**

```typescript
// 当前：只有两个事件类型
export type LlmEvent =
  | { type: "text_delta"; delta: string }
  | { type: "tool_call_start"; id: string; name: string };

export interface LlmResult {
  message: Message;
  usage: TokenUsage;            // 只有 {input, output, total}
}

// 目标：
export type LlmEvent =
  | { type: "text_delta"; delta: string }
  | { type: "tool_call_start"; id: string; name: string }
  | { type: "thinking_delta"; delta: string }
  | { type: "thinking_done"; text: string };

export interface LlmResult {
  message: Message;
  usage: LlmUsage;              // 完整的 usage 结构
  stopReason?: string;
  responseId?: string;
}
```

`pi-llm-caller.ts` 当前实现把 pi-ai 的 `thinking_delta` 事件丢弃了。打开即可。

**2. protocol Message + ToolResultMessage 补充字段**

```typescript
// AssistantMessage.content 加 thinking 类型
export interface ThinkingContent {
  type: "thinking";
  thinking: string;
  thinkingSignature?: string;
}

// ToolResultMessage 加 details
export interface ToolResultMessage {
  role: "toolResult";
  toolCallId: string;
  toolName: string;
  content: (TextContent | ImageContent)[];
  isError: boolean;
  timestamp: number;
  details?: unknown;          // ← engine opaque，host 只持久化
}
```

`api` / `provider` / `model` / `stopReason` / `responseId` 不用放进 protocol Message 类型。它们由 `transcript-builder.ts` 在构建消息时从 `EngineInput.model` 和 `LlmResult` 直接写入 JSONL。要不在 protocol 层面定义它们——就当它们是 message 身上的 metadata，host 不解释。

**3. 格式对齐**

```typescript
// file-session-store.ts
const CURRENT_SESSION_VERSION = 3;  // 原来是 1

function generateEntryId(index: number): string {
  // 改成 8 字符 hex，跟 pi 一致
  return index.toString(16).padStart(8, "0").slice(-8);
}

// session-manager.ts
function createSessionId(): string {
  // 改成 UUID 格式
  return crypto.randomUUID();
}
```

---

#### 不需要改的

**目录**：`~/.piko` 保持独立。pi 兼容是 import/export 能力，不是共用目录。

**entry 类型**：`thinking_level_change`、`compaction`、`branch_summary`、`label`、`custom`——piko 不会生成这些条目。如果从 pi 导入 session，这些条目作为 opaque 行保留即可，piko 的 `FileEntry` 类型需要接受一个 fallthrough `unknown` 分支。

---

#### 存储兼容性总结

| 问题 | 性质 | 改动量 |
|---|---|---|
| thinking 内容块缺失 | `LlmCaller` + `transcript-builder` 没处理 | ~30 行 |
| usage 不完整 | `LlmCaller` 过滤了 cacheRead/cacheWrite/cost | ~15 行 |
| stopReason/responseId 缺失 | `LlmResult` 没透传 | ~5 行 |
| api/provider/model 不写入消息 | `transcript-builder` 没从 `EngineInput.model` 拷 | ~10 行 |
| toolResult.details 缺失 | protocol `ToolResultMessage` 没字段 | ~3 行 |
| version / ID 格式不同 | 常量 + 格式化函数 | ~5 行 |

全部改动加起来不到 100 行，集中在三个文件：
- `llm-caller.ts`（接口定义）
- `pi-llm-caller.ts`（adapter 实现，把筛子拆掉）
- `transcript-builder.ts`（构建消息时写入 metadata）

---

### 第二层：核心产品能力

piko 当前在 TUI 模式下**关闭了工具调用**：

```typescript
// host-tui/src/tui.ts — createHostConfig 中写死：
allowToolCalls: false,
maxSteps: 1,
stopConditions: { stopOnAssistantMessage: true },
```

这意味着 piko 的交互模式是**纯对话窗口**，不是 agent。而 pi 作为 coding agent 的核心价值在于工具调用链。

| 能力 | pi | piko | 说明 |
|---|---|---|---|
| **工具执行** | read/write/edit/bash/grep/find | ❌ 关闭 | piko engine 支持，TUI 没启用 |
| **工具结果展示** | ToolExecutionComponent（展开/折叠） | ❌ | 工具调用的 JSON 结果需要渲染 |
| **多步推理** | tool loop（maxSteps up to 100） | ❌ maxSteps=1 | engine 支持，host 没启用 |
| **Bash 执行展示** | 独立组件（stdout/stderr/exit code） | ❌ | |
| **模型切换** | `/model` + 热键循环 | ❌ 只能启动时指定 | |
| **Thinking 折叠** | 隐藏/显示思考过程 | ❌ | 需要存储层先支持 thinking 块 |
| **Compaction** | auto/manual compaction | ❌ | |
| **Token/cost 统计** | footer 显示累计 usage + cost | ❌ | 需要存储层先支持完整 usage |
| **上下文使用率** | footer 显示 token % | ❌ | |
| **Diff 展示** | 文件变更可视化 | ❌ | |
| **设置管理** | settings selector | ❌ 全部硬编码 | |
| **外部编辑器** | `Ctrl+E` 打开 $EDITOR | ❌ | |
| **图片粘贴** | `Ctrl+V` 粘贴剪贴板图片 | ❌ | |
| **Keybinding 系统** | 集中式 KeybindingsManager + 可配置 | ❌ | |
| **信号处理** | SIGINT 中断 / SIGTERM 优雅退出 / EIO 保护 | 只有基础 SIGINT | |
| **slash command 扩展** | `/settings` `/export` `/import` `/share` `/compact` `/diff` `/changelog` `/theme` | 缺 8 个命令 | |

---

### 第三层：扩展生态（第一阶段不追求）

| 能力 | 说明 |
|---|---|
| Extensions / Skills | pi 的插件系统，扩展注册 slash command / widget / hook |
| OAuth 登录 | 多账户选择 + 登录对话框 |
| Theme 系统 | dark/light + 自定义 theme 文件 + 热切换 |
| Changelog 通知 | 启动时显示新版本变更 |
| Auto-retry | 失败时自动重试 |
| Auto-compaction | context window 接近上限时自动压缩 |

---

## 建议实施路径

差距的核心只有一句话：**piko 不是 agent，只能聊天。**

跨越这个差距的最短路径：

### Step 1：存储层对齐（< 100 行改动）

- [ ] `LlmCaller` 接口加 `thinking_delta` / `thinking_done` 事件
- [ ] `LlmResult` 加完整 `usage`（cacheRead/cacheWrite/cost）+ `stopReason` + `responseId`
- [ ] `pi-llm-caller.ts` 打开 thinking 事件透传，完整 usage 透传
- [ ] `transcript-builder.ts` 从 `EngineInput.model` 写入 `api` / `provider` / `model`，从 `LlmResult` 写入 `usage` / `stopReason` / `responseId`
- [ ] protocol 加 `ThinkingContent` + `ToolResultMessage.details`
- [ ] session version → 3，entry ID → 短 hex，session ID → UUID

### Step 2：打开工具调用（最小可行 agent）

- [ ] TUI `streamPrompt()` 改为多步 loop（复用 `runScheduler`）
- [ ] 去掉 `allowToolCalls: false` 和 `maxSteps: 1`
- [ ] engine 事件（`tool_call_start` / `tool_call_end`）渲染到 chat

### Step 3：工具执行可视化

- [ ] `ToolExecutionComponent`：显示 tool call name + arguments + 展开/折叠 result
- [ ] Bash 执行展示（stdout/stderr/exit code）
- [ ] error 红色标记

### Step 4：模型切换 + Thinking 折叠

- [ ] Host 支持 session 级切换 model
- [ ] `/model <id>` slash command + 热键循环
- [ ] Thinking 内容块折叠/展开

### Step 5：补齐交互细节

- [ ] Keybinding manager（集中式 + 可配置）
- [ ] 外部编辑器 (Ctrl+E)
- [ ] SIGINT/SIGTERM 完善

### Step 6：补齐 pi 的斜杠命令

- [ ] `/export` / `/import` — 导出/导入 session 到文件
- [ ] `/settings` — 显示/修改设置
- [ ] `/diff` — 查看文件变更
- [ ] `/compact` — 手动压缩

---

## 文件一览

```
packages/
  engine-protocol/src/
    index.ts                    # 全部协议类型 + EventStream

  engine-native/src/
    engine.ts                   # createNativeEngine() 工厂
    state-machine.ts            # step + approval 状态机
    provider-runner.ts          # transcript → provider call
    tool-runner.ts              # tool call 执行 + approval 检查
    approval-state.ts           # pending approval 创建/校验
    transcript-builder.ts       # assistant/toolResult 消息构建
    llm-caller.ts               # 抽象 LLM caller 接口
    types.ts                    # 内部类型（EngineState, NativeToolRegistry）

  engine-remote/src/
    remote-engine.ts            # JSON-RPC engine client
    protocol.ts                 # RPC method 常量 + transport 接口

  host-runtime/src/
    host.ts                     # PikoHost（统一 host lifecycle）
    scheduler.ts                # runScheduler（多步 step loop）
    session-runtime.ts          # PikoSessionRuntime（生命周期 + import/dispose）
    session-manager.ts          # SessionManager（JSONL 持久化 + tree）
    session-store.ts            # SessionState（内存态 session）
    file-session-store.ts       # JSONL 文件读写 + meta 查询
    approval-controller.ts      # ApprovalHandler 接口 + auto accept/decline
    model-config.ts             # HostConfig 构造
    model-loader.ts             # 从 pi-ai 加载模型列表
    pi-llm-caller.ts            # pi-ai provider stream → LlmCaller
    bridge.ts                   # pi-ai ↔ protocol message 互转

  host-tui/src/
    tui.ts                      # 主 TUI（header/chat/footer/editor/streaming）
    components/
      tree-selector.ts          # 树形选择器（ASCII tree + search + filter + fold）
      session-tree-node.ts      # 树结构构建 + 展平 + 渲染
      footer.ts                 # FooterComponent（cwd/session/model/msgs）
      spinner.ts                # Braille 加载动画
      status-line.ts            # 瞬态状态消息
    selector-overlay.ts         # 通用选择器浮层（session/tree/fork 复用）
    prompt-overlay.ts           # 文本输入浮层
    session-tree.ts             # Session 树格式化 + select items
    theme.ts                    # Editor/Markdown 主题

  cli/src/
    cli.ts                      # 入口：参数解析 + 启动 TUI / 非交互模式
```
