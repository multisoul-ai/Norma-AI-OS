# 幕僚对话流回合化重构

- 日期：2026-11-15
- 状态：待实施
- 决策记录：[ADR-0030](../adr/0030-staff-reply-single-render-path.md)
- 方案对照页：<https://claude.ai/artifact/Wmr4Y9PAehkqSCznsPdQag>

## 1. 背景

幕僚面板把三套东西平铺在扁平的 `Task.messages` 里：持久化消息、只在内存的 `TaskActivity` 流式进度、Worker 系统事件。导致：

- 一轮回复被 `insert_segment` 切成多条 `Norma`，每条都画署名；流式与落盘两套布局，落盘瞬间重排。
- 工具步骤、耗时、计划不落盘，切任务或重启后消失。
- 活动视图用 `baseline` 下标 + `owns()` 认领本轮消息，brain 中途插入会挪动下标。
- 回答问题后 `answer_text` 又作为 `User` 入库，画成重复气泡。
- 「已创建 Worker」「任务还没送达」是满宽卡片，与对话同权重，已解决后按钮仍在。
- 工具行暴露 `"/Users/…/.norma/brain/bin/norma" brain agents --wor…`。
- 幕僚既能流式写文字，又能 `task reply` 写回；用过 `reply` 时流式文字不落盘，看到的话事后消失。

## 2. 目标

1. 每个幕僚轮次对应一条**幕僚回复**（`TaskMessage::Staff`）：一个署名 → 过程区 → 正文 → 内嵌卡片 → 操作。
2. 单一渲染路径：App 只渲染 `Task.messages`，删除 `TaskActivity` 的 timeline / baseline / owns / reconcile。
3. **过程摘要**（步骤标题、状态、耗时、计划）落盘，历史与实时一致。
4. 复制、重新生成、停止生成、贴底自动滚动。

## 3. 非目标

- 不兼容旧数据：v1 快照备份后清空。
- 不存工具入参与输出。
- 不做多版本切换（ChatGPT 的 1/2）。
- iOS 不改（不解码幕僚消息）。
- 不改主大脑 / 灵动岛的对话呈现。

## 4. 数据模型（`crates/norma/src/protocol/`）

```rust
// staff.rs
enum TaskMessage {
    User { text: String },
    /// 幕僚回复：每轮一条，流式时就地更新。
    Staff(StaffReply),
    /// 用户回答了问题；正文在 Task.questions。
    Answered { question_id: String },
    /// Worker 提问的镜像，不属于任何幕僚轮次。
    WorkerQuestion { question_id: String },
    WorkerStarted { proposal_id: u64, dispatch_id: String },
    WorkerNeedsAttention { node_id: u64, dispatch_id: String },
    WorkerFinished { node_id: u64, dispatch_id: String, outcome: WorkerOutcome, text: String },
    Reopened,
}
struct Task { /* … */ questions: Vec<StaffQuestion>, /* … */ }

// staff_reply.rs（新）
struct StaffReply {
    round: u64,
    trigger: ReplyTrigger,          // User | Event：只有 User 可重新生成
    parts: Vec<StaffPart>,
    status: ReplyStatus,            // Running | Done | Stopped | Failed
    elapsed_ms: u64,
}
enum StaffPart {
    Text(String),
    Step { id: String, title: StepTitle, status: StepStatus, elapsed_ms: Option<u64> },
    Plan(Vec<PlanItem>),
    Proposal(u64),
    Question(String),               // 幕僚或 ACP 原生提问
}
enum StepTitle { Norma(Vec<String>) /* 子命令词 */, Tool(String) }
enum StepStatus { Running, Completed, Failed, Denied, Interrupted }
```

- 删除顶层 `Norma`、`Proposal`、`Question` 变体。
- `StepTitle` 只存结构化信息，展示边界翻译；不存绝对路径与参数。
- `StaffProposal` 新增 `superseded: bool`。`supersede_stale_proposals` 改为标记，不再删消息。
- `pending_question_count` / `expire_pending_questions` 改读 `Task.questions`。
- 快照 `version` 1 → 2。读到 v1：`tasks.json` 改名 `tasks.v1.bak.json`，从空库开始，记一条日志，不报错。

## 5. 技术方案

### 5.1 TaskStore（`crates/brain/src/staff.rs`）

- `update_live(workspace, id, f)`：改内存快照并广播本轮增量（`StaffReplyChanged { task_id, index, reply }`），**不写盘、不加 revision**，节流约 80ms。
- `update` 照旧写盘加 revision。落盘时机：步骤结束、卡片入库、回合结束 / 停止 / 失败、任何非流式写入。
- 加载时把 `Running` 的 `Staff` 改为 `Failed`、运行中步骤改为 `Interrupted`（崩溃恢复）。
- 所有写入经同一把锁，只追加不插入。

