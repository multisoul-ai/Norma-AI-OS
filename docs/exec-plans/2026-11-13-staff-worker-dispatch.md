# 幕僚派发 Worker：通用投递方案

- 分支：`feat/staff-m1-20261112`（worktree `pr-235`）
- 状态：已实施，待用户确认后提交
- 背景：幕僚模式派发给 OpenCode Worker 的任务，60 秒后显示"执行失败"，卡片其实已启动但 prompt 从未打入。

## 1. 根因

幕僚派发只在收到 `SessionStart`（映射为 `AgentTaskState::Ready`）后才打字，60 秒内没收到就判 `staff.worker_not_ready`：

- `crates/app/src/app/brain_host/staff/dispatch.rs:78-83`：spawn 后放入 `staff.pending`，只有已是 Ready 才立即投递。
- `crates/app/src/app/brain_host/staff/input.rs:58-72`：`expire_staff_workers` 60 秒判失败。
- `crates/app/src/app/frame_loop.rs:301-307`：只有 `ready_node_ids` 触发 `submit_staff_ready_worker`。

`SessionStart` 不是通用的"可输入"信号（证据见 `docs/research/`）：

| Agent | 启动即发 Ready | 证据 | 送达事件 |
|---|---|---|---|
| Pi | 是 | `docs/research/2026-10-18-pi-coding-agent.md:229` | `before_agent_start` → `UserPromptSubmit` |
| Hermes | 是（插件 register） | `docs/research/2026-10-18-hermes-agent.md:173` | `pre_llm_call` → `UserPromptSubmit` |
| OpenCode | 否，`session.created` 才发 | `crates/norma/src/agent_hook/opencode.rs:180` | `UserPromptSubmit` |
| ZCode | 否，首次提交才发 | `docs/research/2026-10-18-zcode.md:67` | `UserPromptSubmit` |
| Kimi | 否（新会话）；首次进目录有信任弹窗 | `docs/research/2026-10-18-kimi-code-cli.md:59,90` | `UserPromptSubmit` |
| Codex | 文档称 startup 发，TUI 未实测 | `docs/research/2026-07-12-codex-hooks-0.144.1.md:24` | `UserPromptSubmit` |
| Claude / Cursor / Grok / InfCode | 未核实 | — | `UserPromptSubmit` / `beforeSubmitPrompt` |

结论：Ready 只能用来加速投递；10 个 Agent 都有送达事件，送达事件才是真相。

其他问题：

- 失败后 Norma 重新提议，采用后新建 Worker，旧卡闲置，任务里出现两个同名 Worker。
- 派发后立即视为执行中，未确认 prompt 已送达。

## 2. 目标规则

1. **何时尝试投递**：Ready hook、PTY 输出静默、30 秒宽限，任一先到。
2. **是否送达**：只认该节点的 `UserPromptSubmit`（`AgentHookReport::started_tasks`）。未送达前界面显示"启动中"。
3. **送达超时**：Enter 后 15 秒仍无送达事件 → Worker 转"需要你"，不自动重试（避免往信任弹窗里反复按 Enter）。
4. **失败**：只有 spawn 失败或进程退出才判"执行失败"。
5. **重新投递**：对"需要你"的 Worker 投递到同一张卡，按 `dispatch_id` 去重，不新建 Worker。

不复用 `pending_agent_submissions` 的结构：它绑定 `AgentDispatchReceipt` 与任务协调器（`crates/app/src/app/agent_runtime/pending.rs:215-360`）。只复用规则与常量（`AGENT_STARTUP_GRACE`、`AGENT_ENTER_DELAY`）。

## 3. 实施步骤（TDD）

### 步骤 1：把投递时机抽成纯状态机

- `crates/app/src/app/staff.rs`：`PendingWorker` 增加 `stage`：`Starting { since }` → `Typed { at }` → `Submitted { at }`。
- 新增纯函数（无 GPUI）：输入 `stage`、`now`、`ready`、`quiet`，输出 `Wait | Type | Enter | NeedsYou`。
- 测试先行，覆盖：
  - 无 Ready、30 秒后输出 `Type`（OpenCode / ZCode / Kimi 场景）。
  - 收到 Ready 立即 `Type`。
  - 输出静默立即 `Type`。
  - `Submitted` 15 秒无送达 → `NeedsYou`；收到送达 → 移出队列。
  - `dispatch_id` 不匹配的节点不投递。
- 把 `AGENT_STARTUP_GRACE` 改为 `pub(crate)` 供复用，不复制数值。

### 步骤 2：接入帧循环

