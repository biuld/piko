# piko 架构重置方案

## 结论

我同意当前判断：`piko` 现在主要是一个“协议壳 + 演示壳”，还不是可持续演进的产品架构。

它的问题不只是“功能没做完”，而是**分层切错了**：

- engine / host 的边界虽然在文档里定义了，但实现并没有真正落地
- host 没有承接 `pi` 现有的 session / transcript / TUI / slash command 主路径能力
- TUI 现在只是一个独立小程序，不是可恢复、可扩展、可持久化的 host shell
- session 现在只是本地 JSONL 存档，不是 runtime 的一等状态对象

因此，下一步不应该继续把现有 `piko` 壳层一点点补齐，而应该把目标切换成：

**以 `pi` 现有架构为基底，做一次“runtime ownership 迁移”，把 agent runtime 从 host 中抽走，保留 host 产品层。**

换句话说：

- `pi` 现有产品壳能复用的，尽量直接复用
- 不适合直接复用但结构成熟的，复制代码后做定点改造
- 只有 runtime loop / tool execution / approval continuation 这一层，才作为新的 stateless engine 重写

## 根本目标

我们的本质目标不是“做一个新的 CLI 工具”，而是：

1. 保留 `pi` 已经成熟的产品层能力
2. 把原来由 host 持有的 agent runtime 语义迁到 stateless engine
3. 形成新的固定边界，便于以后接远端 engine、sandbox backend、Codex/Rust engine

所以最终目标应当是：

- `piko` 不是一个从零写的小型替代品
- `piko` 是 `pi` 架构的一次下游重组
- 重组的重点是 runtime 分层，不是 UI 重写

## 对当前 piko 的判断

当前仓库更适合作为“phase 0/1 原型”，不适合作为后续主线直接增量演进。

### 已有价值

- engine protocol 的基本方向是对的
- native engine / remote engine / host-runtime 的包拆分方向基本对
- 文档已经把核心边界说清楚了

### 主要缺陷

#### 1. host 还不是产品 host

现在的 host 只做了一个最薄的 scheduler。

缺的不是几段代码，而是整套 host 职责：

- session 生命周期
- transcript 持久化模型
- approval UI 与恢复
- slash commands
- session tree / fork / compact
- model/config/runtime state 的统一管理
- TUI 事件到 runtime state 的映射

#### 2. TUI 不是 host 的 UI 外壳，而是旁路实现

当前 TUI 直接自己维护：

- `messages`
- `transcript`
- `/clear`
- `/resume`
- streaming 展示

这会导致：

- runtime 状态不统一
- UI 行为和 session 状态脱节
- host-runtime 被绕开
- 后续 approval / tool events / remote engine 很难接进去

#### 3. session 只是落盘格式，不是 runtime 核心对象

当前 session 更像简单日志文件：

- 没有明确 session state schema
- 没有 pending approval / engine checkpoint
- 没有 view state / metadata / resume contract
- 没有和 host scheduler 联动

#### 4. 代码复用策略还不够激进

当前实现虽然用了 `@earendil-works/pi-ai` 和 `@earendil-works/pi-tui`，但只复用了最外层包接口，没有复用 `pi` 已经验证过的产品层组织方式。

结果就是：

- 底层协议是新的
- 上层产品层又重新写了一遍简化版
- 中间成熟能力没有承接

这是成本最高、收益最低的路径。

## 新的总策略

建议把后续实现策略改成：

## 1. 先复用产品层，再重写 runtime 层

优先级固定如下：

1. **直接 npm 引入上游包**
2. **复制上游成熟代码到下游，做定点改造**
3. **仅对 runtime/engine 相关层做新实现**

不要反过来做。

如果产品层已经在 `pi` 里成熟存在，就不应该先在 `piko` 里手写一个简化版，再慢慢补齐。

## 2. 把“copy with modification”当成正式策略

如果某段 `pi` 代码满足以下条件，就应该优先复制而不是抽象重写：

- 产品行为已经稳定
- 逻辑强依赖当前 `pi` 内部结构，短期不适合抽成 npm 包
- 改动范围主要是 runtime 接口替换，而不是功能重写

这类代码复制后改造，通常比重新发明一个简化版更可靠。

要明确承认一点：

**对下游重构项目来说，复制成熟代码并定向修改，往往比“为了优雅而从零抽象”更便宜。**

## 3. 只在真正需要稳定共享时才抽 npm 包

是否抽成上游 npm 包，应以“是否稳定、是否通用、是否短期内还会大改”为准。

适合直接 npm 依赖：

- `@earendil-works/pi-ai`
- `@earendil-works/pi-tui`
- 纯工具型、纯类型型、纯渲染型稳定模块

不适合现在强行抽包的：

