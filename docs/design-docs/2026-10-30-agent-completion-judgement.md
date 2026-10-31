# Agent 任务完成判定:区分「真正完成」与「后台异步导致的暂停」

状态:已实施(仅 Claude)。日期:2026-10-30。取证对象:Claude Code 2.1.296。

## 1. 问题

`crates/norma/src/agent_hook/event.rs:85` 把每个 `Stop` 映射为 `AgentHookTaskState::Done`。
Agent 用 `run_in_background` 启动后台命令或后台子 Agent 后,本轮会先 `Stop`,实际仍在等后台结果。
Done 随即被卡片、播报、移动端推送和 `crates/app/src/app/agent_runtime/coordination/task_result.rs:48`(记 `succeeded`)消费,造成「未完成却报完成」。

根因:把单个 provider 事件(`Stop`)当成「任务完成」。`Stop` 只表示**回合结束**。

## 1.1 取证结论

用独立 command hook 抓了四次 `claude -p` 的真实 payload(单后台、双后台、TaskStop 杀死、后台子 Agent)。用户全局 `~/.claude/settings.json` 未改。路径与 transcript 不入库。

- 登记发生在 `PostToolUse`,且 `tool_input.run_in_background == true`。工作 id 优先 `tool_response.backgroundTaskId` 或 `tool_response.agentId`,否则 `tool_use_id`。`PostToolUseFailure` 与前台工具不登记。
- `Stop` / `SubagentStop` 若带 `background_tasks` 数组,用该数组的 id **替换**开放集合。字段缺失不是空快照,不清已登记的工作。`session_crons` 不计入开放工作。
- 单后台:非空 `Stop` 之后约 1177ms 才来 `<task-notification>`,所以非空快照不能靠 800ms 放行。
- 结清:工具名 `TaskStop`(旧名 `KillBash` / `KillShell` 仍认),id 来自 `tool_input.task_id` 或 `tool_response.task_id`。本次没有 `BashOutput`,不把它当结清信号。
- 唤醒是 `UserPromptSubmit`。payload 没有 `source` 字段。完整 prompt 里的 `<task-notification>` 在 status 非空且不是 `running` 时,按 `<task-id>` 与 `<tool-use-id>` 删除。普通 prompt 不清账本。
- 子 Agent:`SubagentStop` 的快照仍列出 `status: running` 的自己,不能当成结清。父会话的空 `Stop` 可以比 notification 早 36ms,确认窗口要吃掉这个间隔。
- 最终 `Stop` 后约 34ms 就有 `SessionEnd`。空账本窗口内的 `SessionEnd` 先确认成功再转发结束。假 CLI 若要单独观察 800ms 窗口,必须在窗口结束前保持存活。

## 2. 目标与非目标

目标:
- 后台工作未结清时不报 Done;所有 profile 共用同一判定框架。
- Done 之后若 Agent 又被唤醒,状态可回退,且外部副作用不提前发出。
- 任何等待都有上限,不会永久卡在进行中。

非目标:
- 本期不新增用户可见的 WaitingBackground 状态,内部保持 Thinking(待产品决定,见 §8)。
- 不做逐 provider 的深度适配,先 Claude,其余在抓到真实 payload 后接入。

## 3. 核心模型

完成 = 回合结束 **且** 声明等待的工作集合为空 **且** 经过确认窗口仍无后续活动。

```
事件流 ─▶ 排序/去重 ─▶ 工作账本 (按节点) ─▶ 完成判定器 ─▶ 状态 + 外部副作用
                         ▲                       ▲
                  事件层声明(主)          进程层交叉检查(辅)
```

### 3.1 工作账本(纯逻辑,无 GPUI)

实现在 `crates/norma/src/agent_hook/work_ledger.rs` 的 `CompletionJudge`。只拦截 Claude;其他 profile 的 `Stop` 仍立即 Forward。

