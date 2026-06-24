// ---- Protocol: orch_runtime — Host-facing API trait ----
//
// OrchRuntime is the trait that every transport implements.
// It is the unified public API for Host code, regardless of whether
// orchd runs in-process or over RPC.

use std::future::Future;
use std::pin::Pin;

use super::agents::{AgentSpec, AgentTaskId};
use super::config::{OrchdConfig, OrchdError, TaskInput, TaskResult, UserResponse};
use super::events::OrchEvent;
use super::runtime::GraphSnapshot;
use super::state::OrchState;

/// orchd 对 Host 暴露的全部能力。
///
/// 使用 `Pin<Box<dyn Future>>` 返回类型以支持 trait object。
pub trait OrchRuntime: Send + Sync {
    // ── 生命周期 ──

    /// 初始化 orchd，传入 provider 凭证、agent 定义、工具集。
    /// 必须在所有其他方法之前调用一次。
    fn configure(
        &self,
        config: OrchdConfig,
    ) -> Pin<Box<dyn Future<Output = Result<(), OrchdError>> + Send + '_>>;

    /// 优雅关闭 orchd。
    fn shutdown(&self) -> Pin<Box<dyn Future<Output = Result<(), OrchdError>> + Send + '_>>;

    // ── Agent 管理 ──

    /// 运行时注册一个新 agent。
    fn register_agent(
        &self,
        spec: AgentSpec,
    ) -> Pin<Box<dyn Future<Output = Result<(), OrchdError>> + Send + '_>>;

    /// 取消注册一个 agent。
    fn unregister_agent(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), OrchdError>> + Send + '_>>;

    // ── 任务执行 ──

    /// 同步执行一个任务，返回 event stream + 最终 result。
    ///
    /// Host 调用此方法后，会收到一个事件接收器和最终结果的 handle。
    fn run(
        &self,
        input: TaskInput,
    ) -> Pin<Box<dyn Future<Output = Result<TaskResult, OrchdError>> + Send + '_>>;

    /// 异步 spawn 子任务，返回 task_id。不等待结果。
    fn spawn(
        &self,
        input: TaskInput,
    ) -> Pin<Box<dyn Future<Output = Result<AgentTaskId, OrchdError>> + Send + '_>>;

    /// 等待一个 detached 子任务完成并获取结果。
    fn join(
        &self,
        task_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<TaskResult, OrchdError>> + Send + '_>>;

    /// 取消一个运行中的任务。
    fn cancel(
        &self,
        task_id: &str,
        reason: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), OrchdError>> + Send + '_>>;

    // ── 用户交互 ──

    /// Host 回应用户交互请求（ask_user / request_approval）。
    fn respond_user(
        &self,
        task_id: &str,
        response: UserResponse,
    ) -> Pin<Box<dyn Future<Output = Result<(), OrchdError>> + Send + '_>>;

    // ── 事件订阅 ──

    /// 订阅 orchd 事件流。返回取消订阅的 handle。
    fn subscribe(
        &self,
        listener: Box<dyn Fn(OrchEvent) + Send + Sync>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn FnOnce() + Send>, OrchdError>> + Send + '_>>;

    // ── 状态查询 ──

    /// 获取当前 orchestrator 状态快照。
    fn snapshot(&self) -> Pin<Box<dyn Future<Output = Result<OrchState, OrchdError>> + Send + '_>>;

    /// 获取任务图的 graph 表示。
    fn graph(&self)
    -> Pin<Box<dyn Future<Output = Result<GraphSnapshot, OrchdError>> + Send + '_>>;
}
