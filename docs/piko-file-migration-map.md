# piko 文件级迁移表

## 目的

这份文档把前两份方案落到文件级别。

目标不是精确到每一行代码怎么改，而是回答下面几个实际问题：

1. `pi` 里哪些文件应直接复用
2. 哪些文件适合复制到 `piko` 后改造
3. 当前 `piko` 里哪些文件应该保留
4. 哪些文件只是过渡实现，后面应废弃或重写

本文使用四种标记：

- `npm`：直接依赖上游包，不复制源码
- `copy`：复制现有源码到 `piko`，再做定点修改
- `rewrite`：保留职责，但在 `piko` 中重新实现
- `drop`：当前实现不应继续作为主线延续

## 总结

最核心的判断如下：

### 1. engine 相关

以当前 `piko` 为主，继续补实现：

- `packages/engine-protocol`
- `packages/engine-native`
- `packages/engine-remote`

这里主要是 `rewrite` 已经开始，后续继续修正即可。

### 2. host/session/runtime 相关

以 `pi` 现有代码为主要来源，采用 `copy + 改造`：

- `SessionManager`
- `AgentSessionRuntime`
- `AgentSession`
- slash command 注册信息

原因很简单：

- 这些能力在 `pi` 里已经成熟
- 当前 `piko` 只有简化壳
- 继续在壳上补齐，成本高于复制成熟代码后换 runtime 接口

### 3. TUI 相关

分两层处理：

- `@earendil-works/pi-tui` 继续走 `npm`
- `packages/coding-agent/src/modes/interactive/*` 里真正的产品壳逻辑，采用 `copy`

### 4. 当前 `piko/cli` 的大部分交互代码

视为过渡实现：

- 不建议继续扩展
- 应逐步迁出或废弃

## 目标结构回顾

目标目录仍然是：

```text
packages/
  engine-protocol/
  engine-native/
  engine-remote/
  host-runtime/
  host-tui/
  cli/
```

下面按来源拆分。

## A. 上游直接依赖：`npm`

这些不建议复制源码，直接依赖即可。

### `@earendil-works/pi-ai`

来源：

- `packages/ai`

建议方式：

- `npm`

用途：

- model registry
- provider stream
- auth/model helper
- faux provider 测试能力

备注：

- `piko` 里只保留必要的 bridge / adapter
- 不复制模型元数据和 provider 主实现

### `@earendil-works/pi-tui`

来源：

- `packages/tui`

建议方式：

- `npm`

用途：

- `TUI`
- `Editor`
- `Markdown`
- `Container` / `Box` / `Text`
- autocomplete / keybinding 基础设施

备注：

- 作为 UI toolkit 使用
- 不把 session/runtime 逻辑塞回 `host-tui`

## B. 上游优先复制：`copy`

这部分是 `piko` 重构的重点。

## 1. Session 与树结构

### 来源文件

- `packages/coding-agent/src/core/session-manager.ts`
- `packages/coding-agent/docs/sessions.md`

### 建议处理

- `copy`

### 目标落点

```text
packages/host-runtime/src/session/
  session-manager.ts
  session-state.ts
  session-store.ts
  session-serializer.ts
```

### 为什么不是直接重写

`SessionManager` 现在已经承接了太多成熟能力：

- JSONL session 文件格式
- 树形 parent/child 结构
- branch / fork / clone
- label / session_info / custom_message
- session context 构建
- list / open / continue / import / fork

这些不是“可以以后再补”的细节，而是 `pi` 产品层的主骨架。

当前 `piko` 的 session 只是：

- 一个轻量内存对象
- 一个 JSONL 消息文件
- 没有 tree
- 没有 branch/fork
- 没有 compaction/label/session info

继续围绕当前实现扩张，基本等于重写一个次优版 `SessionManager`。

### 复制后要改什么

不是原样照搬。至少要改：

1. 去掉旧 agent runtime 假设
2. 把 `pendingApproval` / `engineState` 纳入 session state
3. 为 stateless engine 的 step checkpoint 预留字段
4. 让 host-runtime 成为唯一 owner

