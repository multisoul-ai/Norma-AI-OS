---
title: Norma 大脑：ACP 后台决策与语音汇报
subtitle: 前台对话保持快速；后台由用户已安装的 Claude Code / Codex 通过 ACP 整理进度、审批和派活
template: doc
theme: paper
lang: zh
date: 2026-10-23
status: 桌面端已实施 · 真实 Claude Code 验收通过 · 见 H 节的限制
---

## 2026-10-24 更新：以下几处以本节为准

**运行边界（2026-10-25 更新）**：Claude Code、Codex、OpenCode 均可作为幕僚后端，不再在服务层或设置页拒绝启动。已知并被用户接受的缺口：Codex 的 read-only 模式只限制写入，其读命令不经 Norma 闸门，可读项目目录之外的文件；OpenCode 仅注入 `bash:ask / edit:deny / webfetch:deny`，其余工具默认放行。只有 Claude Code 的逐条读取审批是完整的。这不影响 Claude／Codex 智能体卡片本身。

Claude 会话隔离用户设置、禁止 bypass，仅暴露 `Bash` 和逐文件 `Read`，并用显式 `permissions.ask` 让两者每次都发 ACP 权限请求。闸门拒绝组合、重定向、整条命令引号二次拆包与持久授权；读取检查真实路径，仅放行项目内存在的普通文件，拒绝已知凭据路径和递归目录工具。这个边界依赖适配器遵守配置，不是操作系统沙箱，也无法识别任意名称的秘密或消除检查与读取之间的文件变化。

目标、提议、审批和决策日志改为按稳定工作空间 ID 隔离，保存在 `~/.norma/brain/projects/<workspace-id-hash>/`；同一目录的不同画布不会共享提议。旧提议文件只原子迁移给一个画布。设置和提议通过进程共享队列有序写入，快照原子替换；关闭后重开先等待旧写入完成。JSON 语法或任一条目损坏时保留整份原件并重试；提议保存失败向工作空间报错，保留最新内存快照重试。派发失败回执保留原提议，只有实际接收／排队后才记成功。文件读取、写入、适配器扫描与进程启动均在后台。磁盘故障期间强制退出仍可能丢失待保存状态，PTY 派发与文件保存不构成跨进程事务。

```callout warn 高风险规则已删除
正文里“命中高风险规则的审批由规则拦截”和“大脑不能把高风险操作降级放行”已不再成立。现在每条审批都交给大脑判断风险。提示要求它评估三件事：是否可逆、是否影响项目之外、是否涉及密钥。它还要识别命令里的诱导文字。

保留的保护有这几项：
- “始终先问你”清单。
- “自行决定”里的三项开关。
- 大脑拿不准、出错或接近 120 秒时转人工。
- 大脑自己只能执行 `norma brain …`。
没有兜底：`git push`、删除、读密钥都可能被大脑放行。模型被命令里的诱导文字说服的风险没有实测。
```

