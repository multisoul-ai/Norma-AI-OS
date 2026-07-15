# Task 5 Report

## RED Evidence

- `cargo test -p realtime --test unit -- callback`
  - 初次失败：`RealtimeToolResultStatus`、`RealtimeToolResult::task_result(...)`、`RealtimeSessionState::{queue_or_inject_tool_result,pending_tool_result_count,handle_transcript_final,handle_assistant_audio_end,with_callback_queue_limit}` 均不存在。
  - 证明新增 `crates/realtime/tests/unit/callback_tests.rs` 确实打在 Task 5 缺失的 callback queue API 上。
- `cargo test -p app app_realtime_callback_tests -- --nocapture`
  - 初次失败：`RealtimeToolResultStatus` 与 `enqueue_task_for_tests` 缺失。
  - 第一轮实现后继续失败：`test_realtime_card_task_user_speaking_queues_until_transcript_final` 断言 flush 后 ToolResult 数量应为 1，实际为 0。
  - 证明 app 侧 user-speaking -> transcript-final flush 路径在实现前确实不完整。

## GREEN Evidence

- `cargo test -p realtime --test unit -- callback`
  - 4 个 callback 专项测试全部通过：
    - idle 立即 inject
    - user speaking 排队，transcript final flush
    - assistant speaking 排队，audio end flush
    - queue limit 丢弃最旧项
- `cargo test -p realtime`
  - `realtime` 全量通过：14 个 lib 单元测试 + 10 个 sidecar/integration 测试。
- `cd sidecars/openai-realtime && npm run build && npm test`
  - build 成功，16 个 sidecar 测试全部通过；既有 `tool_result -> function_call_output -> response.create` 语义保持不变。
- `cargo test -p app app_realtime_card_task_tests -- --nocapture`
  - 5 个既有 card-task app 测试全部通过。
- `cargo test -p app app_realtime_callback_tests -- --nocapture`
  - 4 个新增 callback app 测试全部通过：
    - idle 终态立即注入
    - user speaking 排队直到 transcript final
    - assistant speaking 排队直到 audio end
    - realtime unavailable / tool_call_id missing 时跳过且不 panic
- `cargo check -p app`
  - 通过，无新增 warning。

## Verification Commands And Results

1. `cargo test -p realtime --test unit -- callback`
   - PASS
2. `cargo test -p realtime`
   - PASS
3. `cd sidecars/openai-realtime && npm run build && npm test`
   - PASS
4. `cargo test -p app app_realtime_card_task_tests -- --nocapture`
   - PASS
5. `cargo test -p app app_realtime_callback_tests -- --nocapture`
   - PASS
6. `cargo check -p app`
   - PASS
7. `bash scripts/check-max-file-lines.sh`
   - PASS
8. `bash scripts/check-no-allow.sh`
   - PASS
9. `git diff --check`
   - PASS

## Modified Files

- `crates/realtime/src/lib.rs`
- `crates/realtime/src/protocol.rs`
- `crates/realtime/src/session.rs`
- `crates/realtime/tests/unit/protocol_tests.rs`
- `crates/realtime/tests/unit/sidecar_tests.rs`
- `crates/realtime/tests/unit/callback_tests.rs`
- `crates/app/src/app/realtime.rs`
- `crates/app/src/app/realtime/chat.rs`
- `crates/app/src/app/realtime/shell.rs`
- `crates/app/src/app/realtime/card_tasks.rs`
- `crates/app/src/app/realtime/callbacks.rs`
- `crates/app/tests/unit/app_realtime_callback_tests.rs`

## Self-Review

- 保留并复用了 Task 3 已有的 sidecar `tool_result -> conversation.item.create(function_call_output) -> response.create` 协议，没有重复发明。
- `RealtimeToolResult` 现在有统一的 `status/task_id/summary` helper；invalid tool-call 的既有 failed path 保持兼容。
- callback queue gate 严格受 `user_speaking == false && assistant_speaking == false` 控制。
- app 仅在 card task 终态时构造 `RealtimeToolResult`，并在 realtime 不可用或 `tool_call_id` 缺失时记录日志后跳过，不会重启 stopped session。
- 为满足 500 行限制，把 callback bridge 拆到了 `crates/app/src/app/realtime/callbacks.rs`；`realtime.rs` 和 `card_tasks.rs` 均保持在 500 行以内。
- 全程未向日志、tool result summary、sidecar 测试输出中引入 OpenAI API key。

## Concerns

- 当前实现的 flush 触发点是 `TranscriptFinal` 和 `AssistantAudioState { speaking: false }`。如果未来出现“task 在 session 仍处于 Connecting 时就完成”的极端路径，结果会先入队，现阶段没有专门在 `SessionStarted` 上做一次补 flush；当前 app 流程下未观察到该路径。

---

## Task 5 Fix Implementer Follow-up

### Fix Summary

- 在 `crates/app/src/app/realtime/callbacks.rs` 增加 app-layer availability guard：只有 `self.realtime.is_active() && self.realtime.has_sidecar()` 成立时，才允许把 card-task callback 交给 `RealtimeSessionState::queue_or_inject_tool_result(...)`。
- 当 session_id 仍存在、但 realtime 已 error 或 sidecar 已不可用时，bridge 现在只记录 drop 日志并直接返回，不再把 callback 滞留进 realtime pending queue。
- 保持 `crates/realtime` 的 callback queue 语义不变；此次修复只收紧 app callback bridge 的入口条件。
- 新增回归测试 `ARC-CB-5`：覆盖 sidecar error 后 session_id 仍在、task 完成时既不发送 `ToolResult`，也不在 pending queue 留下 callback。
- 增加 test-only getter `pending_tool_result_count_for_tests()`，仅用于验证 app bridge 未把 callback 误排入 queue。

### Verification Commands And Results

1. `cargo test -p app app_realtime_callback_tests -- --nocapture`
   - PASS（5 个 callback app 测试通过，含新增 ARC-CB-5）
2. `cargo test -p realtime --test unit -- callback`
   - PASS
3. `cargo check -p app`
   - PASS
4. `bash scripts/check-max-file-lines.sh`
   - PASS
5. `bash scripts/check-no-allow.sh`
   - PASS
6. `git diff --check`
   - PASS
