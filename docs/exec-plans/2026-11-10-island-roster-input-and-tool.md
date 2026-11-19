# 灵动岛「运行中」列表：会话名 + 最近输入 + 最近工具

日期：2026-11-10
分支：`design/island-last-input-20261110`
工作树：`/Users/openclawd/Documents/code/norma-os/norma-island-last-input`
基线：`dde1cd48`（`dev`）
视觉依据：`docs/prototypes/island-2/island-2.html`（`?solo=1&still=1&mode=expanded&n=8&tab=running`），参考图 `docs/prototypes/island-2/reference/expanded-running.png`。生产实现一比一还原该行布局。

本文件是实施方案和验收记录。实现在上述工作树进行，经 PR 合并到 `dev`。

## 目标行为

展开岛 →「运行中」标签，每个智能体一行三层：

1. 名称 · provider · **会话名称**（单行截断）。
2. 人形图标 + **用户最近一次输入**（单行截断；没有则整行显示「尚未开始」）。
3. **最近运行的工具**：等宽字体，`工具名 + 参数摘要`。运行中行前是转圈，其余是灰点；运行中行右侧是耗时。没有工具记录则不画这一行。

右侧芯片（运行中 / 等你处理 / 已完成 / 待开始）与「定位」按钮不变。进度条保留。

## 现状（已核对源码）

| 需要的数据 | 现状 | 缺口 |
| --- | --- | --- |
| 会话名称 | `RunOverviewAgent.task`（来自 `CardSnapshot.task_title`，由 `app/agent_runtime/session_titles.rs` 刷新） | 岛没用它，见下 |
| 最近用户输入 | `RunOverviewAgent.latest_prompt` 已有 | 岛里 `Face.task = latest_prompt.or(task)`（`crates/app/src/island/faces.rs:398`），把它当标题，会话名丢了 |
| 最近工具 | 只有 `AgentTaskState::RunningTool{tool_name}`，工具结束就清掉；`live_label()` 只在运行时给「调用 Bash」 | 没有「最近一次工具」的持久记录，也没有参数摘要 |
| 当前第 2 行 | `Face.detail = RunOverviewAgent.action`（进度步骤或任务标题） | 被新两行取代 |

## 设计取舍

参考技能：`.agents/skills/apple-design`（层级、平台熟悉性）。本次没有新增动效，不加载 `emil-design-eng`。GPUI 以钉死的 `gpui =0.2.2` 与 `crates/app/src/island/running.rs` 现有写法为准。

- 层级：会话名是标题，最近输入是次级文字（`style::MUTED`），工具是第三级（`style::FAINT`，等宽）。三行都单行截断，行高随之固定，不因内容长度跳动。
- 反馈：运行中转圈沿用现有 `border` 圆环写法；不新增动画。减少动态效果无差异。
- 可访问性：行的 aria/无障碍文案保持名称 + 状态；工具行属于补充信息，不承载唯一信号。

### 隐私决策（需确认）

现有任务卡规则（`crates/runtime/src/runtime/mobile_cards/summary.rs`）：只拷 provider 给的人类描述，**不拷命令正文、文件内容、工具结果**。岛是桌面本地界面，但岛内容也会被截图和投屏，所以沿用同一条线：

- 工具名：始终显示。
- 参数摘要：`tool_input.description`（provider 的人类描述）优先；文件类工具（Read / Edit / Write 等）显示 **文件 basename**；其余只显示工具名。
- **不显示 Bash 命令正文。** 因此原型里的 `Bash cargo test` 在生产里是 `Bash 运行单元测试`（有 description 时）或仅 `Bash`。
- 摘要有界：沿用 `bounded_excerpt` 的上限；不读文件、不新增 IO。

如果你希望 Bash 也显示命令正文（仅限本机岛，不进 `CardSnapshot` 的移动端序列化），请在合并前告知；需要把摘要拆成「岛专用」与「移动端」两份，工作量约增加一个字段。

## 实施步骤（TDD，每步先写失败测试）