- 每节点一个 `HashSet<WorkId>`。
- 登记:Claude `PostToolUse` 且 `tool_input.run_in_background == true`。id 优先 `backgroundTaskId` / `agentId`,否则 `tool_use_id`,并记下 tool_use_id 别名。`SubagentStart` 的 `agent_id` 也登记。`PostToolUseFailure` 与前台工具不登记。
- 快照:`Stop` / `SubagentStop` 带 `background_tasks` 数组时,用数组 id 替换开放集合。缺字段只走增量账本,且账本已空时仍进入确认窗口,否则旧 hook 与无后台的回合永远不 Done。
- 显式删除:`TaskStop` / `KillShell` / `KillBash` 按 `task_id` 删。`<task-notification>` 在 status 不是空也不是 `running` 时,按 `<task-id>` 与 `<tool-use-id>` 删。删到空也不马上 Done,要等下一次空快照的 `Stop` 加上确认窗口。
- `SubagentStop` 有快照就替换(实测仍可能是 running);缺字段才按 `agent_id` 删除。它本身 Forward,并取消窗口。
- `stop_hook_active == true` 与普通 `Stop` 相同：被阻止后续跑和最终那次 Stop 都带 true，只能靠续跑活动作废窗口，否则卡片会永久运行。Grok 同样进入本账本：子会话（带 `subagentType`）的生命周期事件与会话关闭时 `reason != end_turn` 的 Stop 直接丢弃。
- 清理:账本非空的 `SessionEnd`、`StopFailure`、Agent 进程退出(`abandon`)整体清空,不补发 Done。空账本且确认窗口仍开着时,`SessionEnd` 先 Confirm(`unsettled=false`)再 Forward。
- 新的 `UserPromptSubmit` 不清账本,但取消待定窗口,并记下 `last_prompt_unix_ms`。

### 3.2 事件排序

hook 由脚本经 HTTP 异步转发,`PostToolUse(run_in_background)` 可能晚于 `Stop` 到达。
- 事件带单调序号或接收时间戳,同节点按序处理。
- `Stop` 到达后先进入确认窗口(§3.3),窗口内到达的更早登记事件仍计入账本。
- 空 `Stop` 会清掉 30 分钟时钟。随后迟到的 `PostToolUse(run_in_background)` 或 `SubagentStart` 把 id 放进账本时,从该事件重新武装时钟。之后没有下一次 `Stop`,到点仍降级 Done 并带未结清标记。

### 3.3 完成判定器与确认窗口

`TurnEnded` 后:

1. 账本非空 → 保持 Thinking,不发任何完成副作用。
2. 账本为空 → 进入确认窗口(默认 800ms,常量集中定义)。窗口内:
   - 收到 `PreToolUse` / `PostToolUse` / `UserPromptSubmit` 等活动事件 → 判定作废,回 Thinking。
   - 进程层交叉检查发现 Agent 进程树下有本回合新增且存活的后代进程 → 延后一个窗口并复查,最多 N 次,之后按账本为准。
3. 窗口结束仍无活动 → Done。**播报、移动端推送、coordinator 结算、`task_result` 的 `succeeded` 只在此刻触发。**窗口尚未结束就收到 `SessionEnd` 且账本仍为空时,立刻确认,再转发 `SessionEnd`。同一 drain 的结算使用发布当时的状态,不读 drain 结束时的实时状态。

Done 为可撤销状态:Done 后收到活动事件 → 回 Thinking(外部副作用已发出的情形只在确认窗口过短时出现,窗口值需实测)。

### 3.4 进程层(辅助,不当权威)

只在 `TurnEnded` 时做一次查询,用于发现账本漏登记的后台工作:
- 不使用「每次 prompt 重置基线」;改为只关注 Norma 启动该 Agent 之后、且 `UserPromptSubmit` 之后新出现的后代进程,排除已知常驻类(MCP server 等,按进程名/启动时间窗口排除)。
- 已知盲区:`nohup`/`setsid` 脱离进程树的任务查不到;云端/沙箱 Agent 的进程不在本机。因此它只能「延后判定」,不能单独决定 Done 或不 Done。
- 查询在后台线程执行,不阻塞 GPUI 前台(见 CLAUDE.md「Never block the GPUI foreground path」)。

### 3.5 超时兜底

