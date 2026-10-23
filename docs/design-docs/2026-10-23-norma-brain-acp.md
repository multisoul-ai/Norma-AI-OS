---
title: Norma 大脑：ACP 后台决策与语音汇报
subtitle: 前台对话保持快速；后台由用户已安装的 Claude Code / Codex 通过 ACP 整理进度、审批和派活
template: doc
theme: paper
lang: zh
date: 2026-10-23
status: 方案已确认 · 未实施 · 两项实测待完成
---

新增“语音汇报模式”：用户不需要说话，Norma 也会实时汇报所有 Agent 的进度。所有 Agent 卡片通过 hook 把事件汇报给 Norma。后台大脑通过 ACP 调用用户已安装的 Claude Code、Codex 等编码 Agent，自行整理信息和做决策，只有高风险操作需要用户确认。前台普通对话仍由实时语音模型或 `NormaSession` 负责，速度不受影响。

可交互原型：[`docs/prototypes/norma-brain/index.html`](../prototypes/norma-brain/index.html)。

## A 已确认的决策

| 维度 | 决定 |
| --- | --- |
| 播报节奏 | 事件驱动：完成、出错、需要审批时立即播报；另有定时摘要。没有变化时不说话 |
| 两个大脑分工 | 前台对话：实时语音模型或 `NormaSession` 快速回答。后台工作：ACP 大脑处理事件、审批、派后续任务和需要读代码的问题。两者共用记忆和目标摘要 |
| `NormaSession` | 保留。负责 Legacy 语音、Dock 文字对话，以及 ACP 不可用时的兜底。GPT-Live 的派活改由 ACP 大脑处理 |
| 后端选择 | 全局一个大脑。可选后端、主会话模型、审批会话模型，以及策略（速度优先、自动、质量优先） |
| 会话形态 | 常驻主会话（强模型），加上审批会话（快模型）；两者通过目标摘要保持口径一致 |
| 能力接入 | 复用 `norma` CLI 和托管 skill。Norma 作为 ACP client 处理每次 `request_permission`：放行 `norma` 命令和工作空间内只读文件访问，拒绝其他请求 |
| 大脑输入 | 主动推送 hook 事件和每轮最终输出的摘录；按需拉取卡片上下文和相关文件、文档 |
| 自主权 | 大脑可批准只读、测试、构建和项目内写文件。它也可以给空闲 Agent 派后续任务，或停止、重启卡住的 Agent |
| 高风险 | push 或合并、删除、写项目外路径、部署发布、涉及密钥、付费操作。这些操作在大脑判断前由规则拦截，直接问用户 |
| 审批判断 | 未命中高风险规则的审批全部交给大脑，不设固定时限。判断中的审批在概览中可见，用户随时可以接管 |
| 转给用户 | 命中高风险、大脑拿不准、大脑出错或崩溃、等待接近 hook 120 秒上限。用户没有回应时，事项进入“需要你处理” |
| 关闭语音汇报 | 大脑继续处理审批和派活，只是不说话 |
| 发声与冲突 | 跟随当前语音模式。用户开口时立即停止播报；对话结束后，积压的播报合并成一句再播；高风险确认排在最前 |
| 自主处理记录 | 每条记录提供“为什么”“以后这类先问我”“停止该 Agent”。已执行的操作无法撤销，所以不提供“撤回” |
| 首版范围 | 只做桌面端，iOS 不在本期。审批代理只覆盖 Claude Code 卡片；其他 Agent 只汇报进度 |

## B 分层架构

```flow LR
group 感知: Agent 卡片, hook 事件收集
group 前台对话: 实时语音模型, NormaSession
group 后台 ACP 大脑: 事件分流, 主会话, 审批会话
group 执行: norma 命令, 安全闸门
group 表达: 播报队列, 当前语音模式
Agent 卡片 -> hook 事件收集: 事件 / 审批
hook 事件收集 -> 事件分流
事件分流 -> 主会话: 进度、前台转来的任务
事件分流 -> 审批会话: 审批
(用户) -> 当前语音模式: 说话 / 打字
当前语音模式 -> 实时语音模型: 豆包 / GPT-Live
当前语音模式 -> NormaSession: Legacy / Dock 文字
实时语音模型 -> 事件分流: 派活或深度判断
NormaSession -> 事件分流: 派活或深度判断
主会话 -> norma 命令
审批会话 -> norma 命令
norma 命令 -> 安全闸门
安全闸门 -> Agent 卡片: 派活 / 停止 / 放行或拒绝
安全闸门 -> *需要你处理: 高风险 / 拿不准 / 出错
安全闸门 -> 播报队列: 播报文本
播报队列 -> 当前语音模式
```

| 层 | 职责 |
| --- | --- |
| 感知 | 把各 Agent 的状态、最终输出和审批请求收进 Norma |
| 前台对话 | 1–2 秒内回答用户；需要派活或读代码时先回一句，再把任务转给后台 |
| 后台 ACP 大脑 | 主会话整理进度、派后续、读代码回答；审批会话只判断审批 |
| 执行 | 大脑的一切动作都经过 `norma` 命令；安全闸门执行高风险规则 |
| 表达 | 播报排序、合并、用户开口时让路，再交给当前语音模式 |

