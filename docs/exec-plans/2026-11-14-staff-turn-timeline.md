# 幕僚对话：一轮按到达顺序追加的时间线

## 原则

对话里的卡片和内容只能往后追加，已显示的内容不能被插入的新内容顶下去。

## 现状违反原则的 4 处

1. `crates/app/src/app/staff/view.rs` `staff_message_list`：活动行固定渲染在全部消息之后。幕僚在本轮中途通过 bash 调 `norma brain task propose`（建议卡）或 ACP 问答（`crates/brain/src/staff/questions.rs` `add_question`，问答卡）入库的卡片排在活动行上方，之后的步骤和流式文字长在卡片上方。
2. `crates/brain/src/lane/staff.rs` `insert_reply`：本轮结束时把整段回复插到本轮第一张建议卡之前。
3. `crates/app/src/app/staff/activity.rs` `reconcile`：回复落盘后清空流式文字，Norma 消息改为在活动行上方渲染，文字从工具行下方跳到上方。
4. `crates/app/src/app/staff/messages/text/activity.rs`：先渲染全部工具行，再渲染全部流式文字；新工具调用会插到已显示的文字上方。

## 目标行为

一轮 = 用户消息 + 本轮时间线。时间线按到达顺序依次包含：文字片段、工具行、卡片（问答卡、建议卡）。

- 新条目只追加到时间线末尾。已显示条目的相对顺序永不改变。
- 署名（Norma · 协调）和状态行（正在回复… · N 步 · 用时）固定在本轮顶部，只更新文字，不移动。
- 建议卡、问答卡入库后立即追加到时间线末尾，之后的文字和工具出现在卡片下方。
- 本轮结束：落盘的 Norma 回复不另行渲染，原位继续显示时间线里的文字片段（内容相同）。工具行保持原位，不折叠、不移动。
- 刷新或手机端读取（没有内存时间线）时，按存储顺序渲染；存储顺序必须与时间线一致：Norma(片段1)、卡片、Norma(片段2)……

## 前端改动

### `crates/app/src/app/staff/activity.rs`

- 新增 `pub enum TimelineEntry { Text(String), Tool(usize /* tools 下标 */), Card(usize /* task.messages 下标 */) }`，`TaskActivity` 新增 `pub timeline: Vec<TimelineEntry>`。
- `apply`：
  - `TraceKind::Reply`：若末项是 `Text`，追加到它；否则 push 新 `Text`。保留现有 `reply` 字段语义（汇总文字）供状态判断。
  - `TraceKind::Prompt`（纠错重来）：清空 `timeline` 中的 `Text` 项，与现有清空 `reply` 一致。
  - 新工具（`update_tool` 中 push 新 `ToolActivity` 时）：push `Tool(index)`。已有工具的状态更新不追加条目。
  - `Started`：重置时 `timeline` 清空。
- `reconcile(task)`：扫描 `task.messages[baseline..]`，对尚未出现在时间线中的 `Proposal`/`Question` 按下标顺序 push `Card(index)`。不再因为出现 Norma 消息就清空文字；`committed` 只用于停止接收迟到的 Reply 片段。
- 新增 `pub fn owns(&self, index: usize, message: &TaskMessage) -> bool`：`index >= baseline` 且时间线非空且（消息是 `Norma`，或该下标在时间线 `Card` 中）。用于让消息列表跳过本轮已由时间线负责的消息。
- 失败轮次（`failed`）：时间线照常显示；没有落盘的 Norma 消息，不需要特殊处理。

### `crates/app/src/app/staff/view.rs` `staff_message_list`

- 消息循环里跳过 `activity.owns(index, message)` 为真的消息。
- 活动视图改为渲染：署名、状态行，然后按 `timeline` 顺序渲染：`Text` 用现有流式文字样式（`staff-streaming-reply` 选择器对最后一个文字片段保留）；`Tool` 用现有工具行；`Card` 调 `render_message(task, index, &task.messages[index], …)`，保持原有 `staff-message-{index}` 选择器。
- 本轮之前的消息照旧按存储顺序渲染。

