# piko 模块重组草案

## 目标

这份文档回答一个具体问题：

**`piko` 从当前目录结构，应该怎么重组到更合理的目标结构。**

这里不讨论抽象原则，主要给出：

- 目标包结构
- 每个包的职责边界
- 当前包/文件应迁移到哪里
- 哪些包保留，哪些包要拆，哪些实现应视为过渡代码

## 结论

`piko` 的模块组织应该变，但不是全盘推翻。

### 不需要推翻的部分

- `packages/engine-protocol`
- `packages/engine-native`
- `packages/engine-remote`

这三个包的拆法基本成立，后续主要是补实现，不是改方向。

### 需要重组的部分

- `packages/host-runtime`
- `packages/cli`

当前问题不是包太多，而是 **host 层太薄、cli 层太厚、TUI 放错了位置**。

## 目标目录

建议目标结构固定为：

```text
packages/
  engine-protocol/
  engine-native/
  engine-remote/
  host-runtime/
  host-tui/
  cli/
```

必要时后续再加：

```text
  host-shared/
```

但第一轮不建议先加，避免过早抽公共层。

## 包职责

## 1. `engine-protocol`

职责：

- Stateless engine 协议
- 共享类型
- event/result/envelope

禁止负责：

- provider 调用
- tool 执行
- session
- TUI

结论：

- 保留
- 只做协议收敛和类型修正

## 2. `engine-native`

职责：

- step state machine
- provider runner
- tool runner
- approval pause/resume
- transcript 生成

禁止负责：

- session store
- TUI
- slash commands
- config UI

结论：

- 保留
- 重点补齐 approval resume、error semantics、engineState 传递

## 3. `engine-remote`

职责：

- JSON-RPC client
- remote event stream 映射
- remote approval resolution

禁止负责：

- host state
- UI
- transport 之外的 runtime 逻辑

结论：

- 保留
- 重点补事件隔离、并发 run 安全性

## 4. `host-runtime`

职责：

- session state
- session store
- run state
- engine scheduler
- approval state
- command dispatch
- host actions
- host event subscription
- config/model/runtime state 聚合

这应该成为产品主核心。

当前问题：

- 现在的 `host-runtime` 只是薄 scheduler
- session 还是 demo 级别数据结构
- approval / checkpoint 没进入 host 的一等状态

结论：

- 保留包名
- 大幅扩充职责
- 作为后续重构中心

## 5. `host-tui`

职责：

- 基于 `pi-tui` 的交互壳
- 渲染 host state
- 把用户输入转成 host actions
- 展示 session list / messages / approval / status

禁止负责：

- 直接维护 transcript
- 直接操作 engine stream
- 自己实现 session persistence

结论：

- 新增包
- 从当前 `cli` 中拆出 TUI 相关代码
- 后续优先承接从 `pi` copy 过来的 interactive shell

## 6. `cli`

职责：

- 二进制入口
- 参数解析
- 启动模式选择
- 初始化 config / engine / host / tui

禁止负责：

- TUI 细节
- session 持久化
- transcript 操作
- slash command 实现

结论：

- 保留
- 显著瘦身

## 当前到目标的迁移图

下面按当前包逐个说明。

## A. `packages/engine-protocol`

当前状态：

- 结构基本正确
- 类型边界基本集中

建议：

- 保持包名和目录不变
- 后续只修协议定义

当前文件：

```text
packages/engine-protocol/src/index.ts
```

建议演进：

```text
packages/engine-protocol/src/
  index.ts
  message.ts
  engine.ts
  events.ts
  approval.ts
  tool.ts
  usage.ts
```

说明：

- 不是必须立刻拆
- 但随着协议稳定，建议拆文件，避免 `index.ts` 持续膨胀

## B. `packages/engine-native`

当前状态：

- 目录拆分方向基本可接受
- 逻辑缺口主要是功能完整性，不是模块边界

当前文件：

```text
packages/engine-native/src/
  approval-state.ts
  engine.ts
  index.ts
  llm-caller.ts
  provider-runner.ts
  state-machine.ts
  tool-runner.ts
  transcript-builder.ts
  types.ts
```

建议：

- 保留整体结构
- `types.ts` 只保留内部 state machine 类型
- `llm-caller.ts` 作为注入接口继续保留

建议演进为：

```text
packages/engine-native/src/
  index.ts
  engine.ts
  state-machine.ts
  provider-runner.ts
  tool-runner.ts
  approval-state.ts
  transcript-builder.ts
  llm-caller.ts
  internal-types.ts
```