### 1. 运行时记录最近工具（`crates/runtime`）

- 新增 `RecentTool { name: String, arg: Option<String> }`，放在 `crates/runtime/src/events.rs` 旁，随 `agent_latest_prompts` 同生命周期（节点移除、`SessionStart` 清空、`clear` 时清除）。
- 写入点：处理 `PreToolUse` 的位置（与 `mobile_cards/summary.rs` 同一 hook 流）。`PermissionRequest` 也写入，这样等待审批的 Iris 显示正在请求的工具。
- 不在 `PostToolUse` 清空：「最近」指最近一次，完成后继续显示；新一轮 `UserPromptSubmit` 才清空，避免上一轮的工具挂在新输入下。
- 提取函数纯函数化：`recent_tool(raw_json) -> Option<RecentTool>`，参数规则同上，复用 `bounded_excerpt`。该文件已 `summary.rs`，新函数放新文件 `mobile_cards/recent_tool.rs`，避免 `summary.rs` 继续膨胀。
- 测试（`crates/runtime/tests/unit/`）：
  - Bash 带 `description` → 参数是 description；不带 → `arg=None`，断言结果不含命令正文。
  - Edit 带 `file_path` → 参数是 basename，不含目录。
  - `PostToolUse` 后仍保留；下一次 `UserPromptSubmit` 后为空；`SessionStart` 后为空。
  - 超长参数被截断到上限。

### 2. 传到概览（`crates/norma`）

- `CardSnapshot` 增加 `#[serde(default, skip_serializing_if = "Option::is_none")] pub last_tool: Option<RecentToolSnapshot>`，沿用 `latest_prompt` 的向后兼容写法，旧 iOS 客户端忽略该字段。
- `WorkspaceActivityAgentEntry`（`workspace_activity_view.rs:289`）和 `RunOverviewAgent`（`run_overview.rs:63`）各加一个 `last_tool`，在 `run_overview.rs:240` 附近透传。
- 所有构造 `RunOverviewAgent` / `CardSnapshot` 字面量的测试辅助函数补默认值：`crates/app/tests/support/run_overview_rows.rs`、`tests/unit/island_faces.rs`、`island_staff_tests.rs`、`run_overview_groups_tests.rs`、`run_overview_prompt_tests.rs`、`activity_cache/cards.rs`、`activity_cache/capture.rs`（比较字段也要加）。
- iOS：先确认 `ios/Sources/NormaCore` 解码对未知字段宽容（`skip_serializing_if` 保证 `None` 时不出现）；本期不改 iOS UI。
- 测试：`CardSnapshot` 序列化往返；`None` 时 JSON 不含 `last_tool`；`run_overview` 透传。

### 3. 岛的数据模型（`crates/app/src/island/faces.rs`）

- `Face` 拆字段：`session`（= `agent.task`，缺失回退卡片名之外的空串）、`last_input`（= `agent.latest_prompt`）、`tool: Option<(String, Option<String>)>`。删除 `Face.task` 的「latest 优先」逻辑和 `detail` 对运行行的使用（`detail` 仍给已完成行的结果摘要，见下）。
- 去重规则沿用 `agent_row_falls_back_without_duplicating_title`：最近输入与会话名相同时只显示一次——此时第 2 行仍显示输入（原型要求第 2 行始终是用户输入），会话名行保持不变，不隐藏。
- `faces.rs` 已 479 行，逼近 500 行上限：把 `face_from_agent` 及 `SyntheticFace` 相关抽到 `faces/agent.rs`，再改字段。
- 引用 `face.task` 的其他位置（完成通知文案、`queue_view`、`staff`）逐个 grep 并改用 `session`；不改它们的显示结果。
- 测试（`crates/app/tests/unit/island_faces.rs`）：有会话名和最近输入 → 两字段各归其位；无输入 → `last_input` 为空；工具透传；已完成 / 等你 / 排队各自保留正确字段。

### 4. 渲染（`crates/app/src/island/running.rs`，234 行）

