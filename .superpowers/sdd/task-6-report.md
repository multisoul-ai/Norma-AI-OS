# Task 6 Report

## 1. RED evidence

先按 TDD 新增 `app_realtime_acceptance_tests.rs`，第一次运行：

```text
cargo test -p app app_realtime_acceptance_tests -- --nocapture
error[E0599]: no method named `drain_realtime_card_tasks_with_runner_for_tests`
```

结论：

- app 层当时缺少一个把 fake runner 接进 realtime card-task drain + callback flush 链路的测试入口。
- 这正好说明 acceptance test 命中了“tool_call -> task -> create/send -> callback queue/inject”的完整 app-level 链，而不是纯 domain 窄测试。

## 2. GREEN evidence

补充最小测试 hook 后，重新运行：

```text
cargo test -p app app_realtime_acceptance_tests -- --nocapture
running 1 test
test app::realtime::app_realtime_acceptance_tests::test_realtime_acceptance_tool_call_executes_create_then_send_and_flushes_after_audio_end ... ok
```

GREEN 后新增覆盖确认：

- fake realtime `enqueue_card_task`
- fake planner `Create Codex + SendText(previous_step, submit=true)`
- fake runner 记录真实命令序列
- succeeded `ToolResult`
- assistant speaking 时先排队，audio end 后注入

## 3. Verification commands and results

- `cargo test -p app app_realtime_acceptance_tests -- --nocapture`
  - passed
- `cargo test -p card_task`
  - passed
- `cargo test -p realtime`
  - passed
- `cargo test -p voice`
  - passed, 1 ignored
- `cargo test -p voice_sherpa`
  - passed, hardware/model dependent tests ignored
- `cargo test -p app`
  - passed, 346 passed / 1 ignored
- `cargo check -p app`
  - passed
- `cd sidecars/openai-realtime && npm run build && npm test`
  - passed, 16 Node tests passed
- `bash scripts/check-storage-boundaries.sh`
  - passed
- `bash scripts/check-performance-guardrails.sh`
  - passed
- `bash scripts/check-max-file-lines.sh`
  - passed
- `bash scripts/check-no-allow.sh`
  - passed
- `cargo fmt --check`
  - passed
- `git diff --check`
  - passed

## 4. Modified files

- `crates/app/src/app/realtime.rs`
- `crates/app/src/app/realtime/card_tasks.rs`
- `crates/app/tests/unit/app_realtime_acceptance_tests.rs`
- `docs/design-docs/realtime-card-task-mvp/07-end-to-end-acceptance.md`
- `docs/design-docs/realtime-card-task-mvp/acceptance-report.md`
- `.superpowers/sdd/task-6-report.md`

## 5. Self-review

- acceptance test 走的是 app 路由入口，不是直接调用纯 `card_task` queue。
- 命令序列验证没有依赖真实画布副作用，而是用 fake runner 精确断言 `Create Agent(Codex)` 再 `SendText(node_id=77, submit=true)`。
- callback 排队/注入验证绑定到了 realtime session speaking 状态，覆盖了 Task 6 要求的 assistant speaking queue 场景。
- 只新增了一个测试模块挂载，`crates/app/src/app/realtime.rs` 仍在 500 行以内。

## 6. Concerns

- live OpenAI key + microphone/speaker 的真实语音链路仍需人工补跑。
- 该 acceptance harness 证明的是 deterministic app orchestration；自然语言口播质量和实际首响延迟仍属于手动验收范围。