## C 模块拆分

```tree
crates/
  + brain/ | 新 crate，不依赖 GPUI、Storage
    + event.rs | BrainEvent：统一事件模型
    + router.rs | 分流、去抖、定时摘要
    + lanes/main.rs | 主会话
    + lanes/approval.rs | 审批会话
    + acp/client.rs | JSON-RPC stdio、会话生命周期、重连
    + acp/gate.rs | ACP 权限白名单：norma 命令与只读文件
    + backend.rs | 检测已安装的 CLI 与 ACP 适配器，按策略选择模型
    + goal.rs | 目标摘要、决策日志
    + risk.rs | 高风险规则，纯函数
  ~ norma/
    ~ protocol/ | 新增 Approval、Stop、Report、Tail、Goal 命令
    ~ cli.rs | 新增对应子命令
    ~ agent_hook/approval_forward.rs | 后续把权限桥扩展到 Claude 以外
  ~ agent/
    ~ master.rs | NormaSession 保留；新增“转给后台”工具；与 brain 共用记忆
  ~ app/src/
    + app/brain_host.rs | 在后台执行器上运行 brain，桥接 GPUI
    + app/approval_broker.rs | 挂起的审批、用户接管、接近 120 秒上限时转人工
    + app/high_risk_guard.rs | 调用 brain::risk 做最终裁决
    ~ app/gpt_live/dispatch.rs | 派活改为交给 ACP 大脑
    ~ app/agent_announcements/ | 优先级、外部文本入队、合并、让路
    ~ workspace/desktop/overview_rows.rs | 新增“Norma 判断中”和“Norma 请你确认”
    + settings/brain/ | 大脑设置页
```

`brain` 不调用 App 内部函数。它通过 ACP 接收事件，通过 `norma` CLI 执行动作，所以可以脱离 GPUI 独立测试。

## D 审批流程

```flow
(Agent 申请执行操作) -> {命中高风险规则?}
{命中高风险规则?} -> *交给你: 是
{命中高风险规则?} -> Norma 判断中: 否
Norma 判断中 -> {结果?}
{结果?} -> (放行或拒绝): 大脑执行了 approval decide
{结果?} -> *交给你: 大脑拿不准
{结果?} -> *交给你: 大脑出错或崩溃
{结果?} -> *交给你: 接近 hook 120 秒上限
Norma 判断中 --> 交给你: 用户点“我来处理”
交给你 -> (放行或拒绝): 用户决定
```

- 高风险规则对终端命令先按 shell 语法拆分，再逐段比对 argv 前缀。写文件时规范化路径，再判断是否在项目内。不在白名单内的工具视为高风险。无法解析的命令也视为高风险。
- 只有大脑执行了 `norma approval decide <id> allow|deny|escalate`，才算有结论；大脑在回复里写“可以放行”不算。
- 复用 `crates/norma/src/agent_hook/approval.rs` 的状态机：先到的结论生效。用户接管后，大脑再执行 `decide` 会失败。
- 大脑只能把审批升级给用户，不能把命中高风险规则的操作降级放行。

## E 对话与派活流程

```sequence
participants: 用户, 前台对话, 播报队列, 主会话, Agent
用户 -> 前台对话: 现在几个在跑？顺便让 Codex 补文档
前台对话 -> 播报队列: 暂停播报
前台对话 --> 用户: 3 个在运行，补文档交给后台
前台对话 -> 主会话: 转交派活任务
主会话 -> 主会话: 读卡片上下文和 docs/
主会话 -> Agent: norma agent task assign
主会话 -> 播报队列: norma report
播报队列 --> 用户: 已经交给 Codex
```

## F 与现有代码的差距

| 差距 | 证据 | 处理 |
| --- | --- | --- |
| 权限桥只支持 Claude | `crates/norma/src/agent_hook/approval.rs` 的 `HookApproval::new` 只接受 Claude | 首版只代理 Claude Code 卡片的审批 |
| CLI 没有审批、停止、播报命令 | `crates/norma/src/cli.rs` | protocol 与 CLI 同步新增 |
| 播报队列只处理终态事件 | `crates/app/src/app/agent_announcements/queue.rs` | 增加优先级、外部文本入队和让路 |
| GPT-Live 派活依赖 NormaSession | `crates/app/src/app/gpt_live/dispatch.rs` | 改为转交 ACP 大脑 |
| 概览没有“判断中”和“请你确认” | `RunOverviewAttentionKind` | 新增两种类型，复用现有行和跳转 |

## G 待完成

- 实测：Claude Code 通过 ACP 回答一句话的首字延迟，用来确认前台对话不能交给 ACP 大脑。
- 实测：hook 等满 120 秒后，Claude Code 会不会退回终端里的权限提示。如果会退回，就不需要在接近上限时提前转人工。
- 待定：“Norma 自主处理”记录保留多久，是否需要每天汇总一次。
- 风险：每次审批都经过大脑，高峰期需要并行处理。设置页需要显示两个常驻会话的用量。