- 带明显 runtime ownership 的 host/session/TUI 协调层
- 强依赖当前产品流程的 interactive shell
- approval / resume / session-tree 这种尚在重构中的逻辑

这些更适合先 copy 到 `piko`，边迁移边稳定，再反推哪些值得回抽到上游。

## 目标架构

建议把 `piko` 的最终结构调整为：

```text
packages/
  engine-protocol/        纯协议与共享类型
  engine-native/          stateless engine 本地实现
  engine-remote/          远端 engine client
  host-runtime/           host 核心状态、session、scheduler、approval
  host-tui/               基于 pi-tui 的产品 UI 壳
  cli/                    进程入口、参数解析、模式切换
```

其中：

- `host-runtime` 是产品层核心，不是 demo scheduler
- `host-tui` 只消费 host-runtime 暴露的 state/actions/events
- `cli` 不再直接操作 transcript 或 engine stream

## 分层原则

### Engine 层负责

- provider 调用
- tool loop
- tool 执行
- approval pause / resume
- transcript 生成
- engine checkpoint

### Host Runtime 层负责

- session state
- run state
- pending approval state
- session store
- engine 调度
- slash command dispatch
- view model

### TUI 层负责

- 渲染 host state
- 接收用户输入
- 派发 host actions
- 展示 engine events / approval prompts / status

### CLI 层负责

- 启动模式选择
- 配置加载
- model/provider 选择
- 连接 host-runtime 和 TUI / non-interactive runner

## 复用策略清单

下面按“直接依赖 / 复制改造 / 新实现”分类。

## A. 直接通过 npm 引入，尽量不改

### 1. `@earendil-works/pi-ai`

用途：

- model registry
- provider stream
- faux provider / 测试能力
- message/model 基础能力

要求：

- 尽量只通过公开 API 使用
- 不在 `piko` 里复制 model/provider 数据
- bridge 层只做必要的协议映射

### 2. `@earendil-works/pi-tui`

用途：

- terminal abstraction
- editor
- markdown renderer
- 布局和基础组件

要求：

- 只把它当 UI toolkit
- 不在 TUI 层私自承接 session/runtime 状态

## B. 优先 copy 代码，再做定点改造

这部分建议直接从 `pi` 复制到 `piko`，作为下游拥有代码。

### 1. interactive shell / TUI orchestration

原因：

- 这是产品行为，不只是组件调用
- 很可能强依赖 `pi` 当前交互流程
- 但其核心问题不是功能错，而是 runtime 接口需要替换

改造目标：

- 把旧 agent runtime 接口替换成 host-runtime action/state
- 保留已有交互结构、键位、布局、slash command 体验

### 2. session 管理与 transcript 持久化

原因：

- 这是 host 产品层核心能力
- 和 engine 分层关系密切
- 短期内需求还会变化，不适合现在强抽上游公共包

改造目标：

- 从“消息文件”升级为“session state 存储”
- 持久化：
  - transcript
  - pending approval
  - engineState
  - model/provider snapshot
  - UI/session metadata

### 3. slash commands / command routing

原因：

- 强产品语义
- 会和 host state 深度耦合
- 很多命令本质上是 host action，不该散落在 TUI 里

改造目标：

- `slash command -> host action`
- command handler 不直接碰 engine stream

### 4. session tree / fork / compact 相关主路径

原因：

- 这是 `pi` 的产品差异化能力
- 如果将来 `piko` 也要承接，就不该最后再补

改造目标：

- 先把数据结构和 host contract 留出来
- 可以分阶段启用 UI

## C. 必须新实现的部分

### 1. stateless engine protocol

这个已经明确是新边界，不应该复用旧 agent hook 语义。

### 2. native engine state machine

这里应当参考 `pi` 现有 agent loop 的行为，但不能保留 ownership。

原则：

- 可以参考旧实现
- 可以复制局部逻辑
- 但最终必须重组为 step-based state machine

### 3. remote engine client

这是新的协议能力，直接新实现。

## 推荐实施路径

建议不要在当前 `piko` 上继续小步补功能，而是做一次“架构纠偏”。

## Phase 0: 冻结重置方向

输出新的冻结结论：

- `piko` 不是从零重写产品壳
- `piko` 以复用 `pi` 产品层为优先
- host-runtime 是主产品核心
- TUI 只能消费 host state，不能自持 transcript

交付物：

- 本文档
- 修订后的 rollout 文档

## Phase 1: 盘点 `pi` 中要复用的代码

按模块做 inventory，不写代码，先出清单。

分类：

- 可以直接 npm 依赖的
- 需要 copy 的
- 不再要的
- 必须新写的

最少要盘点：

- interactive TUI shell
- session store
- transcript manager
- slash command 系统
- config/model loading
- approval UI
- session tree / fork / compact