### 在 `piko` 中的分类

- 现有 `packages/host-runtime/src/session-store.ts`：`drop`
- 新 `session-manager.ts`：`copy`

## 2. Runtime 切换与 session 生命周期

### 来源文件

- `packages/coding-agent/src/core/agent-session-runtime.ts`

### 建议处理

- `copy`

### 目标落点

```text
packages/host-runtime/src/runtime/
  host-runtime.ts
  session-runtime.ts
```

### 为什么要复制

这个文件已经包含了 session 主生命周期切换：

- `switchSession`
- `newSession`
- `fork`
- `importFromJsonl`
- runtime teardown / rebuild
- session replacement hooks

这正是 `piko Host` 将来必须承接的东西。

当前 `piko` 只有一个 `run(prompt)` 风格的线性入口，完全没有真正的 runtime host 概念。

### 复制后要改什么

1. 从旧 `AgentSession` 创建逻辑切到新的 stateless-engine host runtime
2. 去掉和旧 extension runner 直接耦合的部分
3. 保留 session replacement / teardown / rebind 机制
4. 为 approval 恢复和 engineState 恢复增加显式状态

### 在 `piko` 中的分类

- 现有 `packages/host-runtime/src/host.ts`：`drop`
- 现有 `packages/host-runtime/src/scheduler.ts`：`rewrite`
- 新 host runtime 生命周期层：`copy + 改造`

## 3. AgentSession 级产品逻辑

### 来源文件

- `packages/coding-agent/src/core/agent-session.ts`

### 建议处理

- `copy`

### 目标落点

```text
packages/host-runtime/src/session/
  host-session.ts
```

### 为什么不是直接沿用当前 `piko` host

`AgentSession` 虽然里面混有旧 runtime 语义，但它也承接了大量产品层逻辑：

- session stats
- bash execution 集成
- queued follow-up / steering 状态
- session tree navigation
- export / import 周边
- compaction 和 branch summary 流程
- model / thinking 变更
- event 归并与 session persistence

这些逻辑在当前 `piko` 里几乎都是空白。

### 复制后要拆掉什么

必须拆掉旧 ownership：

- 旧 `agent.subscribe(...)` 主循环依赖
- `beforeToolCall` / `afterToolCall` hook ownership
- 直接调用旧 agent loop 的部分

### 复制后保留什么

优先保留：

- host 级别的 session API 形状
- stats / export / bash / queue / tree 这些产品逻辑
- 用户能感知到的成熟行为

### 在 `piko` 中的分类

- 不是原样保留
- 也不是全新从零写
- 明确属于 `copy + 大幅定点改造`

## 4. Slash command 清单与路由基础

### 来源文件

- `packages/coding-agent/src/core/slash-commands.ts`

### 建议处理

- `copy`

### 目标落点

```text
packages/host-runtime/src/commands/
  slash-commands.ts
  command-dispatcher.ts
```

### 理由

这个文件本身很轻，但它定义了产品层命令面：

- `/settings`
- `/model`
- `/export`
- `/import`
- `/share`
- `/name`
- `/session`
- `/fork`
- `/tree`
- `/new`
- `/compact`
- `/resume`

当前 `piko` TUI 里只有几个简化命令，而且是写死在 view 层。

这条路不应该继续走。

### 复制后要改什么

- 命令执行目标改成 `host action`
- 不再由 TUI 直接解释和执行 session 操作

## 5. Interactive Mode 主壳

### 来源文件

- `packages/coding-agent/src/modes/interactive/interactive-mode.ts`

### 建议处理

- `copy`

### 目标落点

```text
packages/host-tui/src/
  interactive-mode.ts
```

或者：

```text
packages/host-tui/src/
  tui-app.ts
  interactive-mode.ts
```

### 为什么必须复制

这个文件虽然很大，但它本质上就是 `pi` 的产品级 TUI 壳：

