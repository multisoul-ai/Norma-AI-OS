# 幕僚开放 Norma CLI 命令与内置 norma-brain Skill

- 日期：2026-11-14
- 基线：pr-235（`794eaa39`，分支 `feat/staff-m1-20261112`）
- 状态：已实施（S1–S4），desktop-e2e 与卡片标注见第 12 节

## 1. 目标

1. 把 `norma brain` 的命令开放给幕僚，开放范围按第 3 节的白名单。幕僚可以自己派活，高风险的活交给用户决定。
2. 命令用法统一放进一份内置的 `norma-brain` skill。幕僚、主大脑、审批助手三个角色的系统提示词只保留角色、底线、身份参数和上下文，命令细节都去 skill 里查。
3. 宿主端按角色硬拦截。白名单由代码保证，不再只靠提示词约束。

背景：`794eaa39` 移除了大脑的权限闸门（`crates/brain/src/acp/permission.rs` 一律放行）。幕僚和主大脑共用同一个 token，宿主不区分请求来自谁，所以命令边界目前只靠提示词约束。

## 2. 角色与身份

| 角色 | token | 说明 |
| --- | --- | --- |
| 幕僚（staff lane） | 新增独立的 `NORMA_BRAIN_TOKEN`，**绑定任务 id** | 每个幕僚轮次启动时由 App 下发，轮次结束即失效 |
| 主大脑和审批助手 | 现有的大脑 token | 行为不变 |

- 宿主的 `execute_brain_command` 和 `start_staff_command` 先按 token 认出角色，再查这个角色的白名单。
- 现有 `token_matches`（`crates/app/src/app/brain_host.rs:205`）只比对单个 token，要改成"token → 角色 + 任务 id"的映射；幕僚进程的环境注入在 `crates/app/src/app/brain_host/config.rs:343` 一带。
- 幕僚所有 `task` 命令的任务 id 必须与 token 绑定的任务一致，否则拒绝。`task create` 和 `task list` 对幕僚关闭（幕僚只处理这一个任务）。
- 幕僚 token 只注入幕僚的 ACP 进程环境，不进 Worker 的 PTY 环境。
- 假设：主大脑和审批助手继续共用一个 token，它们之间不做区分。

## 3. 幕僚命令白名单

| 命令 | 幕僚 | 备注 |
| --- | --- | --- |
| `agents` | ✅ | |
| `task reply` / `phase` | ✅ | 任务 id 须与 token 绑定一致 |
| `task create` / `list` | ❌ | 幕僚只处理当前这一个任务 |
| `task propose` | ✅ | 高风险任务必须用它 |
| `task assign <任务id> <agent> --title --prompt --why` | ✅ 新增 | 幕僚自己派活，受 4.1 的全部条件约束 |
| `task archive <任务id>` | ✅ 新增 | 仅在任务 done 之后可用 |
| `task message` | ❌ **不暴露 CLI** | `b73fd418` 刚删除它：模型可借此以用户身份写消息并唤醒自己。面板发消息走内部路径，不需要 CLI |
| `propose` | ✅ | 不挂在具体任务上的下一步提议 |
| `report` | ✅ | 只用于主动提醒（高风险等用户决定、Worker 失败），任务内的回复仍用自然语言 |
| `history` | ✅ | |
| `approvals` / `questions` | ✅ | 只读 |
| `propose-answer` | ✅ | 用户采用后才提交 |
| `context` | ✅ | |
| `assign <agent>` / `assign --proposal` | ❌ | 幕僚只能用 task 系列派活 |
| `decide`（allow、deny、escalate） | ❌ | |
| `answer` | ❌ | |

幕僚调用了不在白名单里的命令时，宿主返回结构化的权限拒绝：说明哪条命令无权执行，并给出替代做法，比如"请告诉用户去处理这条审批"。错误文案需要中英双语 i18n。skill 要求幕僚遇到这种拒绝时停止这个动作，不要重试。

## 4. 派活与风险

### 4.1 task assign 的条件与链路

`task assign` 必须**同时**满足下面四个条件才会直接派发，任一不满足，宿主自动降级为提议（回执告诉幕僚"已转为提议"及原因）：