交付物：

- 一份 migration inventory
- 每个模块的处理方式：`npm` / `copy` / `rewrite`

## Phase 2: 先做 host-runtime，不先补 TUI

先把真正的 host runtime 做完整：

- session state schema
- scheduler
- pending approval state
- engine checkpoint persistence
- host actions
- command dispatch

退出标准：

- host-runtime 不依赖临时 transcript 数组
- host 可以恢复一个中断中的 run
- host 可以持久化 pending approval + engineState

## Phase 3: 用 copy 的 `pi` shell 接 host-runtime

这一步不要继续维持当前手写 TUI。

做法：

- 从 `pi` 复制 interactive shell 相关代码
- 替换掉旧 runtime 调用接口
- 接到新的 host-runtime actions/state

目标：

- TUI 不再自己维护 transcript
- `/clear`、`/resume`、`/sessions` 都通过 host-runtime
- streaming / approval / tool events 都走统一 host event model

## Phase 4: 再修 engine 的真实能力缺口

在 UI 和 host 都接回正轨后，再继续补 engine：

- approval resume 真正执行
- engineState 跨 step 传递
- remote event filtering
- error semantics
- tool continuation

这样顺序更合理，因为：

- engine 的边界是否正确，只有放进真实 host 后才看得清
- 先修 demo TUI，收益不高

## Phase 5: 决定哪些下游改动值得回抽上游

当 `piko` 跑稳以后，再看哪些模块适合回抽成上游包：

- 通用 session storage schema
- host action model
- approval primitives
- TUI shell shared widgets

不要在现在就为未来抽象付成本。

## Session 设计建议

当前 session 模型太轻。建议把 session 提升成 host-runtime 的核心对象。

最少应包含：

```typescript
interface SessionState {
  sessionId: string;
  createdAt: string;
  updatedAt: string;
  systemPrompt: string;
  transcript: Message[];
  runState: "idle" | "running" | "awaiting_approval" | "error";
  pendingApproval?: PendingApprovalState;
  engineState?: unknown;
  modelSnapshot: {
    provider: string;
    modelId: string;
    baseUrl?: string;
  };
  uiState?: {
    draft?: string;
    selectedMessageId?: string;
  };
}
```

关键点：

- `pendingApproval` 必须进 session
- `engineState` 必须进 session
- `runState` 必须持久化
- UI 只读写 session，不额外维护一套旁路 transcript

## TUI 设计建议

TUI 必须从“自带逻辑的小程序”改成“host view”。

### 当前不该继续保留的方式

- TUI 自己维护 transcript
- TUI 自己写 `/resume`
- TUI 自己做 streaming 聚合
- TUI 自己决定 session 存盘时机

### 应改成

- `host.getState()`
- `host.dispatch(action)`
- `host.subscribe(listener)`

TUI 只做：

- 输入框
- 消息列表
- 状态栏
- approval prompt
- session list / picker

这样 approval、resume、abort、engine reconnect 才不会散掉。

## 对现有仓库的建议处理方式

不建议推倒重来，也不建议在现有代码上继续直接堆功能。

建议：

1. 保留现有 `engine-protocol` / `engine-native` / `engine-remote` 目录
2. 把当前 `cli` 和简化 TUI 视为过渡实现
3. 新开一轮 host-centric 重构
4. 逐步废弃当前 TUI 内部自持 transcript 的路径

也就是说：

- 协议层成果保留
- demo shell 不作为最终架构延续

## Definition of Done

当下面几点成立时，才算这次架构重置真正完成：

1. `piko` 的交互主路径复用了 `pi` 的成熟产品层，而不是重新发明一个简化版
2. host-runtime 成为 session / approval / scheduler / command 的唯一所有者
3. TUI 不再直接维护 transcript 和 run loop
4. engine 负责完整 runtime 语义，host 不手工拼 tool result
5. session 可以恢复：
   - 正常历史会话
   - pending approval 会话
   - 带 engineState 的中断会话
6. 上游复用策略清晰：
   - 稳定能力走 npm
   - 产品层复杂逻辑允许 copy
   - 只有新 runtime 层强制重写

## 建议的下一步

最合适的下一步不是立刻改代码，而是先做一份 `pi -> piko` 迁移清单。

建议马上做的事：

1. 盘点 `pi` 里现有 host/session/TUI/slash-command 相关代码
2. 对每个模块标注：
   - `npm`
   - `copy`
   - `rewrite`
3. 基于该清单，重写 rollout 文档
4. 然后再开始 host-runtime 重构

如果要继续推进，我建议下一步直接产出第二份文档：

- `docs/piko-migration-inventory.md`

内容就是逐模块列出应从 `pi` 复用什么、复制什么、重写什么。