可选调整：

- `types.ts` 改名 `internal-types.ts`
- 避免与协议层类型混淆

## C. `packages/engine-remote`

当前状态：

- 结构足够简单
- 主要是实现细节待补

当前文件：

```text
packages/engine-remote/src/
  index.ts
  protocol.ts
  remote-engine.ts
```

建议：

- 保持
- 如果后面 transport 变复杂，再拆：

```text
packages/engine-remote/src/
  index.ts
  protocol.ts
  transport.ts
  remote-engine.ts
  notification-router.ts
```

第一轮不必先拆。

## D. `packages/host-runtime`

这是最需要重做的地方。

当前文件：

```text
packages/host-runtime/src/
  approval-controller.ts
  bridge.ts
  host.ts
  index.ts
  model-config.ts
  model-loader.ts
  pi-llm-caller.ts
  scheduler.ts
  session-store.ts
```

当前主要问题：

- `host.ts` 只是一个小包装器
- `session-store.ts` 只是内存 session helper，不是真 store
- `scheduler.ts` 没承接完整 host state
- `approval-controller.ts` 只是一个 prompt adapter
- `pi-llm-caller.ts` 和 `bridge.ts` 更像 engine integration，不像 host 核心

### 建议新结构

```text
packages/host-runtime/src/
  index.ts

  host/
    piko-host.ts
    host-state.ts
    host-actions.ts
    host-events.ts

  runtime/
    scheduler.ts
    run-controller.ts
    approval-controller.ts

  session/
    session-state.ts
    session-store.ts
    session-serializer.ts

  commands/
    command-dispatcher.ts
    slash-command.ts

  config/
    model-config.ts
    model-loader.ts

  integrations/
    pi-llm-caller.ts
    bridge.ts
```

### 文件迁移建议

#### `host.ts`

迁移到：

```text
host/piko-host.ts
```

并重写为真正的 host facade，而不是“run(prompt)”小壳。

#### `scheduler.ts`

迁移到：

```text
runtime/scheduler.ts
```

继续保留，但后续必须：

- 持有 engineState
- 持有 pendingApproval
- 持有 runState

#### `session-store.ts`

当前文件名保留，但语义要变。

迁移到：

```text
session/session-state.ts
session/session-store.ts
session/session-serializer.ts
```

拆分原因：

- `session-state.ts` 定义结构
- `session-store.ts` 定义读写接口
- `session-serializer.ts` 管 JSON/JSONL 文件格式

#### `approval-controller.ts`

迁移到：

```text
runtime/approval-controller.ts
```

然后把“用户怎么批”与“host 如何管理 pending approval”分开。

#### `model-config.ts` / `model-loader.ts`

迁移到：

```text
config/model-config.ts
config/model-loader.ts
```

#### `pi-llm-caller.ts` / `bridge.ts`

迁移到：

```text
integrations/pi-llm-caller.ts
integrations/bridge.ts
```

这样语义更清楚：

- 这不是 host 核心
- 这是 host 对上游 `pi-ai` 的集成层

## E. `packages/cli`

这是第二个需要明显瘦身的包。

当前文件：

```text
packages/cli/src/
  cli.ts
  config.ts
  theme.ts
  tui.ts
```

当前问题：

- `tui.ts` 承担了太多 host 逻辑
- `config.ts` 实际上是 session persistence
- `theme.ts` 是纯 TUI 资源

### 目标

`cli` 最终应该只剩下：

```text
packages/cli/src/
  cli.ts
  bootstrap.ts
```

必要时加：

```text
  args.ts
```

### 当前文件迁移建议

#### `cli.ts`

保留在 `cli`。

职责收缩为：

- 解析参数
- 选择 interactive / non-interactive
- 初始化 host runtime
- interactive 时启动 `host-tui`

#### `tui.ts`

迁移到新包：

```text
packages/host-tui/src/
  tui-app.ts
```

但不是原样搬过去，而是：

- 删掉它内部维护 transcript 的逻辑
- 删掉它内部直接调 engine 的逻辑
- 改成消费 host runtime

#### `config.ts`

不要留在 `cli`。

它本质上不是 CLI config，而是 session persistence。

迁移到：

```text
packages/host-runtime/src/session/
  file-session-store.ts
```

如果里面有纯目录辅助函数，也可以拆：

```text
packages/host-runtime/src/session/
  session-paths.ts
```

#### `theme.ts`