- session picker
- tree selector
- model selector
- settings selector
- footer / header / widgets
- streaming 渲染
- bash execution 组件化展示
- slash command 入口
- approval / status / compaction / retry 等 UI 行为

当前 `piko` 的 `tui.ts` 只覆盖了其中极小一部分。

如果不复制这个壳，`piko` 后面会一直停留在 demo shell。

### 复制后一定要改什么

这点很关键：不能只是把它搬过去。

必须把它的主依赖从：

- `AgentSessionRuntime`
- `AgentSession`

改成：

- `PikoHostRuntime`
- host state / actions / subscriptions

也就是说：

- 保留 UI 外形和交互骨架
- 替换掉底层 runtime ownership

### 在 `piko` 中的分类

- 当前 `packages/cli/src/tui.ts`：`drop`
- 新 `host-tui` 主壳：`copy + 结构改造`

## 6. Interactive 组件目录

### 来源目录

- `packages/coding-agent/src/modes/interactive/components/*`

### 建议处理

- `copy`

### 目标落点

```text
packages/host-tui/src/components/
```

### 优先复制的组件

优先级最高：

- `assistant-message.ts`
- `user-message.ts`
- `tool-execution.ts`
- `bash-execution.ts`
- `footer.ts`
- `session-selector.ts`
- `tree-selector.ts`
- `model-selector.ts`
- `settings-selector.ts`
- `custom-editor.ts`
- `dynamic-border.ts`

次优先级：

- `branch-summary-message.ts`
- `compaction-summary-message.ts`
- `countdown-timer.ts`
- `oauth-selector.ts`
- `scoped-models-selector.ts`
- `session-selector-search.ts`

### 为什么适合复制

这些文件是产品级 UI 组件，不是协议逻辑。

它们依赖 `pi-tui`，但承接了上层交互语义：

- 消息怎么显示
- 工具执行怎么显示
- session tree 怎么选
- 模型和设置怎么选

这类代码在 `piko` 里不值得再手写一个简化版。

## 7. Theme 与交互视觉资源

### 来源文件

- `packages/coding-agent/src/modes/interactive/theme/theme.ts`
- `packages/coding-agent/src/modes/interactive/theme/dark.json`
- `packages/coding-agent/src/modes/interactive/theme/light.json`

### 建议处理

- `copy`

### 目标落点

```text
packages/host-tui/src/theme/
```

### 理由

当前 `piko` 的 `theme.ts` 只是极简颜色函数。

如果将来真的要承接 `pi` 的产品壳，主题系统没必要重新摸索。

## C. 当前 piko：保留并继续改造

这部分来自当前 `/Users/biu/Projects/piko`，建议保留方向。

## 1. `packages/engine-protocol`

### 当前文件

- `packages/engine-protocol/src/index.ts`

### 建议处理

- `keep`
- 性质：`rewrite` 已开始，继续演进

### 后续动作

- 拆分协议文件
- 修正 message / engineState / approval 语义

## 2. `packages/engine-native`

### 当前文件

- `src/engine.ts`
- `src/state-machine.ts`
- `src/provider-runner.ts`
- `src/tool-runner.ts`
- `src/approval-state.ts`
- `src/transcript-builder.ts`
- `src/llm-caller.ts`

### 建议处理

- `keep`
- 性质：`rewrite`

### 后续动作

- 修真正的 approval resume
- 修 error semantics
- 修 engineState 贯通
- 修多 step 行为

## 3. `packages/engine-remote`

### 当前文件

- `src/remote-engine.ts`
- `src/protocol.ts`

### 建议处理

- `keep`
- 性质：`rewrite`

### 后续动作

- 修 event routing
- 支持多 run 并发隔离

## D. 当前 piko：应迁移或重做

## 1. `packages/host-runtime`

### 当前文件分类

#### `packages/host-runtime/src/scheduler.ts`

- 分类：`rewrite`
- 保留职责，不保留当前实现

问题：