- 账本非空时的最长等待(默认 30 分钟,常量集中定义)。超时后降级为 Done,并在任务结果里带「可能有后台任务未结清」标记。
- 后台任务被杀且不再唤醒 Agent 时,不会让 coordinator 与移动端 pending 永久悬挂。

## 4. 与现有代码的接入点

| 位置 | 变更 |
| --- | --- |
| `crates/norma/src/agent_hook/event.rs` | 原始 `Stop` → Done 映射保持不变。既有 profile 测试依赖它。拦截只发生在 drain |
| `crates/norma/src/agent_hook/work_ledger.rs` | 账本与判定器。确认后仍转发原来的 `Stop` |
| `crates/app/src/settings/agent_hooks.rs` `drain_into_runtime` | Claude 未确认的 Stop 记成 `BackgroundPending` / Thinking,不进 `terminal_tasks`、不进会触发播报和移动端完成的事件列表 |
| `crates/runtime` `agent_descendants.rs` | 后代探针。只延后,最多 3 次;扫不全则信任账本 |
| `AgentTaskResult.background_unsettled` | 30 分钟后降级 Done 的结构化标记。播报在展示边界追加 `voice.background_unsettled` |

注意:不能只丢弃 `Stop` 事件。下游 `observe_mobile_hook`、播报等按 `provider_event == "Stop"` 判断,必须由判定器统一决定何时向它们转发。

## 5. 分阶段实施

1. 取证:已完成,结论见 §1.1。
2. 判定器与账本 + 单测:已完成,只覆盖 Claude。
3. 接入 `drain_into_runtime`:已完成。未确认的 Stop 不发完成副作用。
4. 进程层交叉检查:已完成。只延后,最多 3 次。
5. 其余 profile 未取证,保持「Stop 立即 Done」。接入前仍会误报。

## 6. 测试与验收

- 单测(`crates/norma/tests/`):
  - 单后台:启动 → Stop → 不 Done;结清后 Stop → Done。
  - 双后台:第一个结清后的 Stop 不 Done;全部结清后 Done。
  - 乱序:`Stop` 先于 `PostToolUse(run_in_background)` 到达,仍不 Done。
  - Done 后收到活动事件回到 Thinking。
  - 跨回合:新 prompt 不清账本。空窗口内的 `SessionEnd` 先确认成功再结束;账本非空的 `SessionEnd` 只清账本。
  - 超时降级为 Done 且带标记。
  - 前台工具、其他节点、`PostToolUseFailure` 不登记。
- 集成:`crates/app/tests/unit/agent_hook_completion_tests.rs`。账本非空时 `terminal_tasks` 为空,事件列表没有 `Stop`,卡片保持 Thinking。播报和任务结算都只消费 `terminal_tasks`。
- 真实 App:`bash scripts/desktop-e2e.sh agent-background-completion`。假 Claude 发 `PostToolUse(run_in_background)` → 非空 `Stop` → 结清 → 空 `Stop`。持有期是「运行中」且没有 Done;窗口结束后才是 Done。语音默认关,完成播报不在这条场景里验收。

## 7. 风险

- 确认窗口让真正的完成通知晚约 800ms。子 Agent 的 notification 可以比空 Stop 晚 36ms,窗口按这个间隔取的。
- 进程探针扫不到 `nohup` / `setsid` 和云端 Agent,只能延后,不能单独决定完成。进程名含 `mcp` 的常驻后代排除在外。
- Codex、Cursor、Grok、Kimi、Hermes、Pi、ZCode、OpenCode、InfCode 尚未接入。它们的 Stop 仍立即 Done。
- `claude -p` 的 SessionEnd 紧跟最终 Stop。空账本时同一 drain 先发 Done 再发 Ended。任务结算使用发布当时的状态,避免最终的 Ended 把刚成功的任务记成中断。

## 8. 已定与仍开放

- 本期内部保持 Thinking,不新增 WaitingBackground。卡片 Done 徽章仍是硬编码 `Done`。
- 确认窗口 800ms,账本非空最长 30 分钟,进程探针最多延后 3 次。常量在 `work_ledger.rs`。