- `crates/app/src/app/brain_host/staff/input.rs`：
  - 删除"60 秒判 `worker_not_ready`"。
  - `expire_staff_workers` 改为 `tick_staff_workers(now)`，按步骤 1 的决策执行 `type_agent_task` / `submit_agent_enter`。
- `crates/app/src/app/frame_loop.rs:301`：保留 Ready 触发；新增对 `agent_hook_report.started_tasks` 的处理，命中 staff 节点即确认送达。
- 打字或 Enter 失败沿用现有 `staff_worker_failed`。

### 步骤 3：PTY 输出静默信号

- 现状：runtime/terminal 没有"最后输出时间"。
- 在 `TerminalSession`（`crates/terminal`）记录最后一次 PTY 输出的 `Instant`，经 runtime 暴露 `last_output_at(node)`。只在 drain 路径赋值，不加锁、不加 IO（遵守性能护栏）。
- 静默判定：首次输出后静默 ≥ 1.5 秒视为可尝试。阈值是常量，带一行注释说明来源。
- 单测：terminal crate 内验证时间戳更新。

### 步骤 4："需要你"状态与同卡重投

- `crates/norma/src/protocol/staff.rs`：新增 Worker 消息或状态表示"未确认送达，需要你查看"（例如 `TaskMessage::WorkerNeedsAttention { node_id, dispatch_id }`），任务显示状态归入现有 `needs_you`。
- `crates/app/src/app/staff/messages.rs`：渲染该消息，提供"重新投递"和"在画布中查看"两个按钮（用 `ui::component`，四态齐全）。
- 重新投递：同节点、同 `dispatch_id` 重新入队为 `Starting`（宽限视为已过），不调用 `reserve_staff_worker`。
- brain 侧：Worker 处于"需要你"时，不应为同一提议再生成派发提议。检查 `crates/brain` 的提议逻辑，必要时加测试。
- 文案：`crates/ui/locales/staff.zh-CN.json` 与 `staff.en.json` 新增 `staff.worker_starting`、`staff.worker_needs_attention`、`staff.redeliver`；删除不再使用的 `staff.worker_not_ready`。运行 `python3 scripts/check-i18n.py` 与 `python3 scripts/test-i18n.py`。

### 步骤 5：Worker chip 状态

- 任务头部 Worker chip：未送达显示"启动中"，送达后才显示"执行中"，"需要你"单独样式。
- 设计参考 `.agents/skills/apple-design`；验收记录写明采纳的设计决策。

## 4. 不在本次范围

- 各 Agent 信任弹窗的预处理（启动参数或配置）。需逐个实测，另开任务。
- 改用启动参数直接传 prompt。
- OpenCode 插件改动（通用方案不需要）。

## 5. 验收

| 标准 | 证据 |
|---|---|
| 无 Ready 的 Agent 在宽限或静默后被投递 | 步骤 1 单测 |
| 未收到送达事件不显示"执行中" | 步骤 1 单测 + desktop-e2e |
| 送达超时转"需要你"，不新建 Worker | 步骤 1、4 单测 |
| 重新投递到同一张卡 | 单测 + desktop-e2e |
| OpenCode Worker 在真实 App 中收到 prompt 并开始执行 | `scripts/desktop-e2e.sh` 幕僚模块场景（`--visual`） |
| Claude Worker 派发不回归 | desktop-e2e 同上 |

### Worker chip 设计决策

参考 `.agents/skills/apple-design`：

- 采纳：状态用文字标签加已有语义色即时切换。启动中 `accent_violet`，执行中 `accent_cyan`，需要你 `accent_warm`，完成 `success`。颜色在按下后的同一帧替换，不播放动画。减少动态效果时也没有需要替代的动效。
- 采纳：重新投递是主按钮，使用现有 `staff_primary_style`（hover、pressed、disabled）。在画布中查看是次按钮，使用 `staff_button_style`。焦点由 `ui::component::Button` 承担。不改变按钮高度和字号。选中态不适用于这两个瞬时动作。
- 拒绝：送达超时后自动重试、为同一任务新建第二张 Worker、启动中播放动画。

提交前运行：

```bash
cargo test -p app -p terminal -p runtime -p norma -p brain
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-performance-guardrails.sh
python3 scripts/check-i18n.py && python3 scripts/test-i18n.py
```

## 6. 风险

- 往信任弹窗里按 Enter 会替用户选"信任"（Kimi 已确认，Claude、Codex 类似）。本方案只投递一次、不自动重试，送达超时即转"需要你"。画布现有 30 秒宽限投递有同样风险。
- 静默阈值过短可能在 TUI 仍在加载时打字。由送达确认兜底。