迁移到：

```text
packages/host-tui/src/theme.ts
```

## F. 新增 `packages/host-tui`

建议新增，而不是继续把 TUI 藏在 `cli` 里。

建议结构：

```text
packages/host-tui/src/
  index.ts
  tui-app.ts
  theme.ts
  views/
    header-view.ts
    chat-view.ts
    footer-view.ts
    session-list-view.ts
    approval-view.ts
  adapters/
    host-subscription.ts
```

第一轮不需要拆太细，但至少要先把 `tui.ts` 独立出来。

最低可行结构：

```text
packages/host-tui/src/
  index.ts
  tui-app.ts
  theme.ts
```

## “复制自 pi”的代码应放哪里

如果从 `pi` copy 交互 shell 代码，建议优先落在：

```text
packages/host-tui/
```

如果复制的是 session / slash-command / host 交互协调代码，优先落在：

```text
packages/host-runtime/
```

不要把 copy 来的产品层代码放进：

- `engine-native`
- `cli`

否则后面还是会重新缠在一起。

## 当前实现哪些应标记为过渡代码

以下内容应视为过渡实现，不建议在其上继续堆主功能。

### 1. `packages/cli/src/tui.ts`

原因：

- 自持 transcript
- 自持 session 恢复
- 直接调 engine

处理：

- 只维持可运行
- 不继续加审批、复杂 slash command、session tree 等功能

### 2. `packages/cli/src/config.ts`

原因：

- 命名不准
- 职责位置错误

处理：

- 后续迁到 host-runtime/session

### 3. `packages/host-runtime/src/host.ts`

原因：

- 语义过薄

处理：

- 扩成真实 host facade

## 推荐迁移顺序

模块重组不要一次性乱搬，建议按下面顺序。

## Step 1: 新建包，不先删旧代码

先新增：

```text
packages/host-tui/
```

并让 workspace 能编译。

## Step 2: 把纯 TUI 资源搬走

先搬：

- `packages/cli/src/theme.ts` -> `packages/host-tui/src/theme.ts`

这是最低风险步骤。

## Step 3: 把 `tui.ts` 搬到 `host-tui`

先迁目录，不立即彻底重写。

目标：

- 让 `cli` 不再拥有 TUI 代码

## Step 4: 把 session 文件存储迁到 `host-runtime`

搬：

- `packages/cli/src/config.ts`

到：

- `packages/host-runtime/src/session/file-session-store.ts`

## Step 5: 扩充 `host-runtime`

新增：

- `session-state.ts`
- `host-state.ts`
- `host-actions.ts`
- `command-dispatcher.ts`

先建立正确 ownership，再补行为。

## Step 6: 改 `host-tui` 消费 host-runtime

这是关键切换点：

- TUI 不再直接碰 transcript
- TUI 不再直接调用 `engine.executeStep()`

## Step 7: 瘦身 `cli`

最后把 `cli` 收缩成真正入口层。

## 最小重组版和完整重组版

为了避免第一轮改动过大，可以定义两个级别。

## 方案 A：最小重组版

只做这些：

- 新增 `host-tui`
- `tui.ts` 和 `theme.ts` 从 `cli` 挪到 `host-tui`
- `config.ts` 从 `cli` 挪到 `host-runtime`
- `cli.ts` 改成只做 bootstrap

优点：

- 快
- 风险低

缺点：

- host-runtime 仍然不够完整

## 方案 B：完整重组版

在最小版基础上，再做：

- 重写 host-runtime 为主状态中心
- TUI 改成 host view
- slash commands 收到 host-runtime
- session state 升级

优点：

- 结构真正正确

缺点：

- 改动明显更大

建议：

- 先做 A
- 紧接着做 B
- 不要停留在 A 太久

## 最终判断

`piko` 的模块组织应该改，且改动重点非常明确：

- `engine-*` 基本保留
- `host-runtime` 做厚
- `cli` 做薄
- TUI 从 `cli` 拆到 `host-tui`

一句话概括：

**不是 engine 包拆错了，而是 host/TUI/CLI 三层现在混得太近。**

## 建议的下一步

如果继续推进，下一步最合适的是直接出一份更细的迁移清单：

- `pi` 里哪些文件应复制到 `host-runtime`
- `pi` 里哪些文件应复制到 `host-tui`
- 当前 `piko` 每个文件是“保留 / 迁移 / 废弃 / 重写”

建议文件名：

- `docs/piko-file-migration-map.md`