### 5.2 brain

- 轮次开始：追加 `Staff { status: Running, trigger }`，落盘。
- `lane/staff_stream.rs`（新）：`TraceItem` → 回复变更。`Reply` 追加到最后一个 `Text`（前一个 part 不是 Text 时新开）；`ToolCall/Result/Permission` upsert `Step`；`Plan` 覆盖 `Plan`；`Prompt` 清空本轮 Text。
- `task propose` / `assign` / 幕僚提问：数据入 `proposals` / `questions`，在进行中的 `Staff` 追加引用。
- **取消 `task reply`**：宿主对幕僚 token 拒绝 `StaffOp::Reply`，norma-brain skill 删除该命令，提示词保留“直接用自然语言写回复”。
- 回答问题：写 `questions[i].answers`，追加 `Answered`，不再 `user_message`。
- `lane/staff_prompt.rs`（新）：给模型的任务快照投影。`Staff` 只保留 Text 与卡片摘要，去掉 Step / Plan；`Answered` 展开为“问题 + 答案”文本。
- 删除 `Segments`、`insert_segment`、`save_reply` 的分段逻辑。
- 停止生成：ACP 轮次中途取消（`session/cancel`），独立提交。取消后已生成内容落盘，`status: Stopped`。

### 5.3 Worker 事件

照常 push 到 `messages`。渲染时 `Running` 的 `Staff` 始终排在最底部，运行期间到达的事件显示在它上方。

### 5.4 App

- `registry.rs` 分发 `User` / `Staff` / 事件条 / Worker 结果 / Worker 提问 / `Answered`（不渲染，答案在问题卡上显示）。
- `TaskActivity` 只保留排队计数。
- 运行中最后一段正文用纯文本，完成后切 Markdown（避免每 80ms 重解析整段）。
- `brain_host/staff/{decisions,worker_questions}.rs` 改读 `Task.questions`。

## 6. 代码结构

★ 新增　✎ 修改　✗ 删除

```text
crates/norma/src/protocol/
  staff.rs             ✎ TaskMessage、Task.questions、superseded
  staff_reply.rs       ★ StaffReply/StaffPart/StepTitle 及进行中回复的变更方法
  staff_question.rs    ✎ 改读 Task.questions
crates/brain/src/
  staff.rs             ✎ update_live、v1 备份、崩溃恢复
  lane/staff.rs        ✎ 轮次开始建 Staff；删 Segments/insert_segment/save_reply
  lane/staff_stream.rs ★ TraceItem → 回复变更
  lane/staff_prompt.rs ★ 模型快照投影
  staff/workflow.rs    ✎ 拒绝 Reply
  staff/assign.rs      ✎ 卡片引用挂到进行中的 Staff
  staff/questions.rs   ✎ Task.questions + Answered
  acp/client.rs        ✎ 轮次中途取消
  skills/norma-brain   ✎ 删除 task reply
crates/app/src/app/brain_host/staff/
  events.rs            ✎ 接收本轮增量
  decisions.rs, worker_questions.rs ✎ 改读 Task.questions
crates/app/src/app/staff/
  activity.rs          ✎ 只剩排队计数
  scroll.rs            ★ 贴底跟随、“回到最新”
  composer.rs          ✎ 运行中发送 ↔ 停止
  messages/
    registry.rs        ✎
    reply/             ★ mod.rs（署名+拼装）process.rs（过程区）body.rs（正文）actions.rs（复制/重新生成）step_title.rs
    event.rs           ★ 事件条，替代 notice.rs 与 card/needs_attention.rs
    card/proposal.rs   ✎ “已被替换”状态
    card/question.rs   ✎ 按 id 取 Task.questions，原地显示答案
    card/worker_result.rs ✎ Worker 署名折叠块
    text/user.rs       不变
    text/{prose,notice,activity,activity_tools}.rs ✗
    card/needs_attention.rs ✗
crates/ui/locales/staff.{zh-CN,en}.json ✎
test/desktop-e2e/Scenarios/staff/ ★ 看图场景；Fixtures/staff 重写
```

单文件 ≤ 500 行；`crates/norma/src/protocol/staff.rs` 已 411 行，新类型放 `staff_reply.rs`。

## 7. 界面

参考 skill：`apple-design`（层级、直接反馈、可访问性）、`emil-design-eng`（折叠区展开收起）、`review-animations`（动效审查）。

