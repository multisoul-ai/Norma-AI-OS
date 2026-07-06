# Task 1 Report: `card_task` Domain Queue And Executor Core

## 实现内容

- 新建纯 Rust `crates/card_task` crate，并接入 workspace。
- 定义核心模型：
  - `CardTaskRequest { intent, tool_call_id }`
  - `CardTask`
  - `CardTaskId`
  - `CardTaskStep`
  - `CardTaskState`
  - `CardTaskEvent`
- 实现 `compile_card_task_steps()`，把 `voice::CompiledVoicePlan` 转成 `CardTaskStep`。
- 实现 `CardCommandRunner` trait、`CardTaskExecutionContext`、`execute_card_task_step()`：
  - 记录 `Created { node_id } -> created_by_step[step_index]`
  - 解析 `SendTextToPrevious` / `FocusPrevious`
  - 区分 `MissingPreviousStep` 与 `PreviousStepDidNotCreateNode`
  - 保留 `submit=true/false`
- 实现 `CardTaskQueue`：
  - `enqueue()` 生成 queued task 并同步缓存 `Queued` 事件
  - `run_next()` 串行推进 `Planning -> Planned -> Executing -> StepSucceeded -> Succeeded/Failed`
  - `cancel_latest()` 先取消最新 queued task；若无 queued，再取消 active task
  - active task 取消不回滚已完成 step
- 暴露测试需要的只读辅助：
  - `drain_events()`
  - `pending_len()`
  - `pending_tasks()`
  - `active_task()`
  - `completed_tasks()`

## TDD RED / GREEN 证据

### RED

先创建最小 crate 骨架与测试文件，再执行：

```bash
cargo test -p card_task --test unit -- queue
```

结果：`exit code 101`

关键信号：

- `unresolved imports card_task::execute_card_task_step`
- `unresolved imports card_task::CardTaskQueue`
- `unresolved imports card_task::CardTaskStep`

说明测试先于实现存在，失败原因是目标 API 尚未实现，符合 RED。

### GREEN

实现最小代码后重新执行：

```bash
cargo test -p card_task --test unit -- queue
```

结果：`7 passed; 0 failed`

随后执行完整 crate 验证：

```bash
cargo test -p card_task
```

结果：`11 passed; 0 failed`

## 测试命令和结果

1. `cargo test -p card_task --test unit -- queue`
   - RED: `exit 101`，缺少公共类型/函数
   - GREEN: `7 passed; 0 failed`

2. `cargo test -p card_task`
   - `11 passed; 0 failed`

3. `cargo tree -p card_task | rg -i "gpui|app|runtime|canvas|terminal|storage"; test $? -eq 1`
   - 原样执行后失败
   - 原因：误命中第三方依赖名 `is_terminal_polyfill`

4. 补充精确检查：

```bash
cargo tree -p card_task --prefix none | rg -i '^(gpui|app|runtime|canvas|terminal|storage) v'; test $? -eq 1
```

   - `exit 0`
   - 说明 `card_task` 没有依赖仓库内的 `gpui/app/runtime/canvas/terminal/storage` crate

5. `bash scripts/check-max-file-lines.sh`
   - `exit 0`

6. `bash scripts/check-no-allow.sh`
   - `exit 0`

7. `cargo fmt`
   - `exit 0`

## 修改文件

- `Cargo.toml`
- `Cargo.lock`
- `crates/card_task/Cargo.toml`
- `crates/card_task/src/lib.rs`
- `crates/card_task/src/model.rs`
- `crates/card_task/src/queue.rs`
- `crates/card_task/src/executor.rs`
- `crates/card_task/tests/unit.rs`
- `crates/card_task/tests/unit/model_tests.rs`
- `crates/card_task/tests/unit/queue_tests.rs`
- `crates/card_task/tests/unit/executor_tests.rs`

## 自审发现

- `cancel_latest()` 的优先级按 spec 实现：先 queued，后 active。为此修正了一条初始测试假设，避免把 queued 优先级写反。
- `run_next()` 采用“每次调用最多执行一个 step（首次调用包含 planning + 第一个 step）”的同步推进方式，这样才能在纯 Rust 同步 API 下表达 step 边界取消。
- 所有新 Rust 文件都低于 500 行：
  - `queue.rs = 219`
  - `model.rs = 110`
  - `executor.rs = 96`
  - `queue_tests.rs = 392`

## 风险 / 疑问

- 当前 `CardTaskPlanner` trait 只接收 `CardTaskRequest`。后续 app 集成如果需要把 `VoicePlanningRequest` 的 card/focus/selection 上下文交给 planner，需要在 adapter 层补齐，不建议让本 crate 直接依赖 app/runtime。
- 当前 `summary` 直接复用 `intent`。这满足 Task 1 的结果摘要承载要求，但更自然的“已创建/已聚焦/已停止后续操作”文案应在后续 callback/result formatting 任务中完善。
- brief 中的依赖边界 grep 命令会被 `is_terminal_polyfill` 这类第三方包名误伤；本次已补充更精确的检查命令。

## Reviewer Findings 修复追加

### 修复内容

- 补充 runner error 回归测试，证明执行器错误会连续发出 `StepFailed` 与终态 `Failed`。
- 收紧 active cancellation 语义：
  - `cancel_latest()` 对 active task 只设置 `cancel_requested = true`
  - 下一次 `run_next()` 才在 step 边界把 task 终止为 `Cancelled`
- 增强 active cancellation 测试，明确断言 active task 在取消请求后仍留在 `active`，且 `cancel_requested == true`。
- 新增 `List` / `Delete` 白名单执行覆盖，证明这两个 `CardCommand` 变体能经由 fake runner 正常透传执行。
- 复核 `crates/card_task/Cargo.toml` 的 diff hygiene；`git diff --check` 已为干净状态，没有残留 EOF/trailing whitespace 问题。

### 本轮 TDD 证据

先补测试后执行：

```bash
cargo test -p card_task
```

RED 结果：`13 tests` 中 `test_cancel_active_task_stops_before_next_step_without_rollback` 失败，错误为：

```text
left: None
right: Some(true)
```

说明旧实现会在 `cancel_latest()` 时立即移除 active task，未满足“先置 `cancel_requested`，再在下一次推进时停止”的 reviewer 期望。

修正 `queue.rs` 后重新执行同一命令：

```bash
cargo test -p card_task
```

GREEN 结果：`13 passed; 0 failed`

### 验证命令与结果

1. `cargo test -p card_task`
   - `13 passed; 0 failed`
2. `bash scripts/check-max-file-lines.sh`
   - `exit 0`
3. `bash scripts/check-no-allow.sh`
   - `exit 0`
4. `git diff --check`
   - `exit 0`

### 本轮变更文件

- `crates/card_task/src/queue.rs`
- `crates/card_task/tests/unit/queue_tests.rs`
- `crates/card_task/tests/unit/executor_tests.rs`
- `.superpowers/sdd/task-1-report.md`