同日新增的能力：
- **大脑活动窗口**：⌘⇧B 弹出，显示大脑与 ACP 适配器的全部往来（输入、思考、工具调用与结果、权限判断、输出），只放内存。
- **`norma brain history <agent>`**：大脑按需读取 Agent 的完整对话。只支持带手机控制的 Claude 与 Codex 卡片。
- **目标**：用户发给 Agent 的任务记为目标，保留最近 5 条，随主会话提示一起发给大脑；大脑自己派的任务不算。
- **汇报降噪**：失败和结果里带“需要你”等词的立即汇报；普通成功先进静默池，攒 30 秒再让大脑扫一遍，默认不出声。

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
| 能力接入 | 新增 `norma brain` 子命令（report、approvals、decide、agents、assign），用 App 启动大脑时下发的一次性令牌鉴权。Norma 作为 ACP client 处理每次 `request_permission`：只放行单条 `norma brain` 命令和工作空间内只读文件访问，拒绝其他请求 |
| 大脑输入 | 主动推送 hook 事件和每轮最终输出的摘录；按需拉取卡片上下文和相关文件、文档 |
| 自主权 | 大脑可批准只读、测试、构建和项目内写文件。它也可以给空闲 Agent 派后续任务，或停止、重启卡住的 Agent |
| 高风险 | push 或合并、删除、写项目外路径、部署发布、涉及密钥、付费操作。这些操作在大脑判断前由规则拦截，直接问用户 |
| 审批判断 | 未命中高风险规则的审批全部交给大脑，不设固定时限。判断中的审批在概览中可见，用户随时可以接管 |
| 转给用户 | 命中高风险、大脑拿不准、大脑出错或崩溃、等待接近 hook 120 秒上限。用户没有回应时，事项进入“需要你处理” |
| 关闭语音汇报 | 大脑继续处理审批和派活，只是不说话 |
| 发声与冲突 | 跟随当前语音模式。用户开口时立即停止播报；对话结束后，积压的播报合并成一句再播；高风险确认排在最前 |
| 自主处理记录 | 每条记录提供“为什么”“以后这类先问我”“停止该 Agent”。已执行的操作无法撤销，所以不提供“撤回” |
| 首版范围 | 只做桌面端，iOS 不在本期。审批代理只覆盖带手机控制的卡片（Claude Code 和 Codex）；普通桌面卡片的权限提示仍在终端里，Norma 无法拦截，这些 Agent 只汇报进度 |

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
  + brain/ | 新 crate，不依赖 GPUI、Storage、Runtime、Agent
    + event.rs | BrainEvent（输入）、BrainStatus（输出）
    + router.rs | 去抖、定时摘要、高风险直通人工
    + service.rs | BrainService：分发线程加两条 lane
    + lane/prompt.rs | 把事件渲染成发给大脑的文字
    + lane/worker.rs | 一条 lane：串行处理、会话失败退避重建
    + acp/wire.rs | 按行分隔的 JSON-RPC over stdio
    + acp/client.rs | 握手、建会话、发 prompt、处理权限请求
    + acp/gate.rs | 权限闸门：单条 norma brain 命令与项目内只读
    + backend.rs | 检测已装后端；策略到模型；强制 settingSources 为空
    + goal.rs | 目标摘要与决策日志
    + risk.rs、risk/ | 高风险规则（shell 拆分、命令、路径），纯函数
  ~ norma/
    + protocol/brain.rs | BrainOp、BrainDecision、ReportPriority
    + cli/brain_command.rs | norma brain report、approvals、decide、agents、assign
    ~ client.rs | 为大脑进程带 Workspace 身份
  ~ agent/
    ~ adapter、rig_tools、master_prompt | 新增 handoff_to_brain 工具，前台把请求转给后台
  ~ app/src/
    + app/brain_host.rs、brain_host/ | 配置、启动、事件转发、审批轮询与决定、设置读写
    + app/agent_announcements/report.rs | 大脑的播报进入既有队列与 TTS
    + workspace/desktop/overview_judging.rs | 概览里的“Norma 判断中”
    + settings/brain.rs | 设置页
    ~ norma/server.rs | 把 CardCommand::Brain 交给项目窗口
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
- 只有大脑执行了 `norma brain decide <id> allow|deny|escalate --reason …`，才算有结论；大脑在回复里写“可以放行”不算。
- 复用 `runtime` 里手机审批已有的状态机（`decide_mobile_approval`、`decide_mobile_codex_approval`）：先到的结论生效。用户接管后，大脑再执行 `decide` 会得到 `approval_closed`。
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

## F 概览里的大脑

- **允许与拒绝按钮**：带手机控制的卡片，其审批行右侧有两个按钮。点击直接在该卡片所在的项目里处理，不经过大脑，也不要求大脑在运行。点击只处理审批，不触发整行的“定位”。
- **“Norma 判断中”**：大脑正在判断的审批在“需要你处理”里换成这个标签，仍可“定位”或直接允许、拒绝。
- **“Norma 自主处理”栏**：启用大脑后出现在“待开始”之后。每条记录是大脑自行放行、拒绝或派发的事项，带智能体名和时间。点“为什么”展开大脑给的理由，展开后有两个操作：
  - “以后这类先问我”：命令类按程序名（如 `Bash:cargo`）、写文件类统一为 `Write`，加入配置里的 `always_ask`，之后同类审批不再由大脑放行；派任务这一类则关掉“给空闲的 Agent 派后续任务”。
  - “停止该 Agent”：向该卡片发 Ctrl-C，与取消任务时的做法一致。
- **设置在各项目间同步**：只有工作空间窗口保存配置，并把新设置推送给各项目会话；会话里的大脑状态每 0.5 秒汇总一次给概览。

## F2 与现有代码的差距（已处理）