### `crates/app/src/app/staff/messages/text/activity.rs`、`activity_tools.rs`

- 把"全部工具行 + 一段文字"拆成可按单条渲染的函数，供时间线逐项调用。计划面板（`plan`）保持在状态行下方。
- 单文件 ≤ 500 行（`scripts/check-max-file-lines.sh`），超出则拆文件。

## 后端改动

### `crates/brain/src/lane/worker.rs`

- 本轮开始时记录 `before = task.messages.len()`（已有 `task` 快照）。
- 在 `prompt_observed` 的 `task_update` 回调里维护 `segments: Vec<(usize, String)>`（片段开始时本轮已入库的卡片数，文字）：
  - `TraceKind::Reply`：追加到最后一个片段的文字。
  - 工具类条目（`ToolCall`/`ToolResult`/`Permission`）以及问答（`QuestionHost` 写入问答卡后）：读一次 `env.tasks` 中该任务 `messages[before..]` 里 `Proposal`/`Question` 的数量；若与最后片段记录的数量不同，开新片段。只在工具边界读取存储，不在每个文字片段读取。
  - `TraceKind::Prompt`：清空 segments（与前端一致）。
- 把 segments 传给 `save_reply`。若 segments 文字拼接与 `reply` 不一致（例如适配器只在结束时给整段），退化为单段 `[(0, reply)]`，即放在本轮卡片之前的现有行为改为放在全部卡片之后，见下。

### `crates/brain/src/lane/staff.rs`

- 删除 `insert_reply`。`save_reply` 接收 segments：按顺序遍历，对每段 `(cards_before, text)`，插入位置 = 本轮第 `cards_before` 张卡片之后（`cards_before == 0` 时为 `before` 处，再加上已插入的 Norma 段数）。空白片段跳过。
- 退化单段时追加到消息末尾（全部卡片之后）。
- 保留"模型已用 `task reply` 写回时不重复入库"的判断。
- `supersede_stale_proposals` 不变。

## 测试（先写失败测试，再实现）

- `crates/app/tests/unit/staff_activity_tests.rs`：
  - 文字 → 工具 → 卡片 → 文字 → 工具，时间线顺序与到达顺序一致。
  - `reconcile` 发现新卡片只追加一次；落盘 Norma 消息后时间线不变、`owns` 对本轮 Norma 和卡片为真、对旧轮消息为假。
  - 下一轮 `Started` 后时间线清空，旧轮消息 `owns` 为假。
- `crates/brain/src/lane/staff.rs` 测试：segments `[(0,"前"),(1,"后")]` + 一张卡 → 存储顺序 Norma(前)、卡、Norma(后)；退化单段追加在末尾；`task reply` 已写回时不重复。
- headless 布局测试注意：`TestAppContext` 使用 `NoopTextSystem`，不能证明真实字体下的高度，只用于顺序断言。
- 真实 App（`docs/quality/desktop-e2e.md`，`--visual`）：
  - `staff-workspace-streaming`：`test/desktop-e2e/Fixtures/staff/StaffAdapter.swift` 的 `STREAM-ALPHA` 分支在 `ALPHA PARTIAL` 之后调用一次 `task propose`（标题 `ALPHA CARD`），在 `release-partial` 处截图：卡片在 `ALPHA PARTIAL` 和工具行之后；`release-final` 后截图：`ALPHA FINAL` 在卡片下方，之前内容位置不变。同步更新 `test/desktop-e2e/checks/staff/StaffWorkspaceStreaming.checks.md`。
  - `staff-workflow`：问答流程，回答后新步骤和文字出现在问答卡下方。

## 验收清单

- `cargo test -p app --lib staff::`、`cargo test -p brain` 通过。
- `cargo clippy -p app -p brain --all-targets` 无告警。
- `bash scripts/check-max-file-lines.sh`、`bash scripts/check-no-allow.sh`、`python3 scripts/check-i18n.py` 通过。
- 两个 desktop-e2e 场景 `--visual` 通过；若遇 socket 被其他 Norma 占用（ENV），记录并停止，不改为手动验收。
