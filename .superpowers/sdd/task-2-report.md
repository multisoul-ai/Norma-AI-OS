# Task 2 Report: Card Planner Adapter And App Runner Integration

## 实际实现

本次只完成 Task 2，未实现 Task 3 sidecar function-call mapping，也未实现 Task 5 callback queue。

已落地内容：

- `crates/card_task/src/planner.rs`
  - 新增 `CardTaskPlanJsonProvider`
  - 新增 `parse_voice_plan_json(...)`
  - 新增 `compile_voice_plan_for_card_task(...)`
  - 新增 `compile_voice_plan_json_for_card_task(...)`
  - 新增 `plan_with_provider(...)`
  - 统一复用 `CardTaskPlannerError::{Provider, Json, Compile, TranscriptOnly}`
- `crates/card_task/src/lib.rs`
  - 导出 planner helper API
- `crates/card_task/tests/unit/planner_tests.rs`
  - 覆盖 JSON parse、malformed JSON、execute compile、transcript-only rejection
- `crates/app/src/app/realtime/card_tasks.rs`
  - 新增 `RealtimeCardTaskState`
  - 新增 planner host：`RealtimeCardTaskPlannerMode` + `StoredRequestPlanner`
  - 新增 `AppCardTaskRunner`，通过 `execute_card_command_for_area(...)` 执行命令
  - 新增 `build_realtime_card_task_planning_request(...)`
  - 新增 `MultisoulApp::enqueue_realtime_card_task(...)`
  - 新增 `MultisoulApp::cancel_realtime_card_task(...)`
  - 新增 `MultisoulApp::drain_realtime_card_tasks(...)`
- `crates/app/src/app/realtime.rs`
  - 挂载 `card_tasks` 模块与测试模块
- `crates/app/src/app/realtime/shell.rs`
  - realtime sidecar drain loop 里增加 card-task drain tick
  - 通过 `cx.update(|app| app.windows())` 获取窗口，再在窗口上下文里调用 `drain_realtime_card_tasks(...)`
- `crates/app/tests/unit/app_realtime_card_task_tests.rs`
  - 覆盖 app entrypoint、planning request snapshot、serial execution、planner failure、app runner effect 聚合
- `crates/app/Cargo.toml`
  - 增加 `card_task` 依赖
  - 给 `gpui` 打开 `test-support` feature，支撑 app-level realtime card-task test

## TDD RED / GREEN

### 我亲自复现到的 RED

1. `cargo test -p card_task planner_tests -- --nocapture`
   - 失败点：`test_parse_voice_plan_json_preserves_valid_plan_fields`
   - 真实报错：planner JSON 里 `profile:"codex"` 大小写不合法，serde 期望 `Codex`
   - 结论：现有半成品测试本身错误，导致 planner helper 绿不了

2. `cargo test -p app app_realtime_card_task_tests -- --nocapture`
   - 我补了 app entrypoint test 后，第一次编译失败
   - 真实失败有两类：
     - `gpui::test` / `TestAppContext` 不可用，因为 `gpui` 真实依赖未开启 `test-support`
     - `realtime/shell.rs` 的 loop 闭包里拿到的是 `AsyncApp`，不能直接 `windows()`
   - 结论：Task 2 的 app integration 之前还没有真正跑过 app-level entrypoint 路径

### 只能基于半成品状态诚实推断的 RED

- brief 里要求的 planner helpers、app queue host、app runner、entrypoints 文件骨架已经被前一个子代理起了头，所以我没法再看到“模块/文件完全缺失”的最原始 RED。
- 但从我接手时的 dirty tree 可确认：代码是“部分实现 + 错误报告”，而不是已完成状态；尤其 `.superpowers/sdd/task-2-report.md` 完全是旧 Task 01-03 内容，必须整份重写。

### GREEN

在修正测试、接线 queue host、补 app entrypoint 路径后，以下命令 fresh 通过：

- `cargo test -p card_task`
  - `17 passed; 0 failed`
- `cargo test -p app app_realtime_card_task_tests -- --nocapture`
  - `5 passed; 0 failed`
  - 不是 0 tests 伪通过

## 最终验证命令与结果

```bash
cargo test -p card_task
```

- Exit 0
- `17 passed; 0 failed`

```bash
cargo test -p app app_realtime_card_task_tests -- --nocapture
```

- Exit 0
- `5 passed; 0 failed; 333 filtered out`

```bash
cargo check -p app
```

- Exit 0
- 有 dead_code warning；无 error

```bash
bash scripts/check-storage-boundaries.sh
```

- Exit 0

```bash
bash scripts/check-performance-guardrails.sh
```

- Exit 0

```bash
cargo fmt --check
```

- Exit 0

```bash
git diff --check
```

- Exit 0

## 修改文件

- `.superpowers/sdd/task-2-report.md`
- `Cargo.lock`
- `crates/app/Cargo.toml`
- `crates/app/src/app/realtime.rs`
- `crates/app/src/app/realtime/shell.rs`
- `crates/app/src/app/realtime/card_tasks.rs`
- `crates/app/tests/unit/app_realtime_card_task_tests.rs`
- `crates/card_task/Cargo.toml`
- `crates/card_task/src/lib.rs`
- `crates/card_task/src/planner.rs`
- `crates/card_task/src/queue.rs`
- `crates/card_task/tests/unit.rs`
- `crates/card_task/tests/unit/planner_tests.rs`

## 自审

- 我没有改 Task 1 core executor/model 设计，只在 Task 2 需要的边界上补 planner helper、app runner、queue host。
- 我没有实现 sidecar tool-call mapping；`enqueue_realtime_card_task(...)` / `cancel_realtime_card_task(...)` 目前由 app test 直接走入口，生产侧等 Task 3 再接 realtime `ToolCall`。
- `cargo check -p app` 仍有 dead_code warning，这是因为 Task 2 先把入口和 host 落地，但 Task 3 尚未把 sidecar tool call 正式连到这些入口；这是已知残留，不影响本次验收。
- 现有报告已完全替换，旧的 01-03 Always Listening 内容已删除。
