# Realtime Card Task MVP 验收报告

- 日期：2026-07-07
- 对应任务：Task 6 - End-To-End Acceptance Harness And Documentation
- 对应 SPEC：[`07-end-to-end-acceptance.md`](./07-end-to-end-acceptance.md)

## 1. 自动测试覆盖

本次新增自动验收测试：

- `app::realtime::app_realtime_acceptance_tests::test_realtime_acceptance_tool_call_executes_create_then_send_and_flushes_after_audio_end`
- `e2e::realtime_card_task_pipeline::test_realtime_tool_call_create_send_and_callback_pipeline`

app-level headless 测试确定性覆盖了 07 核心句子的最小闭环：

- `enqueue_card_task(intent="创建一个 Codex 卡片，让它分析 README，然后发送")` 能从 realtime `tool_call` 路由到 app card-task queue。
- fake planner 返回两步 plan：`Create Codex` + `SendText(previous_step=0, submit=true)`。
- fake runner 确认实际执行顺序精确为：
  - `Create Agent(Codex)`
  - `SendText { node_id: created_id(77), text: "请分析 README", submit: true }`
- task 成功终态会桥接成 realtime `ToolResult`，且 `status="succeeded"`。
- assistant speaking 时成功结果先进入 callback queue；assistant audio end 后才注入 sidecar。

新增 `crates/e2e` 测试覆盖了该闭环中可通过公开 crate API 验证的
domain-level pieces：

- `realtime::decode_event` 解码 JSONL `tool_call`。
- `card_task::CardTaskQueue` 执行 create -> send previous-step。
- `realtime::RealtimeSessionState` 在 assistant speaking 时排队 callback，并在 audio end 后 flush。
- `realtime::encode_command` 输出 `tool_result` JSONL。

说明：`crates/e2e` 不依赖 `crates/app` 私有测试入口，因此不验证 app 内部
`tool_call_id` 映射和 callback bridge；这部分由上面的 app-level headless
acceptance test 覆盖。

与本次验收链共同形成覆盖的现有自动测试：

- `app_realtime_tool_call_tests`
  - 覆盖 realtime `tool_call` 路由、非法参数失败回报、cancel 路由。
- `app_realtime_card_task_tests`
  - 覆盖 planning request 快照、host 串行执行、planner 失败、app runner effects。
- `app_realtime_callback_tests`
  - 覆盖 idle 立即注入、user speaking 排队、assistant speaking 排队、sidecar unavailable 丢弃。
- `card_task` / `realtime` / `sidecars/openai-realtime` 测试
  - 覆盖 previous-step send 解析、callback queue 规则、tool call/tool result 协议映射。

按 07 checklist 对齐：

- 已自动覆盖：
  - 3.3 `enqueue_card_task(intent)` 能创建 task
  - 3.3 task 串行执行
  - 3.3 支持 `SendText`
  - 3.3 支持 create -> send previous-step
  - 3.4 成功结果能回灌 Realtime
  - 3.4 用户或 assistant 正在说话时，结果进入 callback queue
  - 3.5 Card Planner 输出必须经过 Rust `compile_voice_plan`
  - 3.5 Executor 只能执行白名单 `CardCommand`

## 2. 仍需人工验证

以下项依赖 live OpenAI key 和真实麦克风/扬声器，本次未运行：

- 3.1 Settings -> Voice 的完整开关与配置缺失提示交互
- 3.1 app close / project close 对 session、PCM stream、sidecar 的真实生命周期收口
- 3.2 用户语音进 OpenAI Realtime
- 3.2 AI 语音回复与 `1-2 秒` 首响目标
- 3.2 用户打断 assistant 回复
- 3.2 打断 assistant 回复时 active card task 不被取消
- 3.4 AI 用自然语言口播结果
- 第 5 节手动脚本 A-E 的真实设备/联网路径

说明：

- 本次没有使用 live OpenAI API key。
- 本次没有做 microphone / speaker 手动验收。
- 本次 `crates/e2e` 是不依赖 app 私有测试入口的 domain-level e2e；
  `crates/app` 仍通过 headless `TestAppContext` 覆盖 app-level 链路。
  这两者都不是启动真实 packaged `.app` 的 L3 验收。
- packaged `.app` 内 card planner helper 的资源拷贝已通过 `Packager.toml`
  配置和 `scripts/test-packager-config.sh` 做静态验证；真实 packaged app
  live planner 仍需发版包内手动验证。
- 这些项不阻塞 Task 6 自动化完成，由主控在最终验收阶段补跑。

## 3. 本次实际运行命令与结果

已运行并通过：

- `cargo test -p app app_realtime_acceptance_tests -- --nocapture`
  - 1 个新增 app-level acceptance test 通过。
- `cargo test -p e2e`
  - 1 个新增 domain-level e2e test 通过。
- `cargo test -p card_task`
  - 20 passed。
- `cargo test -p realtime`
  - 24 passed（unit + sidecar tests）。
- `cargo test -p voice`
  - 28 passed，1 ignored（需要本地 Ollama）。
- `cargo test -p voice_sherpa`
  - 29 lib tests 中 28 passed / 1 ignored，另有 `tts_demo` 1 passed / 1 ignored（需要真实模型/硬件）。
- `cargo test -p app`
  - 354 passed，1 ignored。
- `cargo check -p app`
  - passed。
- `cd sidecars/openai-realtime && npm run build && npm test`
  - build 成功，Node test 16 passed。
- `node --check sidecars/openai-card-planner/openai-card-planner.mjs`
  - OpenAI card planner helper 语法校验通过。
- `cargo tree -p card_task | rg -i '(^|[[:space:]])(gpui|app|runtime|canvas|terminal|storage) v'; test $? -eq 1`
  - `card_task` 未依赖 GPUI/app/runtime/canvas/terminal/storage。
- `bash scripts/check-storage-boundaries.sh`
  - passed。
- `bash scripts/check-performance-guardrails.sh`
  - passed。
- `bash scripts/check-max-file-lines.sh`
  - passed。
- `bash scripts/check-no-allow.sh`
  - passed。
- `cargo fmt --check`
  - passed。
- `git diff --check`
  - passed。

## 4. 未运行项

本次没有单独重跑以下“可选但建议”的最终验证命令之外的 live/manual 项：

- live OpenAI key 联机语音会话
- 手动脚本 A-E

原因：

- 当前任务范围是 deterministic acceptance harness + 文档，不应因外部密钥或音频硬件阻塞。