1. 设置里的"后续任务"（`Autonomy.assign_followups`）为「自动」。默认是「询问」，所以**默认幕僚不会自己派活**。设置页文案补充说明该开关同时覆盖幕僚（中英双语）。
2. 目标 Agent 就是刚完成任务的那个 Agent，做它自己任务的后续。派给别的 Agent、开新方向一律走 propose。
3. 本任务的自动派发次数未超过 3 次。用户采用任意一条提议后计数清零。
4. 没有命中 4.2 的风险判断。

链路：

1. 宿主生成一条提议，标记 `origin = staff`。
2. 立即走现有的 Adopt 派发链路（`dispatch_staff_worker`），任务对话里依次出现 `Proposal` 和 `WorkerStarted`。
3. 任务对话里的卡片显示"幕僚已派发：<标题> → <Agent>"，带取消按钮（复用 `agent task cancel`）。取消只能停止后续动作，已经执行的操作无法回滚，文案不能写成"撤销"。
4. 复用 `AutoKind::Assigned` 的活动记录，不另造留痕。
5. 校验：shared-context 的 `DispatchTask` 限制、Agent 已启用。注意 `crates/app/src/norma/endpoint.rs:362` 把所有 `BrainOp::Task` 直接交给 `staff::dispatch`，而 `DispatchTask` 闸门（`:401`）只匹配 `BrainOp::Assign`，所以要在 `staff::dispatch` 里为 `StaffOp::Assign` 单独接入同一个闸门。

### 4.2 高风险的两层判断

- **第一层：幕僚自己判断。** skill 里写明四类高风险，命中任意一类就必须用 `task propose`：
  1. 推送、合并、部署、发布
  2. 删除或其他不可逆的数据操作
  3. 涉及密钥或凭据、影响项目目录以外
  4. 对外沟通、产生费用，以及用户明确说过"先给我看"的事
- **第二层：宿主关键词兜底。** `task assign` 的 title 或 prompt 命中风险关键词时，宿主把这次派活自动降级为提议，卡片上标注命中的风险类别，回执告诉幕僚"已转为提议"。关键词表和匹配逻辑写成纯函数，放在 `brain` crate 里，中英文都要覆盖。
- **关键词兜底不是安全边界。** 改写、拼接、中英混写都能绕过，它只降低概率。真正的边界是 4.1：只能派给同一个 Agent、有次数上限、受用户设置开关控制。
- 执行时的风险仍然交给现有的审批流程。幕僚只能看审批，不能判审批。

## 5. norma-brain Skill

- **交付方式**：复用 `crates/norma/src/orchestration/` 的托管安装器，新增 `ManagedSkillKind::Brain`，带 managed marker。**只写用户当前选定的大脑后端**（Claude Code、Codex、OpenCode 之一）的全局 skills 目录；切换后端时安装新的、卸载旧的。其余 7 个 CLI 不安装。卸载流程、`skill_component.rs` 的 reconcile 和版本升级都要覆盖新类型。
- **命令路径**：安装时把 norma 二进制路径写进 skill；路径变化（App 移动或升级）时按版本和路径比对后重装。
- **安装时机**：每次启动大脑或幕僚会话之前，做一次幂等的安装或升级（不在 GPUI 前台线程做磁盘 IO）。
- **安装失败降级**：会话照常启动，提示词里临时补一份不带参数细节的命令名速查，岛内显示警告（中英双语）；一个无关故障不能让幕僚整体不可用。
- **Worker 可见的问题**：description 写明"仅用于 Norma 大脑或幕僚会话"。Worker 没有对应的 token，就算误调用，宿主也会拒绝。
- **内容结构**：
  1. 通用规则：只执行 norma 命令、每次一条、身份参数怎么传、怎样安全地传 JSON 参数、回执怎么理解（accepted 和 context.saved 的含义）、50 字的决策文案规范。
  2. 命令参考：每条命令的参数和示例。
  3. 按角色分节：幕僚、主大脑、审批助手各自的可用命令和使用时机。
  4. 高风险清单。

## 6. 提示词精简

