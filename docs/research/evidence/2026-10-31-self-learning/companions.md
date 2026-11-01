# Hermes Agent 与 AgentConnect：当前源码中的学习闭环证据

调查日期：2026-10-31（UTC）。本稿供 Norma 设计比较使用，只读取官方 GitHub REST 元数据与固定提交的原始源码；未安装、运行、测试第三方项目，未验证模型实际学习效果。旧稿 `2026-10-02-hermes-agent-evolution.md`、`2026-10-02-agentconnect-memory.md` 只用作查找线索。

## 首先纠正旧资料可能留下的印象

- **Hermes 当前的后台 Skill 内容改写不局限于 agent-owned 资产。** 当前 guard 明确说 ownership 约束自主删除，内容修改可以改善读过的 Skill；提示词进一步包括手写、安装、bundled、pinned Skill。读后写、可选审批和变更 ledger 仍存在。不能沿用旧稿“所有这些资产均禁止后台修改”的概括。[H5][H6]
- **AgentConnect 当前 managed Dream 默认自动采纳记忆重建。** effective policy 默认每天宿主时区 04:00、`autoAdopt: true`、`mineSkills: true`；显式设置 false 可退出。手动审核流程仍实现，但不能画成默认必经人工闸门。挖出的 Skill 始终走独立接受入口。[A4][A10]
- 两者所读学习闭环改变的是文字记忆、Skill、索引和后续上下文；**未在这些通路发现基础模型权重训练**。这是一段具体源码范围内的判断，不代表穷尽项目所有功能。

## 版本与证据范围

| 项目 | 本次取得的 main HEAD | HEAD 提交时间 | API 返回的 latest release |
| --- | --- | --- | --- |
| NousResearch/hermes-agent | `0c181d3782ce64fbb3b6835127732e53c948ebf1` | 2026-10-31 09:03:06 UTC | `v0.22.5`，2026-10-23 05:28:45 UTC |
| agentconnect-md/agentconnect | `b1ea82dd5d7eda74b4eb0eb1788a6e270083f242` | 2026-10-31 03:54:03 UTC | `v2.8.0`，2026-10-28 03:18:12 UTC |

