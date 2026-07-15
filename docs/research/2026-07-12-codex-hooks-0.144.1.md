# Codex CLI 0.144.1 Hook 事件与 Norma OS 覆盖核查

> 日期：2026-07-12  
> 范围：本机 `codex-cli 0.144.1`（Apple Silicon）可配置的 command hook 事件；不把较新主分支或其他 Agent 的事件倒灌为该版本能力。  
> 方法：本机安装包版本、OpenAI 官方 `rust-v0.144.1` 标签源码，以及本项目/本机 Hook 配置。未使用二手资料。

## 结论

Codex CLI 0.144.1 实际支持 **10 个**事件，而不是 8 个：`PreCompact` 和 `PostCompact` 也在其公开的事件列表中。Norma OS 的 Codex 安装清单和当前 `~/.codex/hooks.json` 都只转发 8 个，缺少这两个 compaction 事件。

`SessionEnd`、`StopFailure`、`PostToolUseFailure` **不在** 0.144.1 的 10 项枚举中，因此不能视作此版本可注册、会触发的 Codex Hook。它们是 Norma 的跨 Provider 状态映射能力，不是 Codex 0.144.1 的已证实能力。

## 证据与版本边界

- 本机命令 `codex --version` 输出：`codex-cli 0.144.1`。
- 本机 npm 包：`@openai/codex@0.144.1`；其 `package.json` 指向 OpenAI 官方仓库 `openai/codex`。
- OpenAI 官方标签 [`rust-v0.144.1`](https://github.com/openai/codex/tree/rust-v0.144.1) 的 [`HOOK_EVENT_NAMES`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/hooks/src/lib.rs#L18-L30) 明确列出下表全部 10 个事件。
- 本表只保证上述版本。Codex 后续版本可能增加、删除或改变事件/触发语义；升级时须重新核对该版本源码或官方文档。

## 事件、触发时机与当前覆盖

| Codex 0.144.1 事件 | Codex 的实际触发时机 | Norma 是否安装/转发 | 当前卡片状态映射 |
|---|---|---|---|
| `SessionStart` | 新会话初始化、恢复、`/clear` 或 compact 后重新开始会话生命周期；`source` 可为 `startup`、`resume`、`clear`、`compact`。 | 是 | `Ready` |
| `UserPromptSubmit` | 每次用户输入已进入 turn、在正常 Agent 执行前运行。 | 是 | `Thinking` |
| `PreToolUse` | 一个支持 Hook payload 的工具将要执行前；Hook 可阻止或改写输入。 | 是 | `RunningTool(tool_name)` |
| `PermissionRequest` | 工具操作需要权限决策时，在向用户/审批机制请求前运行；Hook 可返回 allow/deny。 | 是 | `WaitingApproval(tool_name)` |
| `PostToolUse` | 支持该 Hook payload 的工具产出结果后；不覆盖所有工具，且 Codex 没有独立的 `PostToolUseFailure` 事件。 | 是 | `Thinking` |
| `PreCompact` | 上下文 compact 开始前，手动和自动 compact 都会触发；payload `trigger` 为 `manual` 或 `auto`。 | **否** | 无（收到也会被忽略） |
| `PostCompact` | 上下文 compact 已完成后，手动和自动 compact 都会触发；payload `trigger` 为 `manual` 或 `auto`。 | **否** | 无（收到也会被忽略） |
| `SubagentStart` | 由 thread-spawn 创建的子 Agent 开始时。内部/合成子 Agent 不暴露用户配置的生命周期 Hook。 | 是 | `RunningTool("subagent")` |
| `SubagentStop` | 上述 thread-spawn 子 Agent 的 turn 正常结束时；根 Agent 的同一位置发的是 `Stop`。 | 是 | `Thinking` |
| `Stop` | 根 Agent 当前 turn 不再需要 follow-up、准备结束时运行。它是“正常 turn 收束”Hook：手动 interrupt 会中止任务为 `TurnAborted`，而非走该正常收束调用点；进程被杀/崩溃也没有可靠的 Hook 送达保证。 | 是 | `Done` |

触发时机的一手依据：

- `SessionStart` / `SubagentStart` 的分流和内部子 Agent 排除，见 [`hook_runtime.rs`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/core/src/hook_runtime.rs#L102-L153)；session source 枚举见 [`session_start.rs`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/hooks/src/events/session_start.rs#L22-L37)。
- `UserPromptSubmit` 在 SessionStart 后、正常 turn 执行前调用，见 [`turn.rs`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/core/src/session/turn.rs#L186-L192)。
- `PreToolUse` 的“执行前”、可阻止/改写语义，见 [`hook_runtime.rs`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/core/src/hook_runtime.rs#L158-L215) 与实际工具调度顺序 [`registry.rs`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/core/src/tools/registry.rs#L493-L539)。
- `PermissionRequest` 返回 allow/deny 的 schema，见 [`permission-request.command.output.schema.json`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/hooks/schema/generated/permission-request.command.output.schema.json#L5-L59)。
- `PostToolUse` 的“成功输出后”语义和 success 条件，见 [`hook_runtime.rs`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/core/src/hook_runtime.rs#L258-L295) 与 [`registry.rs`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/core/src/tools/registry.rs#L570-L598)。
- 两个 compact Hook 的前后顺序，见 [`compact_token_budget.rs`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/core/src/compact_token_budget.rs#L68-L89)；`manual` / `auto` 标签见 [`hook_runtime.rs`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/core/src/hook_runtime.rs#L770-L775)。
- 根 turn `Stop` 与子 turn `SubagentStop` 的分流，见 [`hook_runtime.rs`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/core/src/hook_runtime.rs#L297-L365)；正常收束调用点见 [`turn.rs`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/core/src/session/turn.rs#L372-L415)。手动 interrupt 的实现是 `abort_all_tasks(TurnAbortReason::Interrupted)`（[`session/mod.rs`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/core/src/session/mod.rs#L3960-L3966)），而 turn 对 `TurnAborted` 直接返回（[`turn.rs`](https://github.com/openai/codex/blob/rust-v0.144.1/codex-rs/core/src/session/turn.rs#L419-L421)）；这证明不能把 `Stop` 当作 interrupt 的必达通知。

## Norma OS 覆盖的本机证据

1. [`crates/norma/src/agent_hook/event.rs`](../../crates/norma/src/agent_hook/event.rs) 的 `HookSource::Codex` 安装清单包含：`SessionStart`、`UserPromptSubmit`、`PreToolUse`、`PostToolUse`、`PermissionRequest`、`Stop`、`SubagentStart`、`SubagentStop`（8 个）。同文件的状态映射决定了表中的卡片状态。
2. 本机 `~/.codex/hooks.json` 当前也正好有这 8 个顶层事件键，未包含 `PreCompact` 或 `PostCompact`。
3. 安装代码遍历上述清单写入 `~/.codex/hooks.json`，见 [`crates/norma/src/agent_hook/install.rs`](../../crates/norma/src/agent_hook/install.rs)。因此 Settings 的“安装 Codex Hooks”不会自动补齐这两个真实支持的事件。

## 与“终止后动画仍运行”问题的关系

`Stop` 只覆盖正常 turn 收束，不能被当作进程崩溃、外部 kill 或手动中断必达的终态信号。并且 `StopFailure` / `SessionEnd` 在本机 0.144.1 不存在，不能通过“注册缺失事件”补救。因此 UI 的运行态清理仍需要独立的本地进程/PTY 生命周期兜底；补 `PreCompact` / `PostCompact` 只会完善上下文整理可见性，不能修复终止动画。