- 只做线性 step loop
- 不保存 `engineState`
- 不保存 `pendingApproval`
- 没有真实 session state

建议：

- 基于上游 `agent-session-runtime.ts` 的思路重做
- 吸收当前 scheduler 中对 stateless engine 的调用方式

#### `packages/host-runtime/src/host.ts`

- 分类：`drop`

问题：

- 只是一个 `run(prompt)` 小外壳
- 不是产品 host

建议：

- 被新的 `host-runtime` facade 取代

#### `packages/host-runtime/src/session-store.ts`

- 分类：`drop`

问题：

- 只是内存 helper
- 远不足以承接真实 session 管理

建议：

- 被 copy 过来的 `SessionManager` 体系替代

#### `packages/host-runtime/src/approval-controller.ts`

- 分类：`rewrite`

保留职责，但应改成两层：

1. host 对 `pendingApproval` 的状态管理
2. UI/CLI 对用户决策的采集

#### `packages/host-runtime/src/model-config.ts`

- 分类：`keep`

但应迁到：

```text
config/model-config.ts
```

#### `packages/host-runtime/src/model-loader.ts`

- 分类：`keep`

但应迁到：

```text
config/model-loader.ts
```

#### `packages/host-runtime/src/pi-llm-caller.ts`

- 分类：`keep`

但应迁到：

```text
integrations/pi-llm-caller.ts
```

#### `packages/host-runtime/src/bridge.ts`

- 分类：`keep`

但应迁到：

```text
integrations/bridge.ts
```

## 2. `packages/cli`

### `packages/cli/src/cli.ts`

- 分类：`keep`

但应瘦身为：

- 参数解析
- bootstrap
- 启动 interactive / non-interactive

### `packages/cli/src/tui.ts`

- 分类：`drop`

原因：

- TUI 自己维护 transcript
- TUI 自己管理 session 恢复
- TUI 自己调用 engine
- 和目标 host ownership 相冲突

### `packages/cli/src/config.ts`

- 分类：`drop`

原因：

- 实际是 session 文件持久化，不是 CLI config
- 应迁到 host-runtime/session

### `packages/cli/src/theme.ts`

- 分类：`drop`

原因：

- 会被 `host-tui` 里的主题系统取代

## E. 建议新增的 piko 文件

这是按目标结构推导出来的建议新增文件。

## `packages/host-runtime/src/session/`

建议新增：

- `session-manager.ts` `copy`
- `session-state.ts` `rewrite`
- `session-store.ts` `rewrite`
- `session-serializer.ts` `rewrite`
- `file-session-store.ts` `rewrite`

说明：

- `session-manager.ts` 负责树和 JSONL 语义
- `session-state.ts` 负责运行态 state，包括 `pendingApproval` / `engineState`
- 这样“存档结构”和“运行态结构”才不会混成一个东西

## `packages/host-runtime/src/runtime/`

建议新增：

- `host-runtime.ts` `rewrite`
- `session-runtime.ts` `copy + 改造`
- `scheduler.ts` `rewrite`
- `approval-controller.ts` `rewrite`

## `packages/host-runtime/src/commands/`

建议新增：

- `slash-commands.ts` `copy`
- `command-dispatcher.ts` `rewrite`
- `builtin-commands.ts` `rewrite`

## `packages/host-tui/src/`

建议新增：

- `interactive-mode.ts` `copy + 改造`
- `theme/theme.ts` `copy`
- `components/*` `copy`
- `index.ts` `rewrite`

## 优先实施顺序

如果按最小风险路径推进，建议顺序如下。

## 第 1 步：先建壳，不切行为

先新增目录：

- `packages/host-tui`
- `packages/host-runtime/src/session`
- `packages/host-runtime/src/runtime`
- `packages/host-runtime/src/commands`

此时先不删旧文件。

## 第 2 步：复制上游产品壳代码

先复制：

- `interactive-mode.ts`
- `components/*`
- `theme/*`
- `slash-commands.ts`
- `session-manager.ts`
- `agent-session-runtime.ts`

