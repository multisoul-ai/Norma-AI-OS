# Super-Agent 工作流程与 Norma 关系调研

日期：2026-11-04。范围：DeepSeek 官方仓库的一手文档与源码，以及 Norma 当前代码。本文是调研记录，不是已经实施的产品规格，也没有修改正式 App。

上游基准为 GitHub `commits/main` API 当次返回的 `8c32a9dc5d72c033816c6a8110b37f40cf94fba4`。下列上游链接均固定到该提交；本地基准为 `22c9526d`。用户提供的微信公众号页面触发环境验证，未读取到正文，本文不声称逐段读过该文章。上游 README 也明确说它是研究原型，和内部版本、原文章描述并不完全相同。[S0]

## 最重要的结论

1. **事实：用户管理的是工作，不是会话。** Ticket 是一件有授权范围、来源、输入、交付与生命周期的工作。Worker 是执行者；Session 是执行者保留上下文的会话。一次 Session 回合结束，不代表工作完成。[S2][S3]
2. **事实：DeepSeek 主 Session 只做路由，不制定技术实现方案。** 它转发原始请求与路由事实；Executor 读原始资料、判断任务范围、实施并验证。主 Session 也不长时间等待 Worker，后台消息自动到达。[S4][S6]
3. **事实：正常进度不会层层打扰人。** 源码中的 `reportRelay` 在 `progress` 且没有 `answers` 时直接返回 `undefined`；具体问题、可用结果、阻塞等才送到主 Session。它不是把所有 Worker 的聊天逐条搬进总对话。[S7]
4. **事实：等待人的清单有可检查的原因。** 上游区分输入未送达、待回答、待审阅、受阻/失败，另有排队、工作中、等事件。不能把“暂时没输出”推断成“需要人”。[S2][S3]
5. **产品建议：Norma 的页面应让用户给目标、做取舍、验成果。** 原画布保留为可进入的执行现场；大对话页负责统一沟通；旁侧只放需要用户动作的事项。技术推荐方案由掌握上下文的 Worker 提供依据，再由 Norma 汇总。这个页面划分是针对 Norma 的设计建议，不是上游现成 UI。[S1][S5]

## 用饭店来理解角色

| 比喻 | 在系统里是什么 | 负责什么 |
| --- | --- | --- |
| 老板 | 用户 | 定目标、说限制、做关键取舍、确认成果 |
| 店长 | 产品层面的 Norma / Super-Agent | 接住沟通、把请求交给合适执行者、记住工作归属、带来需要老板决定的事项 |
| 订单 | Work；上游叫 Ticket | 记录到底要做什么、为什么能做、做到什么程度 |
| 厨师 | Worker；Norma 画布中的 Coding Agent | 决定做法、实际执行、自检、报告结果或具体问题 |
| 厨师的工作笔记 | Session | 保存这件工作执行时的上下文 |
| 一次派工 | Assignment | 记录这次由哪个执行实例负责，何时启动和结束 |
| 开放厨房 | Norma 画布 | 能直接看现场、操作卡片、接管执行 |
| 老板桌上的决定单 | 人类 TODO / Attention | 只有老板的回答、批准或验收能推进的事项 |

这是解释用的比喻，不把饭店术语加入产品领域模型。尤其“店长”不是上游 Router 的技术职责扩张：上游主 Session 不替厨师写做菜方案。[S2][S4]

## 工作、执行者、会话与卡片不能混为一谈

**事实：领域模型将 Ticket 和 Executor 解耦。** Ticket 自己定义状态，不跟着 DSH Session 变化；同一 Ticket 随时间可以有多个 Assignment。当前 DSH Executor 的具体实现为每个 Ticket 创建一个命名为 `worker-<ticket id>` 的 Session 和一个目录，重试沿用该 Session、目录、分支和 PR，并更换 Assignment。[S2][S3]

```text
一件工作 Ticket
  ├─ 授权范围、原始请求、后续输入、交付依据
  ├─ 第一次派工 Assignment → Executor 实例
  ├─ 重试后的派工 Assignment → 后续执行实例
  └─ 当前执行现场 → 会话 / 目录 / 分支 / PR

Norma 的画布卡片 = 进入执行现场的入口
Norma 的工作条目 = 这件事情是否真的办成的记录
```

