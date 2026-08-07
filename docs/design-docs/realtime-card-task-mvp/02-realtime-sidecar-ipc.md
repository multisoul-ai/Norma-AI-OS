# 02 Realtime Sidecar IPC SPEC

- 日期：2026-07-06
- 状态：草稿
- 上游：`01-always-listening-shell.md`
- 下游：`03-realtime-voice-chat.md`、`04-card-task-queue.md`

## 1. 目标

建立 Rust app 与 Node/TypeScript sidecar 之间的最小 JSONL IPC。这个阶段的成功标准是：Rust 能启动 sidecar，发送 `start_session` / `stop_session`，接收 `session_started` / `session_error`，并把 malformed JSON 和 sidecar 异常退出转换为明确的 realtime error。

## 2. 范围

本阶段要做：

- 新增或扩展 `crates/realtime`，定义 Norma 内部 command/event 类型。
- Rust 通过 stdio JSONL 启动和管理 sidecar 子进程。
- Node/TypeScript sidecar 提供最小入口，能读取 stdin JSONL 并写出 stdout JSONL。
- 定义并测试 JSONL parser。
- 支持 `start_session`、`stop_session`、`session_started`、`session_error`。
- unknown event type 记录日志并忽略。
- malformed JSON line 触发 session error 并停止 session。

本阶段不做：

- 不接入真实 OpenAI Realtime。
- 不传输音频。
- 不处理 tool call。
- 不执行卡片任务。

## 3. IPC 协议

Rust -> Sidecar：

```json
{"type":"start_session","session_id":"rt_1","config":{"model":"gpt-realtime","tools":["enqueue_card_task","cancel_card_task"]}}
{"type":"stop_session","session_id":"rt_1"}
```

Sidecar -> Rust：

```json
{"type":"session_started","session_id":"rt_1"}
{"type":"session_error","session_id":"rt_1","message":"OpenAI realtime disconnected"}
{"type":"session_stopped","session_id":"rt_1","reason":"user stopped"}
```

所有 JSONL 消息必须满足：

- 一行一个 JSON object。
- 必须包含 `type`。
- 必须包含 `session_id`，除非是 sidecar process 级 fatal error。
- 不允许日志里输出 OpenAI API Key。

## 4. 技术边界

`crates/realtime` 负责：

- `RealtimeCommand`
- `RealtimeEvent`
- JSONL encode/decode
- sidecar process lifecycle
- session state

Node/TypeScript sidecar 负责：

- 读取 stdin。
- 写 stdout。
- 把内部错误输出为 Norma JSONL event。

Sidecar 明确不负责：

- 卡片命令规划。
- 修改画布。
- 写 SQLite。
- 保存长期状态。

## 5. 生命周期

```text
用户打开 Always Listening
  -> Rust 创建 realtime session id
  -> Rust spawn sidecar
  -> Rust 发送 start_session
  -> sidecar 返回 session_started
  -> UI 进入 Listening

用户关闭 Always Listening
  -> Rust 发送 stop_session
  -> 等待 sidecar 正常退出
  -> 超时则 kill
  -> UI 回到 Voice

sidecar 异常退出
  -> Rust 记录 exit status
  -> session 进入 Error
  -> UI 显示 Realtime error
```

## 6. 验收点

自动验收：

- JSONL parser 能解析 `session_started`。
- JSONL parser 遇到未知 `type` 不 panic。
- JSONL parser 遇到 malformed JSON 会返回 session error。
- `start_session` command encode 后包含 `type`、`session_id`、`model` 和 tools。
- sidecar process command 日志不包含 API Key。
- stop path 能处理 sidecar 已经退出的情况。

手动验收：

- 使用 fake sidecar 启动 Always Listening，底部语音条进入 `Always Listening`。
- fake sidecar 输出 `session_error`，底部语音条进入 `Realtime error`。
- fake sidecar 输出坏 JSON，app 不崩溃，并进入 error 状态。
- 关闭 Always Listening 后 sidecar 进程消失。

## 7. 失败与边界

- sidecar path 不存在：不启动 session，显示 realtime error。
- sidecar 无执行权限：不启动 session，记录 path 和 OS error。
- sidecar 无输出或启动超时：进入 error。
- malformed JSON：关闭当前 session，不尝试继续读取。
- stdout/stderr 日志截断保存摘要，避免泄露敏感信息。
