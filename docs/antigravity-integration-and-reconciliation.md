# piko: Antigravity 集成与 TUI 状态调和（Reconciliation）机制分析报告

本记录详细回顾了 `antigravity` Provider 的接入过程、遇到的故障现象、根本原因分析以及最终的修复方案。

---

## 1. Antigravity Provider 的集成与初步故障

### 1.1 初始现象
在 `piko` 中选中 `antigravity` 模型并发送消息后，界面无任何返回，呈现“死锁”或“静默失败”的状态。

### 1.2 故障排查与 Debug Trace 接入
通过在 Provider 中引入 `startDebugSpan` 并开启 `PIKO_DEBUG=1`，我们捕捉到了关键的 Trace 日志：
- **401 Unauthorized**: 日志显示 `loadCodeAssist` 和 `streamGenerateContent` 均返回了 401 错误，表明 OAuth 令牌已过期。
- **UI 响应挂起**: 原有逻辑中，Provider 必须收到第一个有效 Token 才会推送 `start` 事件。由于 401 报错发生在请求初期，UI 还没来得及创建消息占位符流就结束了，导致用户感知为“无响应”。

### 1.3 修复措施：预热与自动刷新
- **同步 Warmup 逻辑**: 参考 `noagy` 实现，补全了 `fetchAvailableModels` 预热调用。
- **自动 Token 刷新**: 在 `PikoHost` 层接入了 `refreshAuth` 机制，在每次请求前检查令牌有效期，并自动触发 OAuth 刷新。
- **即时反馈**: 调整流式输出，一旦进入请求阶段立即推送 `start` 事件，确保 UI 能够即时显示“Thinking”状态。

---

## 2. TUI 状态调和（Reconciliation）失效问题

### 2.1 现象描述
修复 401 问题后，消息能够正常流式显示，但在回复结束的一瞬间（即 Reconcile 触发时），Assistant 消息会从界面上消失。

### 2.2 根本原因分析
`piko` 使用 `finalizeProjection` 函数来同步“实时流”状态与引擎返回的“权威历史（Canonical Transcript）”。

#### 顺序匹配偏移 (Positional Offset)
该算法极其依赖 UI 的消息顺序与权威历史的消息顺序**完全一致**。
- **问题**: `piko` 的 Compaction（压缩）和 Branching（分支）功能会生成 `role: "system"` 的摘要消息。
- **缺陷**: `finalizeProjection` 之前的实现只识别 `user`, `assistant`, `toolResult` 角色。当会话历史中存在系统摘要时，它们会被同步逻辑跳过，导致同步指针发生**下标偏移**。
- **后果**: Assistant 消息被错误地匹配到了它之前的系统消息（或者匹配失败），导致渲染内容被覆盖或由于状态未标记为“完成”而被过滤器剔除。

### 2.3 修复方案
补全了 `finalizeProjection` 对所有角色的支持：
- 显式处理 `role === "system"` 和 `role === "custom"`。
- 根据 `customType` 将其映射为正确的渲染类型（`compaction-summary` 或 `branch-summary`）。
- 确保同步队列与权威历史严格对齐。

---

## 3. Message 类型的重载与扩展

### 3.1 角色重载 (Overloading)
`piko` 在设计上为了复用 `Session` 存储，将非对话类的 Entry（如摘要）强行转换为了 `Message` 对象：
- **`role: "system"`**: 代表非对话的系统结构化消息。
- **`customType`**: 携带具体的子类型信息（如 `compactionSummary`）。

### 3.2 类型绕过 (Type Casting)
在同步逻辑中，为了读取这些 `piko` 特有的扩展字段，使用了 `cmsg as any` 的手段。
- **原因**: 基础库 `@earendil-works/pi-ai` 中的 `Message` 类型并没有定义这些扩展字段，直接读取会导致 TypeScript 编译失败。
- **现状**: 虽然解决了功能性问题，但揭示了 Orchestrator 与 TUI 之间协议定义（Protocol Consistency）的薄弱点。

---

## 4. 架构偏好与后续改进方向

基于本次故障复盘，确定了以下核心改进原则，旨在消除不必要的复杂性并增强系统的健壮性：

### 4.1 回归 Pi 原生逻辑 (Eliminate Unnecessary Deviations)
`piko` 当前对 Compaction 和 Branching 的实现被认为是不必要的偏离。
- **原则**: 停止“造轮子”，完全对齐 `pi` 的原生逻辑。
- **目标**: 将摘要信息作为独立的 `Entry` 类型处理，而不是强行重载为 `Message`，从而保持消息流的纯净。

**落地状态（2026-06-21）**：已完成。持久化层使用 `PersistableMessage`
排除 `branchSummary` / `compactionSummary`，摘要只能通过独立的
`branch_summary` / `compaction` Entry 写入。`buildSessionContext()` 中的摘要
Message 仅是 LLM 上下文适配结果，不再是合法的 Message Entry 表示。

### 4.2 体系化的 Reconcile 系统设计 (Robust Reconciliation Design)
当前的同步算法过于依赖顺序，且缺乏类型安全保障。
- **有机结合**: 将 Reconcile 系统与 Session 存储、Message 类型定义深度集成。
- **类型驱动**: 利用 TypeScript 的穷举检查（Exhaustive Check），确保每当增加新的 `Message` 或 `Entry` 类型时，编译器会强制要求在 Reconcile 逻辑中增加对应处理，从根本上杜绝“漏处理导致的消失”问题。
- **特征对齐**: 同步机制应从“位置驱动”转向“标识驱动”，确保复杂操作下的 UI 稳定性。

**落地状态（2026-06-21）**：已完成。实时投影按 runtime message ID、
`toolEntityId` 和 `toolCallId` 更新；完成阶段在 Host 持久化并 flush 后读取带
Entry ID 的当前 branch 快照，TUI 原子切换到 durable identity。无快照的兼容
路径只允许按 `toolCallId` 合并工具结果，不再依据 Message 数组位置改写消息。
`entriesToTranscript()` 对 `SessionTreeEntry` 使用穷举检查，新增 Entry 类型遗漏
映射会在编译期失败。

### 4.3 优雅的 OAuth 刷新机制 (Elegant OAuth Lifecycle)
目前“每次请求前尝试刷新”的方案仅为临时补丁，不够优雅。
- **原则**: 参考 `pi` 的 Token 管理逻辑。
- **目标**: 实现一套按需刷新、自动预警的鉴权生命周期管理机制，减少不必要的网络开销，同时保证请求的连续性。

### 4.4 标准化 Debug 机制与错误回显 (Debug & Error Feedback Design)
系统需要一套从内核到 TUI 的透明错误反馈路径。
- **结构化 Trace**: 建立统一的 Trace 埋点规范，确保所有 Provider 和内核环节都具备同等的可观测性。
- **透明回显**: 重新设计错误透传路径。当 API 报错时，不应仅仅显示“Provider Error”，而应将后端返回的原始错误（Raw Error）、请求上下文（Request Context）以及可能的排查建议（Troubleshooting Tips）有机地整合在 UI 消息中，让用户具备自排查能力。
- **实时诊断**: 完善 `/doctor` 等即时诊断工具，使其能够实时拉取并解析最新的 Trace 片段，在 UI 上直观展示“哪里卡住了”。