**推断：** 用户更换模型、重开 Session 或移动卡片时，应继续认出“还是同一件事”。不能仅把卡片名或屏幕坐标当作工作身份。将这个独立工作身份与现有 Norma Runtime 卡片建立稳定关联，还需要产品与域模型设计；本次没有证明 Norma 已有与上游相同的 Ticket 层。

## 一件工作怎样推进

| 步骤 | 上游实际机制 | 对人有什么价值 |
| --- | --- | --- |
| 1. 收到事情 | Source 接入 GitHub、可选飞书；产生不可变 Trigger，归入一个或多个 Thread | 留住原始请求与来源，不靠转述记忆 |
| 2. 找到已有归属 | 订阅的 Thread 将 Trigger 投递到相关 Ticket；已有负责人处理范围内后续消息 | 用户不用重新说明背景或再次点名 |
| 3. 判断是否新开工 | 没有现有执行者负责的请求进入 Eval；接单后生成 Ticket 与不可变 Mandate | 避免同一请求重复开工；授权范围可核对 |
| 4. 开始执行 | Assignment 把工作交给 Executor，准备目录并启动 Worker Session | 每件工作保留独立上下文，互不挤占 |
| 5. 自行推进 | Worker 读来源、实施、验证；耗时任务可后台执行 | 接到新消息时不用等前一个长命令结束 |
| 6. 普通进度留后台 | `progress` 无回答时只留 Ticket 内部 | 老板不必旁听每一步 |
| 7. 真正需要人时提问 | Worker 报告一个具体 `human_action` 问题，主 Session 原样转达 | 问题有对象、有动作，不埋在长报告里 |
| 8. 人的回答回原工作 | 回复与准确 Ticket / Thread 关联，由原 Worker 处理；超出范围才交还路由 | 不让老板扮演人工消息转发器 |
| 9. 成果就绪后审阅 | `ready` 可能进入 Awaiting review；有 PR 不等于工作已完成 | 让交付依据和验收成为完成标准 |
| 10. 结束后仍可追溯 | 相关 PR 关闭/合并且没有未完成续作后结束；人的后续消息可在授权有效且非人为停止时续接 | 完成后补一个问题，不必从零讲起 |

依据：[S2][S3][S4][S7]。上游并不要求每条后续消息都再经过主 Session：订阅可以直接送给负责的 Worker。与工作仅有“话题相近”不足以自动附着，官方要求明确的 PR、Issue、源消息、确认消息等引用；上下文关联也不等于新授权。[S2][S4]

## “做完一轮”与“办成一件事”

| 观察到的现象 | 能否直接算工作完成 | 原因 |
| --- | --- | --- |
| Worker 本轮不再输出 | 不能 | 可能仍有后台工作；也可能只是等新事件 |
| Shell 后台任务仍在执行 | 不能 | Ticket 继续占 Worker slot，保持 working |
| Worker 报告 waiting | 不能 | 当前回合结束，订阅事件仍可唤醒 |
| 提交了草稿 PR | 不能 | 开 PR 不等于 ready，更不等于合并 |
| Worker 报告 ready，且没有 PR | 上游可以完成 | 官方实现直接结束 Ticket 为 completed |
| Worker 报告 ready，且仍有相关开放 PR | 不能 | 等审阅，属于 person/review |
| 相关 PR 已关闭或合并 | 有条件可以 | 仍需检查恢复、重试或跟进工作是否未完成；结束结果也要区分 merged 和 closed |

依据：[S3]。**产品建议：** Norma 应展示“待你验收”“已交付、等待合并”“已合并”等可解释阶段，不用一个“100%”掩盖这些差别。没有可靠步骤数据时展示当前阶段与最后有效进展，不编造完成百分比。

## 人类 TODO 与推荐方案

**事实：** 上游面板先问“现在是否需要我”，再问系统是否正常，最后才查历史。数字只有在改变用户动作时才出现；健康状态不额外占空间。[S1][S5]

**产品建议：** 右侧 TODO 以工作为单位，主要包括三类：

- **要你回答：** 缺少关键业务选择或资源，Worker 无法自行确定。
- **要你批准：** 当前授权不覆盖的操作，说明影响范围后由用户决定；沿用 Norma 原有权限边界。
- **要你验收：** 结果已经可使用或可审阅，提供成果入口和验证证据。