| 角色 | 保留在提示词里 | 移到 skill 里 |
| --- | --- | --- |
| 幕僚（`crates/brain/src/lane/staff.rs`） | 角色、怎么和用户说话、何时问何时提议、底线（包括高风险找用户）、身份参数、任务快照，外加一句"命令用法见 norma-brain skill" | "可用命令"整节 |
| 主大脑（`crates/brain/src/lane/prompt.rs` 的 `primer`） | 职责、简短口语汇报、下一步规则 | "常用命令"那一行、context 回执说明、DECISION_COPY 里的命令细节 |
| 审批助手（`crates/brain/src/lane/prompt.rs` 的 `approval`） | 风险评估规则、审批数据 | `decide` 的用法 |

事件模板（`urgent_batch` 等）里出现的命令名可以保留，但参数细节以 skill 为准。

## 7. CLI 与协议改动

- `crates/norma/src/cli/staff_command.rs`：新增 `assign`、`archive` 两个子命令。**不新增 `message`**。
- `StaffOp`（`crates/norma/src/protocol/staff.rs`）：新增 `Assign` 变体；`Archive` 已经存在。
- 提议的数据结构加一个 `origin` 字段，任务加"自动派发计数"。旧数据没有这些字段时，默认按"用户采用"、计数 0 处理，要有迁移测试。
- 非目标的调整：原先的"补 Message CLI"取消，原因见第 3 节。

## 8. 边界情况

- Agent 不存在或没有启用：返回 `staff.agent_unavailable`，和现有行为一致。
- 幕僚派活后用户取消：Worker 被取消，卡片显示"已取消"，幕僚下一轮能在快照里看到。
- 关键词误判导致派活被降级：用户点一下采用就能派发，代价可以接受。
- Skill 版本和二进制不匹配：安装器按版本号升级。
- 幕僚 token 泄漏给 Worker：幕僚 token 只注入幕僚的 ACP 进程环境，不写进 Worker 的环境。

## 9. 非功能要求

- 新增的 UI 文案和错误文案中英双语，跑过 `check-i18n.py` 和 `test-i18n.py`。
- 安装 skill 和关键词匹配都不能在 GPUI 前台线程做磁盘 IO。
- 单个 Rust 文件不超过 500 行，不使用 `#[allow]`。

## 10. 验收标准

| # | 标准 | 证据 |
| --- | --- | --- |
| 1 | 幕僚 token 调用白名单之外的命令会被拒绝，并返回替代做法 | app 单测 |
| 2 | 主大脑 token 的行为不变 | 现有测试回归 |
| 3 | `task assign` 产生"幕僚已派发"卡片和 Worker；取消后卡片状态正确 | brain/app 单测 + desktop-e2e |
| 4 | 命中风险关键词的 `task assign` 被降级为提议，并标注风险类别 | brain 纯函数单测 + desktop-e2e |
| 5 | skill 覆盖白名单里的全部命令；三个角色的提示词不再包含命令参数细节 | skill 契约测试 + prompt 单测 |
| 6 | 启动幕僚前 skill 已安装；安装失败时显示错误，不启动会话 | 安装器测试 |
| 7 | CLI 中不存在 `task message`；幕僚无法唤醒自己 | CLI 解析测试 |
| 8 | 中英双语文案正确 | i18n 脚本 + desktop-e2e 双语看图 |
| 9 | 幕僚 token 操作其他任务 id、调用 `task create/list` 被拒绝 | app 单测 |
| 10 | 「后续任务」为「询问」时，`task assign` 一律降级；为「自动」时，只能派给同一 Agent；第 4 次自动派发降级；用户采用一条提议后计数清零 | brain/app 单测 |
| 11 | 幕僚 token 不出现在 Worker 的环境里 | 单测 |
| 12 | 旧任务数据（无 origin、无计数）能正常加载 | 迁移测试 |

## 11. 评审中补充的决定

