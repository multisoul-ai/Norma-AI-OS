# 04 卡片任务队列 SPEC

- 日期：2026-07-06
- 状态：草稿
- 上游：`02-realtime-sidecar-ipc.md`
- 下游：`05-card-send-text.md`、`06-task-result-callback.md`

## 1. 目标

让 Realtime tool call 能异步派发卡片操作任务，并由 Rust 串行执行基础卡片命令。这个阶段先支持 `Create`、`List`、`Focus`、`Delete`，不做 `SendText`。

## 2. 范围

本阶段要做：

- 新增 `crates/card_task`。
- 定义 `CardTaskRequest`、`CardTask`、`CardTaskStep`、`CardTaskState`、`CardTaskEvent`。
- Realtime tool call `enqueue_card_task(intent)` 可以入队。
- CardTaskQueue 串行执行，任意时刻最多一个 active task。
- OpenAI HTTP Card Planner 返回 `VoicePlan` compatible JSON。
- 复用 `voice::compile_voice_plan`。
- Executor 通过 app 提供的 runner 执行 `CardCommand::{Create,List,Focus,Delete}`。
- 支持 queued task 取消和 active task step 边界取消。

本阶段不做：

- 不支持 `SendText`。
- 不做任务面板。
- 不做卡片 badge。
- 不并发执行多个任务。
- 不做高风险确认。

## 3. 核心模型

```rust
pub struct CardTaskRequest {
    pub intent: String,
}

pub struct CardTask {
    pub task_id: CardTaskId,
    pub request: CardTaskRequest,
    pub summary: String,
    pub steps: Vec<CardTaskStep>,
    pub state: CardTaskState,
    pub cancel_requested: bool,
}

pub enum CardTaskState {
    Queued,
    Planning,
    Executing { step_index: usize },
    Succeeded,
    Failed { message: String },
    Cancelled { message: String },
}
```

任务事件：

```rust
pub enum CardTaskEvent {
    Queued { task_id: CardTaskId, summary: String },
    Planning { task_id: CardTaskId },
    Planned { task_id: CardTaskId, step_count: usize },
    Executing { task_id: CardTaskId, step_index: usize },
    StepSucceeded { task_id: CardTaskId, step_index: usize },
    StepFailed { task_id: CardTaskId, step_index: usize, message: String },
    Succeeded { task_id: CardTaskId, summary: String },
    Failed { task_id: CardTaskId, message: String },
    Cancelled { task_id: CardTaskId, message: String },
}
```

## 4. 流程

```text
OpenAI Realtime tool_call enqueue_card_task(intent)
  -> sidecar 发 JSONL tool_call
  -> crates/realtime 解析为 RealtimeToolCall
  -> app 把 intent 交给 crates/card_task
  -> CardTaskQueue 创建 queued task
  -> 如果没有 active task，进入 Planning
  -> OpenAI HTTP Planner 返回 VoicePlan JSON
  -> voice::compile_voice_plan 校验
  -> CardTaskExecutor 串行执行 steps
  -> emit Succeeded / Failed / Cancelled
```

## 5. Planner 输入输出

Planner 输入复用：

```rust
pub struct VoicePlanningRequest {
    pub transcript: String,
    pub cards: Vec<CardRef>,
    pub focused_node_id: Option<u64>,
    pub selected_node_ids: Vec<u64>,
    pub max_steps: usize,
}
```

MVP 规则：

- `transcript` 来自 `CardTaskRequest.intent`。
- `cards` 来自当前画布卡片列表。
- `max_steps` 由 app/card_task 配置。
- Planner 输出必须能被 `voice::compile_voice_plan` 接受。
- 如果 compiled plan 标记 `requires_confirmation`，MVP 仍直接执行。

## 6. 取消语义

```text
如果 latest queued task 存在：
  从队列移除
  emit Cancelled

否则如果 active task 存在：
  set cancel_requested = true
  当前 step 执行完
  下一个 step 开始前停止
  emit Cancelled

否则：
  返回 "no active card task"
```

MVP 不回滚。已经创建或删除的卡片不会因为取消自动恢复。

## 7. 验收点

自动验收：

- `enqueue_card_task(intent)` 创建 queued task。
- 串行队列同一时间只允许一个 active task。
- 第二个 task 在第一个 active task 完成前保持 queued。
- Planner provider error 会产生 `Failed`。
- malformed planner JSON 会产生 `Failed`。
- queued task 取消会从队列移除并 emit `Cancelled`。
- active task 取消会在 step 边界停止，不回滚已完成 step。
- `Create/List/Focus/Delete` 的 compiled command 能通过 fake runner 执行。

手动验收：

- 对 AI 说：“创建一个 Codex 卡片。”
  预期：画布新增 Codex 卡片，聊天主线不中断。
- 对 AI 说：“聚焦刚才的卡片。”
  预期：该卡片获得焦点。
- 对 AI 说：“列出当前卡片。”
  预期：任务成功，结果可以记录日志或准备回灌。
- 对 AI 说：“取消刚才那个操作。”
  预期：queued task 直接取消，active task 在 step 边界取消。

## 8. 失败与边界

- `enqueue_card_task` 缺少 intent：返回 failed tool result。
- tool call 名称未知：返回 failed tool result。
- CardCommand 目标不存在：task failed。
- Planner HTTP 超时：task failed。
- task 失败时 realtime 已不可用：只写 tracing log，不写 SQLite，不弹 modal。