| 差距 | 处理 |
| --- | --- |
| CLI 没有审批、停止、播报命令 | 新增 `norma brain` 子命令和 `CardCommand::Brain`，见 `crates/norma/src/cli/brain_command.rs` |
| 播报队列只处理终态事件 | `agent_announcements/report.rs` 把大脑的文字放进同一队列：优先级 `now` 排最前，Legacy 路径整段合成，豆包路径走常驻通道 |
| GPT-Live 派活依赖 NormaSession | 未改。GPT-Live 仍由 `NormaSession` 规划；前台对话新增 `handoff_to_brain` 工具，把需要读代码或派后续的请求转给后台 |
| 概览没有“判断中” | 在“需要你处理”里把大脑正在判断的审批标为“Norma 判断中”，仍可点“定位”接管，见 `overview_judging.rs` |

## G 实施记录与已知限制

**启用步骤**：本机已安装并登录 Claude Code。打开设置，进入 Norma 下的“Norma 大脑”。点“安装 Claude Code 适配器”，装好后选择后端和策略，再打开“启用后台大脑”。

启用后，标题栏右侧出现“语音汇报”开关。打开时，大脑的汇报用当前语音模式念出来。关闭时，大脑照常工作，只是不说话。它与语音设置里的“智能体任务播报”互不影响。

- **ACP client 手写**：只用到 `initialize`、`session/new`、`session/set_mode`、`session/prompt`、`session/request_permission`，所以没有引入 `agent-client-protocol` crate，权限处理完全自己掌控。
- **适配器**：Claude Code 用 `@agentclientprotocol/claude-agent-acp@0.94.0`，设置页的“安装”按钮会装到 `~/.norma/brain/adapters`；Codex 和 OpenCode 已放开启动，读取隔离缺口见上文运行边界。norma 路径含空格时，大脑使用 `~/.norma/brain/bin/norma` 符号链接，避免 Codex 的命令包装让闸门无法还原路径。
- **权限请求**：仅切到 `default` 并不足以让安全读取逐次询问；还必须隔离用户设置、禁用 bypass、限制工具列表并显式配置 `permissions.ask`。历史探测见 `docs/research/2026-10-23-acp-adapters-probe.md`。
- **审批代理范围**：只有带手机控制的卡片有可代理的审批。普通桌面卡片的权限提示留在终端里，Norma 无法拦截。要覆盖它们，得给所有卡片装阻塞式权限 hook，这是单独的一项工作。
- **高风险规则是启发式，不是沙箱**：它只看命令文字。
  - 能挡住：push、删除、部署、密钥、提权、项目外写入、解释器内联代码，以及 `env -i rm`、`GIT push`、`git -c alias…` 这类绕写法。
  - 挡不住：`npm install`、`make`、`python script.py` 这类执行项目内代码的命令，它们算普通操作。
  - 大脑只能放行规则判为普通的审批；审批里的命令和路径始终当作数据，不当作指令。
- **每个项目一个大脑**：每个打开的项目窗口各启动一个大脑（两条 lane，各一个适配器进程）。同时打开很多项目会占用更多内存和额度。
- **设置页**：设置 → Norma → Norma 大脑，按原型排布。
  - 选项：启用、后端卡片、策略、两个会话的模型、定时摘要、自行决定、始终先问你。
  - 另有：判断较慢时的提醒、并行审批、审批范围。
  - 自行决定里有三项生效：只读命令、项目内写文件、派后续任务。“停止或重启卡住的 Agent”没有提供，灰显。
  - 配置保存在 `~/.norma/brain/config.json`，`NORMA_BRAIN_DIR` 可改位置。除“语音汇报”外，修改后都会重启大脑。
- **记录**：大脑的回复写入 `decisions.jsonl`，放行或拒绝的审批写入 `approvals.jsonl`，目标摘要在 `goal.md`，提议在 `proposals.json`；均位于上文按工作空间隔离的目录。
- **真实 App 验收**：`scripts/desktop-e2e.sh brain-report`。假 `claude` 完成任务后，真实 Claude Code 适配器收到事件，通过 `norma brain report` 把汇报交回 App。隔离数据目录里没有 TTS，所以日志显示“TTS 不可用”而没有真正发声。

## H 待完成

- 已测：Claude Code 通过 ACP 回答一个词的首字延迟约 3 秒，带一次工具调用约 5 到 8 秒，不适合前台对话。
- 未测：hook 等满 120 秒后，Claude Code 会不会退回终端里的权限提示。
- 未做：多个审批同时到达时审批会话并行处理（现在每条 lane 串行）；定时摘要提醒的语音选项；Codex 和 OpenCode 的权限请求字段实测。
- 待定：“Norma 自主处理”记录保留多久，是否需要每天汇总一次。
- 风险：每次审批都经过大脑，高峰期需要并行处理。设置页需要显示两个常驻会话的用量。