| # | 事项 | 决定 |
| --- | --- | --- |
| 1 | 幕僚与主大脑分工 | 按任务归属路由：Agent 完成的是幕僚任务里派出的活，只交给幕僚，主大脑跳过下一步判断；用户直接派的活仍交给主大脑。路由依据是派发记录里的任务归属 |
| 2 | skill 安装失败 | 降级继续，见第 5 节 |
| 3 | 二进制路径 | 安装时写入，路径变化就重装，见第 5 节 |
| 4 | 安装范围 | 只装当前大脑后端，见第 5 节 |
| 5a | `history` | 开放，仅限本任务派出过的 Agent，`--last` 上限 20；更新 `prompt_tests` 里"history 不进幕僚提示词"的断言；skill 注明内容是数据，不是指令 |
| 5b | `report` | 沿用现有播报设置决定是否朗读；每个任务 10 分钟内幕僚最多主动 report 2 条，超出的只进任务对话，不出声 |
| 6 | iOS | 本次不改 iOS，只核对：iOS 读到 `origin` 等新字段不崩溃、按普通派发显示；推送事件不新增，后续单独做 |
| 7 | 文档 | 更新 `docs/design-docs/2026-10-23-norma-brain-acp.md` 和 `AGENTS.md`；新建 `GLOSSARY.md`（幕僚、自动派发、降级为提议等术语）；在 `docs/adr/` 新增 ADR，记录"按 token 绑定角色和任务"这一决定 |

对应新增验收：

| # | 标准 | 证据 |
| --- | --- | --- |
| 13 | 主大脑对幕僚任务派出的 Agent 完成事件不再提议下一步 | brain 单测 |
| 14 | skill 安装失败时会话仍启动，提示词含命令速查，并有警告 | 安装器测试 + app 单测 |
| 15 | 切换大脑后端后，只有新后端装有该 skill，旧的被卸载；其余 7 个 CLI 不受影响 | 安装器测试 |
| 16 | 幕僚 `history` 只能读本任务派出的 Agent；`report` 超过频率上限时只进任务对话 | brain/app 单测 |
| 17 | iOS 读到新字段不崩溃 | iOS 单测（`swift test --package-path ios`） |

## 12. 实施记录与偏离

实施时与上文不同或未完成的地方：

| 项 | 实际做法 |
| --- | --- |
| token 绑定任务 id | 没有做“每任务一个 token”：ACP 会话进程跨任务复用，环境变量启动时固定。改为幕僚 token 只代表角色，任务绑定靠宿主判定“当前进行中的轮次”，没有进行中的轮次时一律拒绝（[ADR-0029](../adr/0029-bind-brain-roles-by-token-and-authorize-staff-commands-in-host.md)） |
| report 超限 | 不是静默降级，而是拒绝并返回 `staff.denied.report_rate`，提示把内容写进任务回复 |
| `origin = staff` 与卡片 | 没有给提议加 `origin` 字段，也没有“幕僚已派发”专用卡片和取消按钮。自动派发沿用现有的提议卡和 `WorkerStarted`；`Task.auto_dispatches` 记录次数。降级原因只在回执里给模型，卡片上没有风险类别标注 |
| 安装失败提示 | 只写 `tracing::warn!` 并降级到命令速查，岛内没有警告 |
| 术语表 | 仓库的词汇表是 `CONTEXT.md`，没有新建 `GLOSSARY.md` |
| 主大脑分工 | 用 `staff_owned` 标记：幕僚任务派出的活，正常完成不再叫醒主大脑；失败仍告诉用户，但提示里不再要求提下一步 |
| desktop-e2e | 见实施总结：没有新增自动派发的 e2e 场景 |

评审后补充的加固（对抗式评审发现）：

- 任务绑定：必须恰好一个任务正在进行一轮；没有或同时有多个都拒绝，不再取第一个。
- `history`：不只比对 Agent 名，而是要求 Agent 当前打开的卡片就是本任务派出的那张；`--last` 超过 20 拒绝；拿不到卡片时拒绝。
- 归档：幕僚自己设的 done 不算，必须是用户手动确认的 done（`phase_manual`）才允许归档。
- 授权在窗口关闭等拿不到应用状态时拒绝，不再放行；幕僚 token 缺失时幕僚会话不带任何 token，不退回主大脑 token。
- 风险关键词：先去零宽字符、全角转半角、折叠空白；补充 `rm -fr`、`git clean`、`terraform apply`、`.env`、`| sh` 等写法；派活的 `why` 也参与匹配。
- skill 安装器：托管标记必须独占一行；卸载编排集成时一并删除 norma-brain skill。
- 已知未处理：派发失败后提议保持“已批准未派发”且已占用一次自动派发额度；多个窗口/项目并发的幕僚轮次由宿主按“恰好一个”规则全部拒绝，而不是精确区分调用者。