普通“正在写代码”“已读 5 个文件”“Agent 没有输出”不进入人类 TODO。可选的下一步建议与真正阻塞当前工作的事项要区分，避免建议越多、用户负担越重。

每个决定单建议包含：**要决定的事 → 推荐动作 → 关键依据 → 不选它的影响 / 备选 → 对应工作与现场入口**。默认短到能做决定，证据和原始内容按需展开。点“采用方案”后应显示送达、执行与完成的不同状态；旧问题已解决或版本变化时不得重复提交。

**推荐是谁准备的？**

- **上游事实：** Executor 判断范围、实施、验证，主 Session 只路由；问题通过 Worker 的结构化报告提出。[S4][S6]
- **针对 Norma 的建议：** 技术候选、验证材料与不确定性应由拥有上下文的 Worker 准备。Norma 对用户负责，整合这些材料并表达推荐；跨工作的事实可由它协调。若另需深入方案比较，可交给专门的规划或调查 Worker。
- **边界：** 上游并没有实现“由 Router 统一生成所有技术方案”的产品机制；Norma 当前后台大脑已经会读 Agent 对话并生成回答建议，不能把上游角色约束误写成 Norma 当前代码的事实。

## 暂停、停止、重试与重启

| 操作或情况 | 上游事实 | 设计注意 |
| --- | --- | --- |
| Pause processing | 停止派发、新评估、评估超时、输入投递与回复；Source 继续记事件，已经执行中的回合不被中断 | 不能把它显示成“所有 Agent 已停止” |
| 人明确暂停工作 | 提示词要求暂停跨后续消息、新 Session 与重启持续有效，自动反馈不得恢复 | 用户明确取消暂停才继续；区分提示词要求与宿主强制机制 |
| 人停止 Ticket | 撤回 Mandate；被人停止的 Ticket 不再收订阅消息 | 停止不是普通 idle，不应被后台事件悄悄重启 |
| 临时网络 / Provider 故障 | 对规定的暂时性失败先进行有上限退避重试 | 不把每次暂时失败都交给人 |
| blocked / failed | 自动输入被留存但不继续唤醒；人可直接在 Session 中接管 | 不等于工具全部锁死 |
| Retry | 在送达失败、受阻、失败等可重试阶段创建新 Assignment；保留原会话、目录、分支、PR | 让用户认出仍在修同一件事 |
| Host restart | 忙碌派工用恢复说明续接；会话、目录、订阅、待处理输入与已读记录持久保留 | 核对外部真实状态后续接，避免重复对外执行 |

依据：[S2][S3][S4]。上游 Executor 的 OS 权限和免逐命令审批是该原型的部署选择，不是设计这个页面就获得的授权，也不应移植成 Norma 的默认执行权限。[S3]

## Norma 当前接点与差距

以下是当前代码事实，不能被新界面文案改写为更强能力。

| 当前能力 | 代码依据 | 对新设计的含义 |
| --- | --- | --- |
| 后台大脑整理进度、回答转交问题、提议后续 | `crates/brain/src/lane/prompt.rs:128` | 已有 Super-Agent 的部分协调能力，页面应呈现现有职责 |
| 正常完成默认静默；异常、风险、需要决定才汇报 | `crates/brain/src/lane/prompt.rs:31`、`crates/brain/src/lane/prompt.rs:39` | 可以用摘要代替日志洪流 |
| 下一步提议需用户确认，且提议给原来刚完成的 Agent | `crates/brain/src/lane/prompt.rs:10` | 不能在原型中暗示任意换人或自动派发已实现 |
| 提议按工作空间保存，同 Agent 只留最新一条，总数最多 5 条 | `crates/app/src/app/brain_host/proposals.rs:1`、`crates/app/src/app/brain_host/proposals.rs:75` | 这是 Agent 级后续提议，尚不等于完整工作单模型 |
| 点派发时检查目标是否存在、启用、忙碌、待审批或待回答 | `crates/app/src/app/brain_host/proposal_actions.rs:161` | 不把待确认当作空闲；状态变化后操作必须复核 |
| 派发失败保留提议，成功后才移除 | `crates/app/src/app/brain_host/proposal_actions.rs:227` | “点击成功”与“工作完成”是不同反馈 |
| 未委托的原问题只能生成回答建议 | `crates/brain/src/lane/prompt.rs:63` | 推荐答案与实际替用户提交答案要分开 |
| 建议绑定精确 QuestionRef、现存问题与状态；秘密问题不能代答 | `crates/app/src/app/brain_host/questions.rs:73` | 决定单要绑定原问题，不从摘要猜原题 |
| 采用 / 编辑动作才产生明确授权，送达前复核问题、作用域与时效 | `crates/app/src/app/brain_host/questions.rs:106`、`crates/app/src/app/brain_host/questions.rs:145` | 不能让模型自报“用户同意”来绕过确认 |
| 明确委托只对这一原问题有效，一次性消费并有时效 | `crates/app/src/app/brain_host/questions/delegation.rs:3`、`crates/app/src/app/brain_host/questions/delegation.rs:58` | 不把一次“你替我回答”升级为永久授权 |
| accepted 仅表示宿主接收，原题 Answered 才表示回答完成 | `crates/brain/src/lane/prompt.rs:63`、`crates/brain/src/lane/prompt.rs:128` | TODO 不应因模型说“已提交”就提前消失 |
| 判断审批可逆性、项目范围与外部影响，拿不准则升级用户 | `crates/brain/src/lane/prompt.rs:109` | 页面沿用原闸门，不承诺默认全自动 |