HEAD 与 release 不是同一版本；下文结论全部对应固定 HEAD，不把 HEAD 能力归属于上述发布标签。元数据来源：[Hermes commit](https://api.github.com/repos/NousResearch/hermes-agent/commits/0c181d3782ce64fbb3b6835127732e53c948ebf1)、[release API](https://api.github.com/repos/NousResearch/hermes-agent/releases/latest)；[AgentConnect commit](https://api.github.com/repos/agentconnect-md/agentconnect/commits/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242)、[release API](https://api.github.com/repos/agentconnect-md/agentconnect/releases/latest)。release API 会漂移，上表是调查时结果。

阅读核心范围：Hermes 的 `turn_finalizer`、`background_review`、`MemoryStore`、memory/skill 工具、write approval、curator、prompt builder、session search；AgentConnect 的 daemon post-turn/extraction、managed provider、entry writer、DreamRunner/dreamer、Skill publisher、policy 和真实前端审核代码。没有完整安全审计，也没有跨所有 provider 验证删除传播。

## Hermes：完成回合后让另一个模型通路修改知识资产

### 触发与纠正识别

**实际实现。** `turn_finalizer` 在答复完成、非中断且没有 `skip_background_review` 时，根据 memory/skill nudge 标记调用 `_spawn_background_review`。Skill nudge 按本回合累计工具迭代数判断；memory/skill 配置默认数值均为 10。这是维护调度频率，不是“需要 10 次证据才学习”。自动 review 可关闭，delegate 默认不再递归产生自动 review；`/refine` 是显式触发，可越过自动触发开关与空闲延迟。[H1][H2]

**模型提示规则。** `_SKILL_REVIEW_PROMPT` 明确把“纠正风格、语气、格式、详细程度”“纠正工作顺序”“工具/Skill 过时”列为信号，写着任一信号足以行动；先更新本回合加载的 Skill，再找已有类别 Skill，再添加支持材料，最后才创建新类别 Skill。跨任务偏好可入 USER.md，任务做法归 Skill；同一偏好不要两边重复保存。提示词也允许 `Nothing to save`，但同时要求多数会话应有更新，会形成鼓励积累的倾向。[H3]

**边界。** 这是模型从对话中理解纠正，未发现固定三次纠正/三次成功的硬门槛。`conversation_loop` 确有中断时把先前回复 checkpoint 和用户纠正拼入本轮上下文的机制；这解决当前回合接续，不能等同于自动建立长期“错误 A → 用户替代 B”的结构化关系。[H4]

### 提取、合并、存储与召回

| 层次 | 已实现 | 保证边界 |
| --- | --- | --- |
| 工作上下文 | 当前对话 history，review 读取结构拷贝；同模型 fork 可用完整快照，路由到别的模型时使用 digest | digest/上下文预算可能丢失细节，不等于完整经历保真 |
| 常驻事实 | `MEMORY.md` 默认 2,200 字符；`USER.md` 默认 1,375 字符；会话加载冻结 prompt 快照 | 中途写盘成功不刷新已加载的 system 快照；后续新会话才加载新版本 |
| 精确修改 | 模型可发 `add/patch/remove`、批量操作；`patch` 保留同一条目其余字节；重复匹配拒绝 | 语义纠错由模型决定；`add` 的宿主去重是 exact content，不是语义去重 |
| 并发与审阅 | 文件锁内重新读盘、额度与漂移检查；pending 修改 pin 完整已审条目，条目变化后拒绝重放 | 不能把文件锁、stale-entry 检查称为语义冲突解决 |
| 历史检索 | `session_search` 查 SQLite/FTS5，返回实际消息；无 LLM 摘要 | 返回有预算和截断标记；关键词召回不是语义关联保证 |
| 程序性知识 | SKILL.md、references/templates/scripts；索引进入后续 prompt，模型匹配后读取 Skill | 新文件可发现不等于总会选中、执行正确或胜过旧版 |

对应实现：[H7][H8][H9][H10]。内置记忆默认按 profile/HERMES_HOME 保存，不能直接等同于 Norma 的项目级授权与隔离。

### 写入审批、后台权限与恢复

- `memory.write_approval` 与 `skills.write_approval` 未设置时默认关闭；打开后可 stage 并审阅/批准。**无人值守 review 的 memory `patch/remove`（包括 batch）另有硬 gate，无论普通写入审批是否打开都会进入 pending，失败则拒绝；add 仍可直接尝试。** `/refine` 的 attended review 不走该无人值守 gate。[H11][H12]
- review 的工具 dispatch 白名单默认允许 memory/skills（memory 还取决于本次触发）、`read_file/search_files`；普通写文件与 terminal 不在默认列表。用户配置 `extra_tools` 可扩大已有工具白名单。它是工具派发限制，不能据此声称完整操作系统沙箱。[H13]
- 后台修改已有 Skill 先要求本次 review fresh read。当前自主删除额外保护 pinned、external、protected/bundled/hub、非 curator-managed 资产；**这些 ownership 删除保护不等于禁止内容改写**。这是当前源码相对旧材料必须保留的区别。[H5][H6]
- Skill 内容扫描 `skills.guard_agent_created` 默认 false；写入有 atomic replacement、失败扫描恢复旧文件、变更 ledger 和 prompt cache 清理。Curator 的内容整理可用前置快照/rollback，归档可 restore。没有在这些路径看到“先过留出任务质量评测，再允许启用”的统一硬门槛。[H14][H15]

### Curator 与删除边界

Curator 是空闲触发维护，默认开启、最小空闲 2 小时、间隔 7 天；首次观察先种时间戳并推迟一个周期。确定性 stale/archive 逻辑与可选 LLM consolidation 分开，后者默认关闭。过期阈值 14/30 天，未使用不是自动归档的充分证据；pin/cron 引用另有保护。归档保存在 `.archive` 可恢复，consolidation 的 prompt 要求合并到类别 Skill，但语义合并质量仍由模型承担。[H16]

从 memory 文件移除一条事实只改变该存储；当前会话冻结快照、历史消息、pending、provider 的独立存储与备份不是同一删除操作。Skill 归档本就特意保留恢复材料。**未在所读通路发现统一的、阻止旧对话再次提炼出被遗忘事实的 tombstone 协议，不能承诺删除后永不复活。**这不是断言项目所有子系统都没有相关功能。[H7][H9][H11][H16]

可用于图解的真实调用链：

```text
答复/工具回合结束
  → turn_finalizer：nudge 到期 + 有答复 + 非中断
  → AIAgent._spawn_background_review：自动开关、delegate、空闲延迟
  → _spawn_background_review_now → 后台线程 → _run_review_fork
  → 模型读对话 + review policy（纠正识别在模型中）
  → dispatch 工具白名单
      ├ memory add → 可选审批 → MemoryStore 锁/重读/额度 → 文件
      ├ 无人值守 memory patch/remove → pending → 人工批准 → pin 条目复验
      └ skill_manage → 可选审批 → fresh-read guard → 文件/ledger/cache
  → 后续会话：冻结 memory 快照 + Skill index；需要时查历史原文

空闲维护（独立）→ Curator：确定性 stale/archive；可选模型合并
                    → 归档/快照/报告/恢复
```

其中“模型识别纠正”“prompt 要求避免噪声”应画成模型判断，不能画为宿主已保证的事实验证器。

## AgentConnect：逐回合蒸馏，周期性完整重建，Skill 独立采纳

### 逐回合蒸馏

**实际调用链。** `finalizeDelivery` 等答复发布、工具记录与 apply chain 收口后调用 `queueMemoryPostTurn`；后者依据会话选择 memory layer，跳过研究/认识用户任务的内部会话、capture-excluded shared turn 等，managed 工作按 agent 排后台 chain。`ManagedMemoryProvider.recordTurn` 需 `autoDistill` 与 extractor 才执行，生成 sourceTurnId、检查支持它的 home 的 capture suppression，然后把本轮 input/output 与少量现有 MD 交给提取器。[A1][A2]

`runMemoryExtraction` 按 agent+layer 串行运行 extraction；绑定专用 memory MCP scope 与 `source:'distill'`，建立 read-only 或 plan session，缺少已验证模式则失败。支持 trusted system prompt 的 runtime 用专门通道，其他 runtime 将政策前置到文本。read-only 限 native tools，专用 writeMemory 仍被允许。home 不可达时有 durable capture outbox 的补偿路径；普通后台 Promise chain 不能笼统称为每个回合都已先持久化的可靠队列。[A1][A3]

**提取政策是模型规则。** `MEMORY_DISTILLATION_SYSTEM_PROMPT` 规定只提耐久事实、仅追加、不重写删除、语义重复不写、先读同主题再写；没有 durable fact 是常见情况。`buildDistillationPrompt` 只有本轮 input/output（各最多 16,000 bytes）和最多 9 个现有文件（每个约 4,000 bytes、总 32,000 bytes），没有给每条纠正构造明确的 rejected-action / replacement-action 键。[A5]

**宿主 writer 不只允许 append。** `writeMemory` 暴露完整内容或 exact edit；entry writer 还可 update/delete、校验 revision、exact edit 唯一匹配。不能把提示词 `additive only` 画为文件层已强制不可删改。[A6]

### 长期存储与召回

managed provider 是逻辑 Markdown 文件集合：`MEMORY.md` 目录加 topic `.md`，具体 home 可为 daemon 文件、sandbox volume 或 Control Plane。它不是“所有正文都在本地 SQLite”。共享层、person 层、conversation 层有不同写入目标与读 overlay；同名 active 文件遮蔽 inherited 文件。profile、agent、person、conversation 这些范围不能未经核验直接映射 Norma 项目隔离。[A2]

`standingContextAtSessionStart` 加载相关层索引；managed `recallForTurn()` 自身返回空数组，后续内容由工具按需读取。entry `search` 是在 label/description/body 上按空白切词的确定性 substring 评分，最多扫描 2,048 项/16 MiB，结果标 lexical/partial coverage；不是默认每回合 embedding top-K 自动注入。[A2][A7]

### Dream：提示词语义合并，宿主暂存与发布

当前 effective policy 默认 daily 04:00、自动采纳记忆；Skill mining 默认开但不自动安装。[A4]

`DreamRunner.start → reserve` 在 agent 锁内选来源会话（自动窗口根据上次成功 Dream 后的活动，最多 100），对 live store 做 snapshot/digest，保存 job 并异步 run；run 在空 staging 中调用模型。Dream policy 让模型合并重复、矛盾保留较新值、剔除瞬时信息和 secrets，完整写出要保留的 topic，没写的文件代表拟删除；MEMORY.md 由宿主生成。宿主验证路径和 bound memory-topic provenance，对“旧库非空但没有写任何 topic”的重建拒绝全删。[A8][A9]

这里“矛盾保留最新”是模型指令，不是时间字段驱动的确定性裁决；也不代表用户明确否定就能可靠把 A 与稍后 B 自动配对。

`adopt` 验 reviewed bytes digest（caller 传 token 时），再验 live snapshot；只有已证明 distill-only 的漂移才尝试按行增量 rebase，其他变化拒绝或由显式 force 覆盖。默认 auto-adopt 路径不传 review token，保留 live 冲突 fence，失败留下可审候选。人工 UI 用 live/staged 路径并集，删除项优先，逐文件 LineDiff；这是真实实现的可选审核通路。[A10][A11]

发布准备 sibling replacement，再把 live 改名 backup、replacement 改名 live；第二步失败尝试恢复。目录锁是宿主协调机制，两个 rename 不是一次跨所有文件/所有 reader 的通用事务。成功后只保留这次采纳的 backup；不能承诺任意历史版本都可回滚。[A10]

### Skill 成长、证据门槛与执行边界

- `MIN_SKILL_SESSIONS = 2`。解析器确实要求至少两个 **不同且属于实际 mined-set** 的 session IDs，去重并排除外来 ID；这比纯 prompt 的重复要求更硬。**这是候选引用数量/来源校验，不是重放验证这两个会话确实成功，也不是固定三次学习门槛。** 普通记忆和用户纠正没有在所读代码中发现三次门槛。[A12]
- `skillAccept` 独立于 memory adopt；读取 staged 内容后用 `expectedDigest` 绑定发布快照，发布 immutable bounded local-source bundle/index，再标 accepted。`skillDismiss` 记录 dismissed 并删除 staging，后续 Dream 可得到已拒绝名称以免重复建议。该 rejected-skill 名称记忆不应外推成通用“用户拒绝任何 A 后学会 B”。[A13]
- publisher 会清理 unreferenced bundles，不能把 immutable 命名自动解释为无限版本保留或通用 Skill 回滚 API；本次未查到独立质量评测通过才发布的统一门槛。[A14]
- Dream 当前生产调用允许；源码中遗留 blocked/test-only policy 名称不表示它现仍全部停用。`runDreamExtraction` 使用一次性 host、完成后 stop/dispose；`buildDreamHost` 跟随 agent strategy，排除 agent tool credentials。源码明确 **unsandboxed agent 的 Dream 也不 sandbox**。验证 native read-only 模式、一次性 host 与 staged 审核是不同边界，不能夸大为所有运行环境下的完整权限隔离。[A15]

### 删除边界

entry delete 有 revision/conditional-home 检查；inherited entry 禁止从当前 view 删除，删除 override 可能重新显露 inherited memory。普通文件 home 对 topic/index/history 不提供与 CP 事务 home 相同的提交保证，writer 对 ambiguous write 要求先查状态而非盲重试。[A6][A16]

Dream backup、history、原始 transcript、已发给外部 provider 的数据与当前 active MD 是不同存储。`recordTurn` 在支持 captureStatus 的 home 能检查同 sourceTurnId suppression，防止部分迟到蒸馏，但这不是内容级全局遗忘协议；重新提炼旧会话仍是必须单独审计的问题。**未发现能保证所有历史、副本、外部 provider 同时遗忘且不会再次学回的统一实现证据。**[A2][A10][A16]

可用于图解的真实调用链：

```text
本轮答复发布完成 → finalizeDelivery → queueMemoryPostTurn
  → 会话/layer/capture policy → managed.recordTurn（autoDistill 才进入）
  → 本轮 input/output + 已有 MD → extractor session
  → native read-only/plan 硬 gate + scoped writeMemory
  → 模型判断耐久/重复/追加 → 统一 MD writer → topic + generated index/history
  → 后续 session 加载索引；模型按需读/search lexical entries

手动或每日调度 → DreamRunner.start/reserve → live snapshot + 来源会话
  → 一次性 host（strategy 跟随 agent）→ 空 staging → 模型完整重建
  → 宿主路径/来源/非空校验 → completed
      ├ 默认 autoAdopt 或可选 UI diff + reviewed digest
      │   → live digest/rebase fence → backup + directory swap
      └ Skill candidate（至少两个 mined session 引用）
          → 独立阅读/显式 accept + digest → bundle/index 发布
```

## 对“拒绝 A，再输入 B”的限定结论

两项目都能把普通对话当作后续模型提炼输入，Hermes 的 review prompt 明确要求学习用户纠正，AgentConnect Dream prompt 明确挖掘 corrections/preference shifts。**但在本次读取的链路中，未发现一般性的结构化 correction event 将 rejected A、replacement B、所处任务、作用域和证据 ID 自动可靠关联，再验证 B 成功后发布。**不能由“模型看见连续消息”推出这种关系必然学到。

Hermes 的中断 checkpoint 与 AgentConnect 的 Skill dismiss 是明确、较窄的结构化机制，应分别说明用途。固定“三次就记住”的规则在本次源码中无依据；AgentConnect 的“两次不同会话引用”只适用于 Skill 候选过滤。

对 Norma 的推断建议（不是第三方已实现）：把用户明确否定/替代写为有 source-message IDs 的候选关系，保留“不确定是否相关”，把任务范围、是否采纳、后续工具验证分开；文本合并和事实冲突不要只靠“较新句子获胜”；遗忘需另定义来源消费抑制与恢复材料保留政策。可借鉴两者的 review/diff/digest/作用域，但能力上线与学到内容正确性需要自己的验证证据。

## 固定源码索引

Hermes：

- [H1] [turn_finalizer：nudge、完成后的 review](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/turn_finalizer.py#L832-L860)；[初始化默认值](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/agent_init.py#L1210-L1253)，[Skill 默认 interval](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/agent_init.py#L1286-L1289)。
- [H2] [spawn、自动/显式触发与空闲队列](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/run_agent.py#L752-L823)。
- [H3] [纠正识别、优先更新、允许无变化的 review prompt](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/background_review.py#L448-L570)。
- [H4] [中断 checkpoint 与用户纠正进入当前上下文](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/conversation_loop.py#L325-L354)。
- [H5] [ownership 仅约束后台删除](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/skill_manager_guards.py#L164-L229)。
- [H6] [prompt 允许改善手写/安装/bundled/pinned 内容](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/background_review.py#L469-L475)。
- [H7] [MemoryStore 额度、加载快照](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/memory_tool_store.py#L175-L255)，[冻结读取](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/memory_tool_store.py#L649-L652)。
- [H8] [锁内 mutation、add/patch/remove 与 stale-entry 匹配](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/memory_tool_store.py#L350-L478)。
- [H9] [session search 非 LLM、FTS、DB 原文](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/session_search_tool.py#L1-L49)，[返回截断标记](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/session_search_tool.py#L237-L251)。
- [H10] [后续 prompt 的 Skill index 和匹配提示](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/prompt_builder.py#L1295-L1356)。
- [H11] [无人值守 destructive memory 操作转 pending](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/memory_tool.py#L179-L219)。
- [H12] [write_approval 默认关闭](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/write_approval.py#L44-L52)，[显式 attended fork](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/background_review.py#L1165-L1182)。
- [H13] [dispatch 白名单与 extra_tools](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/background_review.py#L1092-L1133)，[执行 fork](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/background_review.py#L1183-L1220)。
- [H14] [可选扫描默认 false](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/skill_manager_tool.py#L41-L64)，[写与失败恢复](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/skill_manager_tool.py#L408-L429)，[ledger/cache](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/skill_manager_tool.py#L746-L782)。
- [H15] [Curator snapshot/rollback 范围](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/curator_backup.py#L1-L15)，[rollback 实现入口](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/curator_backup.py#L406)。
- [H16] [Curator 默认、调度和自动状态迁移](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/curator.py#L31-L255)。

AgentConnect：

- [A1] [答复交付后排 memory](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/daemon.ts#L21635-L21708)，[queueMemoryPostTurn、layer/filter、后台 chain/outbox](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/daemon.ts#L11912-L12017)。
- [A2] [Managed provider：home/layers/索引/空 recall/recordTurn](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/memory/providers/managed.ts#L46-L196)。
- [A3] [scoped extraction、read-only mode、prompt transport](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/daemon.ts#L12037-L12155)；[managed outbox 适配](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/memory/managed-distill-outbox.ts#L1-L65)。
- [A4] [Dream policy 默认与 effective 解析](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/protocol/src/frames/memory-connection.ts#L44-L158)。
- [A5] [蒸馏 prompt、read-only 边界、本轮输入构造与预算](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/memory/distill.ts#L1-L74)。
- [A6] [writeMemory 专用工具写入](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/mcp/ops/memory.ts#L253-L301)，[entry mutation、revision、唯一 exact edit](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/memory/entries/writer.ts#L30-L116)。
- [A7] [lexical match 算法](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/memory/entries/managed.ts#L59-L87)，[bounded search/coverage](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/memory/entries/managed.ts#L252-L291)。
- [A8] [Dream start/reserve/snapshot](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/runner.ts#L517-L680)，[staging、bound-topic 与拒绝空重建](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/runner.ts#L705-L880)。
- [A9] [Dream 的模型纠正、冲突/完整重建政策](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/dreamer.ts#L143-L197)。
- [A10] [adopt/digest/fence/两个 rename/backup 保留](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/runner.ts#L1193-L1355)，[auto-adopt 与按行 rebase](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/runner.ts#L1366-L1482)。
- [A11] [前端 live/staged 路径并集、删除优先](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/web/src/components/console/DreamPanel.tsx#L622-L639)，[LineDiff](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/web/src/components/console/DreamPanel.tsx#L708-L726)。
- [A12] [两次不同会话常量](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/dreamer.ts#L130-L133)，[agent Skill mined-set 引用数量硬校验](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/dreamer.ts#L608-L642)，[organization Skill 同门槛](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/dreamer.ts#L545-L553)。
- [A13] [Skill 独立 accept/dismiss](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/runner.ts#L1540-L1598)。
- [A14] [Skill immutable source 发布、expected digest 与清理](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/skills/dream-skills.ts#L186-L263)。
- [A15] [生产 Dream admission](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/daemon.ts#L13090-L13100)，[一次性 host/credentials/strategy/read-only 限制](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/daemon.ts#L12457-L12678)。
- [A16] [delete 与 inherited 边界](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/memory/entries/managed.ts#L373-L405)，[override 删除显露 inherited 的工具说明](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/memory/entries/tools.ts#L92-L99)，[文件 home mutation 的保证与不足](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/memory/entries/writer.ts#L119-L168)。
