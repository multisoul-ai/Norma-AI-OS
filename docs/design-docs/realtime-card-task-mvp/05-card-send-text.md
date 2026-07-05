# 05 卡片输入与发送 SPEC

- 日期：2026-07-06
- 状态：草稿
- 上游：`04-card-task-queue.md`
- 下游：`06-task-result-callback.md`、`07-end-to-end-acceptance.md`

## 1. 目标

在卡片任务队列基础上支持向指定卡片或刚创建的卡片输入内容并发送。这个阶段让用户可以说：“创建一个 Codex 卡片，让它分析 README，然后发送。”

## 2. 范围

本阶段要做：

- 支持 `CardCommand::SendText`。
- 支持 previous-step target：新建卡片后，把后续文本发送到该卡片。
- 支持 focus previous step。
- Executor 记录 `created_by_step`，用于解析后续 step 的目标。
- Planner prompt / schema 明确允许 create -> send 的复合任务。
- `submit=true` 时执行发送动作，`submit=false` 时只输入文本。

本阶段不做：

- 不读取卡片内容。
- 不等待被发送卡片的内部任务完成。
- 不解析 terminal output 或 Codex 回复。
- 不做高风险确认。

## 3. 新增 step 语义

```rust
pub enum CardTaskStep {
    Command(norma::protocol::CardCommand),
    SendTextToPrevious {
        previous_step: usize,
        text: String,
        submit: bool,
    },
    FocusPrevious {
        previous_step: usize,
    },
}
```

执行上下文：

```rust
created_by_step: HashMap<usize, NodeId>
```

解析规则：

```text
step 0 = Create Agent(Codex)
  -> runner 返回 created node_id = 12
  -> created_by_step[0] = 12

step 1 = SendTextToPrevious { previous_step: 0, text, submit: true }
  -> 查 created_by_step[0]
  -> 转为 CardCommand::SendText { node_id: 12, text, submit: true }
```

如果 previous step 没有创建节点，当前 task failed。

## 4. 用户流程

```text
用户说：“创建一个 Codex 卡片，让它分析 README，然后发送。”
  -> enqueue_card_task(intent)
  -> Planner 生成两步计划：
     1. Create Agent(Codex)
     2. SendText target=PreviousStep(0), text="请分析 README", submit=true
  -> Executor 执行创建
  -> Executor 记录新节点 id
  -> Executor 向新节点输入文本并发送
  -> task succeeded
```

## 5. 验收点

自动验收：

- create -> send previous-step 能保留 created `node_id`。
- `SendTextToPrevious` 引用不存在 step 时，task failed。
- `SendTextToPrevious` 引用非 create step 时，task failed。
- `submit=true` 会传给 `CardCommand::SendText`。
- `submit=false` 不触发发送，只输入文本。
- send 失败时 task failed，后续 step 不继续执行。
- active task 在 create 后、send 前被取消时，不回滚已创建卡片，并停止 send。

手动验收：

- 对 AI 说：“创建一个 Codex 卡片，让它分析 README，然后发送。”
  预期：画布新增 Codex 卡片，卡片收到“请分析 README”并发送。
- 对 AI 说：“给当前卡片输入 hello，但不要发送。”
  预期：当前卡片出现输入文本，但不提交。
- 对 AI 说：“取消刚才那个操作。”
  预期：如果 create 已完成但 send 未执行，不删除已创建卡片，只停止后续发送。

## 6. 失败与边界

- 目标卡片不存在：task failed。
- 当前焦点不是可输入卡片：task failed。
- previous-step target 无法解析：task failed。
- `SendText` 只执行白名单命令，不执行模型生成代码。
- 不因为 `submit=true` 做额外确认；这是 MVP 明确接受的风险。
