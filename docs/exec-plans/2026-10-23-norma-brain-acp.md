# Norma 大脑（ACP）实施计划

日期：2026-10-23

设计见 [`docs/design-docs/2026-10-23-norma-brain-acp.md`](../design-docs/2026-10-23-norma-brain-acp.md)，原型见 [`docs/prototypes/norma-brain/`](../prototypes/norma-brain/README.md)。分支：`feat/norma-brain`。

## 目标

用户开启“语音汇报”后，Norma 实时汇报所有 Agent 的进度。后台大脑通过 ACP 调用用户已安装的 Claude Code、Codex 等，整理事件、判断审批、给空闲 Agent 派后续任务；高风险操作交给用户确认。前台普通对话保持现状。

## 代码结构

依赖方向：`brain -> norma`；`agent -> norma`（不变）；`app -> brain`。`brain` 不得依赖 `gpui`、`runtime`、`storage`、`ui`、`agent`。`agent` 不得依赖 `brain`，`NormaSession` 通过 `BackgroundHandoff` trait 转交后台任务，由 `app/brain_host` 实现。

```text
crates/brain/src/
  lib.rs            对外导出 BrainService、BrainEvent、BrainStatus、BrainConfig、risk_class
  event.rs          BrainEvent（输入）、BrainStatus（输出）
  router.rs         去抖、定时摘要、分发到两条 lane
  lane/main.rs      主会话
  lane/approval.rs  审批会话
  lane/prompt.rs    把事件渲染成发给大脑的文字（纯函数）
  acp/client.rs     子进程生命周期、session/prompt、重连
  acp/gate.rs       request_permission 策略：norma 命令与工作空间内只读文件
  backend.rs        检测已安装的 CLI 与适配器，按策略选择模型
  goal.rs           目标摘要与决策日志
  risk.rs           高风险规则，纯函数，无 IO
crates/norma/src/protocol/brain.rs         Approval、Stop、Report、Tail 命令与结果
crates/norma/src/cli/approval_command.rs   approval list / decide
crates/norma/src/cli/report_command.rs     report、agent tail、agent stop
crates/agent/src/master/handoff.rs         BackgroundHandoff trait 与“转给后台”工具
crates/app/src/app/brain_host.rs           后台执行器上运行 BrainService，转换 BrainStatus
crates/app/src/app/approval_broker.rs      挂起的审批、用户接管、接近 120 秒上限时转人工
crates/app/src/app/high_risk_guard.rs      调用 brain::risk；大脑只能升级，不能降级
crates/app/src/norma/server/brain.rs       处理新命令
crates/app/src/settings/brain/             设置页
```

`risk_class` 只做词法判断：路径需要由调用方先规范化并解析符号链接。`high_risk_guard` 负责这一步。

## 阶段

| 阶段 | 内容 | 验证 | 状态 |
| --- | --- | --- | --- |
| 0 实测 | Claude Code ACP 的首字延迟与权限请求形态；确认适配器包名 | `docs/research/2026-10-23-acp-adapters-probe.md` | 完成；hook 满 120 秒后的行为、Codex 适配器未测 |
| 1 纯逻辑 | `brain` crate：`risk`、`event`、`router` | `cargo test -p brain` | 完成 |
| 2 协议 | `norma brain report/approvals/decide/agents/assign`；client 带 Workspace 身份 | `cargo test -p norma --test brain_cli_tests` | 完成（没有做 `agent tail`、`agent stop`：停止沿用现有 `agent task cancel` 的内部路径，大脑暂不需要） |
| 3 ACP client | 手写 client、闸门、后端检测、目标摘要、两条 lane、服务 | `cargo test -p brain`（假适配器）；真实 Claude Code 冒烟 | 完成 |
| 4 App 接入 | `brain_host`、审批轮询与决定、播报接入、概览“Norma 判断中”、`handoff_to_brain` | `cargo test -p app --lib`、`cargo test -p agent` | 完成 |
| 5 设置与验收 | 设置页（开关、后端、策略、安装适配器）；真实 App 验收 | `scripts/desktop-e2e.sh brain-report` | 见下方记录 |

## 实施偏差

- ACP client 手写，没有引入 `agent-client-protocol` crate。
- 大脑命令走 `norma brain …`，不复用 `agent …`：后者要求调用方是 Agent 卡片。
- 审批代理只覆盖带手机控制的卡片，Claude 与 Codex 都支持；普通桌面卡片的权限提示无法拦截。
- GPT-Live 的派活没有改走大脑，仍由 `NormaSession` 规划；前台新增 `handoff_to_brain`。
- 审批会话目前每条 lane 串行；“多个审批并行处理”未做。

## 约束

- 每个阶段先写失败的聚焦测试，再实现。
- 事件到达、审批计时、ACP 子进程 IO 都在 `background_executor` 或后台线程，不进入 GPUI 帧循环。
- 单个 Rust 文件不超过 500 行，不使用 `#[allow(...)]`。`protocol.rs`（390 行）和 `cli.rs`（483 行）已接近上限，新命令必须放进新文件。
- 新 crate 要加入 `CLAUDE.md` 与 `AGENTS.md` 的依赖图，以及 `cargo tree` 边界检查。
- 首版审批代理只覆盖 Claude Code 卡片；其他 Agent 只汇报进度。

## 验收

- 打开语音汇报且无人说话：Codex 完成后，大脑派后续任务并播报；播报在用户开口时立即停止，结束后合并补播。
- Claude Code 申请 `cargo test`：左侧显示“Norma 判断中”，大脑放行后记入“Norma 自主处理”；等待期间用户点“我来处理”，大脑晚到的结论被拒绝。
- Claude Code 申请 `git push`：不经过大脑，直接问用户；无回应则进入“需要你处理”。
- 大脑进程崩溃或拿不准：审批转到“需要你处理”。
- 关闭语音汇报：大脑继续处理审批和派活，不播报。
- Legacy 语音、Dock 文字对话行为不变；ACP 不可用时回退到 `NormaSession`。
