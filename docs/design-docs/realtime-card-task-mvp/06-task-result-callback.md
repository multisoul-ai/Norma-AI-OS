# 06 任务结果自然回报 SPEC

- 日期：2026-07-06
- 状态：草稿
- 上游：`03-realtime-voice-chat.md`、`04-card-task-queue.md`、`05-card-send-text.md`
- 下游：`07-end-to-end-acceptance.md`

## 1. 目标

把卡片任务结果回灌到 OpenAI Realtime session，让 AI 在当前对话中自然汇报成功、失败或取消结果。这个阶段的关键是：任务完成不打断用户说话，不覆盖 assistant 当前播报。

## 2. 范围

本阶段要做：

- `crates/realtime` 维护 callback queue。
- `crates/realtime` 维护 `user_speaking`、`assistant_speaking`、`active_response_id`。
- CardTask completed event 转成 `RealtimeToolResult`。
- sidecar 能把 tool result 写回 OpenAI Realtime session。
- Realtime 根据 tool result 自然生成后续回复。
- 支持 success / failed / cancelled 三类结果。

本阶段不做：

- 不做任务活动面板。
- 不做卡片 badge。
- 不做持久化任务历史。
- 不做复杂插话策略。

## 3. 回调时机

```text
如果 user_speaking == false 且 assistant_speaking == false：
  立即注入 task result

否则：
  先进入 callback queue
  等当前 user turn final 或 assistant audio end 后再注入
```

这样避免：

- 用户正在说话时被任务结果打断。
- assistant 正在播报时被另一个结果覆盖。
- 多个 task result 同时注入导致对话混乱。

## 4. Tool Result 协议

Rust -> Sidecar：

```json
{"type":"tool_result","session_id":"rt_1","tool_call_id":"call_123","result":{"status":"succeeded","task_id":"task_7","summary":"已创建 Codex 卡片并发送请求。"}}
```

失败：

```json
{"type":"tool_result","session_id":"rt_1","tool_call_id":"call_124","result":{"status":"failed","task_id":"task_8","summary":"没有找到目标卡片。"}}
```

取消：

```json
{"type":"tool_result","session_id":"rt_1","tool_call_id":"call_125","result":{"status":"cancelled","task_id":"task_9","summary":"已停止后续操作。Codex 卡片已创建，但没有发送内容。"}}
```

## 5. 用户体验

成功示例：

```text
用户：“创建一个 Codex 卡片，让它分析 README，然后发送。”
AI：“我来处理。”
后台任务执行中，用户可以继续说话。
任务完成后，AI：“我已经创建了 Codex 卡片，并把 README 分析请求发过去了。”
```

失败示例：

```text
用户：“把内容发送到那个浏览器卡片。”
任务失败：目标卡片不存在或不可输入。
AI：“我没找到可以输入的目标卡片，所以没有发送内容。”
```

取消示例：

```text
用户：“取消刚才那个操作。”
AI：“我已经停止后续操作。已经完成的部分不会自动回滚。”
```

## 6. 验收点

自动验收：

- user/assistant idle 时 callback 立即注入。
- user speaking 时 callback 入队。
- assistant speaking 时 callback 入队。
- user turn final 后，pending callback 被注入。
- assistant audio end 后，pending callback 被注入。
- success / failed / cancelled 都能生成 tool result JSONL。
- realtime 不可用时，task result 只写 tracing log，不 panic。

手动验收：

- 卡片任务成功后，AI 在当前语音对话里自然汇报结果。
- 用户说话期间任务完成，AI 不抢话；等用户说完后再汇报。
- assistant 正在回复时任务完成，当前回复不被硬切断；结果排队汇报。
- 任务失败后，AI 用自然语言说明失败原因。
- 任务取消后，AI 说明已完成部分不会回滚。

## 7. 失败与边界

- tool call id 丢失：无法注入 Realtime tool result，记录日志并标记 failed callback。
- realtime session 已停止：不重启 session，不弹 modal，只写日志。
- callback queue 过长：MVP 可限制最近 N 条，并丢弃最旧 callback，日志记录丢弃原因。
- 多个结果连续完成：按完成顺序串行注入。