| 元素 | 形态 |
| --- | --- |
| 用户消息 | 右侧气泡，保持现状 |
| 幕僚回复 | 左对齐无底色；署名一次 → 过程区 → 正文 → 卡片 → 悬停操作 |
| 过程区 | 运行中全部展开（计划在顶部，每步状态与计时）；完成后折叠为「已处理 N 步 · 用时 X 秒」，可点开；无步骤时不显示 |
| 提议卡 | 嵌在回合里；被替换后变灰、去掉采纳按钮、标「已被新方案替换」 |
| 问题卡 | 嵌在回合里；回答后原地显示「你的回答：…」 |
| Worker 提问 | 以 Worker 署名的问题卡，位于回合之间 |
| 事件条 | 一行药丸 + 状态色点 + 文字按钮；已解决（按 Worker 当前状态判断）变灰、去掉按钮 |
| Worker 结果 | Worker 头像署名的折叠块，默认 3 行摘要 |
| 步骤标题 | 已知 `norma` 子命令查 i18n 表（`brain agents` →「查看可用 Agent」）；其余去路径与参数，只显示工具名 / 子命令名 |
| 操作 | 复制（本回合 Text 拼接的 Markdown）；重新生成仅在最后一条 `trigger: User` 的 `Staff` 上 |
| 停止 | 运行中发送按钮变停止；回复标「已停止」 |
| 滚动 | 在底部时跟随；用户上翻后不抢滚动，显示「回到最新」 |

所有文案中英双语；折叠/展开动效在减少动态效果下改为瞬切；控件具备 hover / pressed / focus / disabled 状态。

## 8. 边界情况

- 运行中崩溃 / 退出：下次加载 `Running` → `Failed`，步骤 → `Interrupted`。
- 排队中再发消息：保持现有排队语义，每轮一条 `Staff`。
- 一轮只有卡片没有文字：正常显示署名与卡片。
- 停止后立即重新生成：等前一轮落盘完成再开始。
- 重新生成：用该轮对应的用户输入追加新轮次，不删旧回合；已派发的 Worker 不回滚。
- 回合 Running 时用户回答问题 / 采纳提议 / Worker 事件写入：经同一把锁，只追加。
- 被替换提议所在的旧回合不会变空。
- 升级清库：画布上仍在运行、由旧任务派发的 Worker 完成事件将判定为无主；发版说明写明。

## 9. 非功能

- 渲染、PTY drain、鼠标移动路径不读盘；流式广播只带本轮回复，节流约 80ms。
- `update_live` 不加 revision。
- 满足 `check-performance-guardrails.sh`、`check-max-file-lines.sh`、`check-no-allow.sh`、`check-i18n.py`、`test-i18n.py`。
- `cargo tree -p brain` 依赖边界不变。

## 10. 交付

一个 PR，worktree 分支 `feat/staff-turn-rendering-<date>`。按提交顺序评审：

1. 协议 + TaskStore（v2、清库、update_live、崩溃恢复）
2. brain 流式写回、卡片引用、取消 `task reply`、提示词投影
3. ACP 中途取消
4. App 回合渲染，删除双路径
5. 事件条、Worker 结果、问题去重
6. 复制 / 重新生成 / 停止 / 贴底滚动
7. desktop-e2e 场景与夹具

## 11. 验收

| 标准 | 证据 |
| --- | --- |
| 一轮只有一条 `Staff`，卡片按到达顺序在 parts 里 | brain 单测（`staff_streaming`、`staff_workflow`） |
| v1 快照备份为 `tasks.v1.bak.json` 后空库启动 | TaskStore 单测 |
| 加载时 `Running` → `Failed` | TaskStore 单测 |
| 回答问题追加 `Answered`，不产生 `User` | `staff_questions` 单测 |
| Worker 提问为顶层 `WorkerQuestion`，问题本体在 `Task.questions` | `staff_native_question_tests`、brain 单测 |
| 被替换提议保留并标记 `superseded` | `staff_tasks` 单测 |
| 幕僚调用 `task reply` 被宿主拒绝 | `staff_cli` / 授权单测 |
| 模型快照不含 Step / Plan，`Answered` 展开为问答文本 | `staff_prompt` 单测 |
| 步骤标题映射，不含绝对路径 | App 单测 |
| 停止后状态 `Stopped`、已生成内容保留 | brain 单测 + desktop-e2e |
| 流式中的回合（过程区展开、计时、停止按钮） | desktop-e2e `--visual` |
| 完成后折叠，重启后一致 | desktop-e2e `--visual` |
| 问题卡 / 提议卡嵌在回合里，无重复气泡 | desktop-e2e `--visual` |
| 事件条（未送达 / 已解决）与 Worker 结果折叠块 | desktop-e2e `--visual` |
| 复制、重新生成 | desktop-e2e + 单测 |
| 浅色 / 深色、中 / 英 | desktop-e2e `--visual` |