- 第 1 行：`name · provider · session`（原本的 `face.task` 位置换成 `face.session`）。
- 第 2 行：`ui::component::Icon`（用户图标，确认 `IconName` 里有可用项，没有则用同尺寸 SVG 资源，不手绘 div）+ `truncate` 文本；无输入时「尚未开始」。
- 第 3 行：状态点（运行中沿用 `border` 圆环，其余 6px 灰点）+ 工具名（semibold，`MUTED`）+ 参数（`truncate`，`FAINT`，等宽字体取 `style` 里已有等宽字体常量）+ 右侧耗时（仅运行中，沿用 `island.elapsed_minutes`）。
- `running_detail`、`elapsed_minutes` 保留；进度条保留在三行下方。
- 新行高度变化会影响岛的 `fit` 高度：检查 `running_filter.rs`、`body.rs` 里用于估算行高的常量并同步，保证 8 个智能体时滚动区仍可滚动、不裁切。
- 交互状态：行 hover 已有；本次不新增可点击控件，无新增 pressed / focus 需求。

### 5. 国际化

- 本次基本不新增文案：「尚未开始」沿用 `island.not_started`。如果工具行需要无参数时的占位或无障碍标签，新增 `island.recent_tool` 到 `island.en.json` / `island.zh-CN.json`，占位符一致。
- 用户输入、会话名、工具名、参数属于用户内容 / 原始诊断，不翻译。
- 运行 `python3 scripts/check-i18n.py` 与 `python3 scripts/test-i18n.py`。

## 验收

| 标准 | 证据 |
| --- | --- |
| 行展示会话名而不是最近输入作标题 | `island_faces` 单测 |
| 第 2 行是最近用户输入，缺失显示「尚未开始」 | `island_faces` 单测 + e2e 看图 |
| 第 3 行是最近工具，完成后仍保留，新一轮输入后清空 | runtime 单测 |
| 不泄漏命令正文 / 完整路径 | runtime 单测（负断言） |
| 旧移动端 JSON 兼容 | norma 序列化测试；`None` 不出现字段 |
| 单行截断、8 个智能体不裁切、滚动正常 | desktop-e2e 看图 |
| 与原型一致 | 对照 `reference/expanded-running.png` |

真实 App 验收：在 `test/desktop-e2e/Scenarios/island/IslandModule.swift` 新增「运行中列表三行」场景，并在模块 `visualScenarioNames` 登记看图版；运行 `scripts/desktop-e2e.sh island`。夹具用 hook 注入 `UserPromptSubmit` + `PreToolUse`（可复用 `IslandHookSupport.swift`）。OCR 报 e5rt / Vision 不可用视为 ENV，改走 `--visual`。深色岛表面不跟随工作空间主题，浅色 / 深色工作空间各看一张。

提交前：`cargo test -p runtime -p norma`、`cargo test -p app island`、`cargo check -p app`、`bash scripts/check-max-file-lines.sh`、`bash scripts/check-no-allow.sh`、`python3 scripts/check-i18n.py`、`cargo tree -p brain | grep -iE "gpui|runtime|storage|agent "`（应无输出，本次不应触碰）。代码 review 使用独立 subagent 做对抗式审查。

## 范围外

- 「需要你处理」卡片、完成通知、收起态、幕僚页（原型和参考图本轮未改）。
- iOS 界面。
- 按工具类型的图标；工具点击跳转。
- 工具历史列表（只保留最近一次）。

## 风险

- 本次是跨 crate 加字段（runtime → norma → app），测试辅助构造点多，漏补会编译失败而不是静默出错。
- `PreToolUse` 的 `tool_input` 结构因 provider 而异（Claude / Codex / Grok / Kimi 各自 hook）。提取函数缺字段一律回退为仅工具名，并为每个已支持 provider 至少一条 fixture 测试；Hermes / Pi 等无 `PreToolUse` 的 provider 不显示第 3 行，属预期。
- 第 3 行让每行变高，8 个以上智能体时依赖滚动；行高常量不同步会导致裁切，由 e2e 看图兜底。