**仍需设计并验证：** 独立工作身份与卡片、会话、目标的映射；跨 Agent 工作依赖；对话中如何选择明确工作；人类 TODO 的去重、排序、消失条件；推荐依据的可追溯性；直接接管后如何把新事实带回 Norma；多工作持续上下文。这些不是换掉抽屉就自然具备的能力。

**保留画布的建议：** 大对话页是用户与 Norma 讨论目标、处理决定的工作台；原画布仍是 Worker 的可见执行现场。工作单和决定单都能进入精确卡片，返回后保留对话位置与处理状态。不要另做一套 Agent 仪表盘要求用户再次逐个巡检。

## 背景与呈现边界

父任务已通过生产设置确认当前壁纸是 `wallpapers/norma-awake.mp4`，资源位于 `crates/app/assets/wallpapers/norma-awake.mp4`。它应作为后续高保真原型的共同背景，而非替换画布的理由。HTML 说明页只解释流程和关系，不把建议写成已经实现的新产品页面。

## 一手资料

- [S0：官方研究原型说明](https://github.com/deepseek-ai/dsh-superagent-prototype/blob/8c32a9dc5d72c033816c6a8110b37f40cf94fba4/README.md)
- [S1：设计原则](https://github.com/deepseek-ai/dsh-superagent-prototype/blob/8c32a9dc5d72c033816c6a8110b37f40cf94fba4/docs/000-philosophy.md)
- [S2：领域概念、关联与授权](https://github.com/deepseek-ai/dsh-superagent-prototype/blob/8c32a9dc5d72c033816c6a8110b37f40cf94fba4/docs/001-domain.md)
- [S3：工作生命周期、后台执行与恢复](https://github.com/deepseek-ai/dsh-superagent-prototype/blob/8c32a9dc5d72c033816c6a8110b37f40cf94fba4/docs/003-execution.md)
- [S4：主会话、执行会话与问答职责](https://github.com/deepseek-ai/dsh-superagent-prototype/blob/8c32a9dc5d72c033816c6a8110b37f40cf94fba4/docs/004-prompts.md)
- [S5：面板要回答的问题与信息层级](https://github.com/deepseek-ai/dsh-superagent-prototype/blob/8c32a9dc5d72c033816c6a8110b37f40cf94fba4/docs/005-panel.md)
- [S6：Router 实际提示词](https://github.com/deepseek-ai/dsh-superagent-prototype/blob/8c32a9dc5d72c033816c6a8110b37f40cf94fba4/prompts/orchestrator.md)
- [S7：进度过滤与结果转交源码](https://github.com/deepseek-ai/dsh-superagent-prototype/blob/8c32a9dc5d72c033816c6a8110b37f40cf94fba4/src/app/notices.ts#L115-L129)

本次没有安装、运行上游原型，也没有验证它在真实团队中的效果。结论来自上述代码、文档及本地静态核对；“产品建议”不属于上游已实现能力。