目的是先把真实产品层材料放进仓库。

## 第 3 步：替换 ownership

把复制过来的代码从旧 `AgentSession` 体系迁到新 host-runtime：

- session 切换
- tree / resume / new / fork
- TUI 订阅
- command dispatch

这是最重要的一步。

## 第 4 步：废弃当前 demo 壳

当新的 host-runtime + host-tui 跑通后，再逐步废弃：

- `packages/cli/src/tui.ts`
- `packages/cli/src/config.ts`
- 旧 `host.ts`
- 旧轻量 `session-store.ts`

## 简化判断表

下面给一张压缩版速查表。

| 来源文件 | 处理方式 | 目标位置 |
|---|---|---|
| `packages/ai/*` | `npm` | 直接依赖 |
| `packages/tui/*` | `npm` | 直接依赖 |
| `packages/coding-agent/src/core/session-manager.ts` | `copy` | `host-runtime/session/session-manager.ts` |
| `packages/coding-agent/src/core/agent-session-runtime.ts` | `copy` | `host-runtime/runtime/session-runtime.ts` |
| `packages/coding-agent/src/core/agent-session.ts` | `copy` | `host-runtime/session/host-session.ts` |
| `packages/coding-agent/src/core/slash-commands.ts` | `copy` | `host-runtime/commands/slash-commands.ts` |
| `packages/coding-agent/src/modes/interactive/interactive-mode.ts` | `copy` | `host-tui/interactive-mode.ts` |
| `packages/coding-agent/src/modes/interactive/components/*` | `copy` | `host-tui/components/*` |
| `packages/coding-agent/src/modes/interactive/theme/*` | `copy` | `host-tui/theme/*` |
| `piko/packages/engine-protocol/*` | `keep` | 原位继续演进 |
| `piko/packages/engine-native/*` | `keep` | 原位继续演进 |
| `piko/packages/engine-remote/*` | `keep` | 原位继续演进 |
| `piko/packages/host-runtime/src/scheduler.ts` | `rewrite` | `host-runtime/runtime/scheduler.ts` |
| `piko/packages/host-runtime/src/host.ts` | `drop` | 被新 host facade 取代 |
| `piko/packages/host-runtime/src/session-store.ts` | `drop` | 被新 session 层取代 |
| `piko/packages/host-runtime/src/approval-controller.ts` | `rewrite` | `host-runtime/runtime/approval-controller.ts` |
| `piko/packages/host-runtime/src/model-config.ts` | `keep` | `host-runtime/config/model-config.ts` |
| `piko/packages/host-runtime/src/model-loader.ts` | `keep` | `host-runtime/config/model-loader.ts` |
| `piko/packages/host-runtime/src/pi-llm-caller.ts` | `keep` | `host-runtime/integrations/pi-llm-caller.ts` |
| `piko/packages/host-runtime/src/bridge.ts` | `keep` | `host-runtime/integrations/bridge.ts` |
| `piko/packages/cli/src/cli.ts` | `keep` | `cli/cli.ts` |
| `piko/packages/cli/src/tui.ts` | `drop` | 被 `host-tui` 取代 |
| `piko/packages/cli/src/config.ts` | `drop` | 被 host session store 取代 |
| `piko/packages/cli/src/theme.ts` | `drop` | 被 `host-tui/theme` 取代 |

## 最终判断

最关键的迁移不是 engine，而是下面三条：

1. 把 `pi` 的 `SessionManager` 体系迁进来
2. 把 `pi` 的 interactive mode 产品壳迁进来
3. 用新的 stateless engine host-runtime 替换旧 agent runtime ownership

如果这三条不做，`piko` 会继续停留在“协议看起来像，产品层还是假的壳”这个状态。

如果这三条做了，`piko` 才会真正变成：

- 上层复用 `pi` 的成熟产品层
- 下层切换到新的 stateless engine 架构

这才是这次重构真正应该达到的形态。
