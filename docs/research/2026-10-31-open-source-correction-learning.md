---
title: 开源 Agent 怎样从纠正中学习
subtitle: 5 个项目的源码架构与流程 · 2026-10-31
template: doc
theme: paper
lang: zh
source: 官方仓库固定提交，静态调用链核验
---

这五个项目主要通过更新记忆、Skill 或提示词，让未来的 Agent 改变行为。最值得 Norma 借鉴的是：保留真实经历，后台归纳，受控发布，再明确地让新版本进入后续上下文。

## A 先看全貌：它们究竟在“进化”什么

```flow
(真实对话与任务结果) -> 后台回顾: 回合结束或定时触发
后台回顾 -> {需要改变什么?}
{需要改变什么?} -> 用户与项目记忆: 事实和偏好
{需要改变什么?} -> 方法与Skill: 可复用操作步骤
{需要改变什么?} -> Agent提示词: 回答和决策规则
用户与项目记忆 -> *未来上下文
方法与Skill -> 未来上下文: 按需读取
Agent提示词 -> 未来上下文: 应用选择新版本
未来上下文 -> 下一次建议与执行
```

**这张图是跨项目归纳，不是任何单一项目的完整实现。** 在所查学习通路中，改变的是模型读到的文字和外部状态；没有基础模型权重训练步骤。每个项目怎样写入、发布和加载，见后面的独立图。

| 项目 | 主要学习对象 | 核心实现思路 | 最值得看什么 |
| --- | --- | --- | --- |
| Hermes Agent | 用户偏好、事实、Skill | 完成回合后 fork 后台 review，调用记忆和 Skill 工具 | 怎样把纠正转成操作经验 |
| AgentConnect | Markdown 记忆、Skill 候选 | 当轮增量蒸馏 + 周期性 Dream 暂存重建 | 如何整理和发布记忆新版本 |
| Mem0 Python OSS | 条目化事实与偏好 | 当前 V3 召回旧项后提取并追加新项 | 新旧事实为什么可能同时存在 |
| LangMem | 结构化记忆、提示词候选 | 检索后提取/补丁；另有反馈驱动的 prompt optimizer | 应用如何组合自己的学习流程 |
| Letta Code | 核心上下文、外部记忆、Skill | 反思子 Agent 在独立记忆 worktree 修改，合并后更新上下文 | 如何避免后台直接覆盖前台记忆 |

**范围与时间。** 调研读取 2026-10-31 可取得的官方 main，并固定下表提交。main 不等同于已发布版本。没有安装或运行第三方 Agent，也没有测量学习成功率、成本或延迟。

| 项目 | 固定提交 | 提交时间 UTC |
| --- | --- | --- |
| Hermes Agent | [0c181d3782ce](https://github.com/NousResearch/hermes-agent/commit/0c181d3782ce64fbb3b6835127732e53c948ebf1) | 10-31 09:03 |
| AgentConnect | [b1ea82dd5d7e](https://github.com/agentconnect-md/agentconnect/commit/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242) | 10-31 03:54 |
| Mem0 | [753496cf1435](https://github.com/mem0ai/mem0/commit/753496cf1435d76c3a711f3af51303f7fdcb6420) | 10-30 21:35 |
| LangMem | [839c3fa0fc2c](https://github.com/langchain-ai/langmem/commit/839c3fa0fc2c118734611d5d86503e52fc3a3dd1) | 10-30 19:35 |
| Letta Code | [59df54248384](https://github.com/letta-ai/letta-code/commit/59df5424838419e083de0691fa4d08524f348937) | 10-31 05:20 |

图中“模型判断”表示靠提示词和推理完成；“宿主检查”表示有实际代码校验。后者通常核验身份、版本或文件一致性，不能自动证明经验总结正确。

## B Hermes：回合后回顾，把纠正写成记忆或方法

```flow
(本轮答复完成) -> 调度检查: nudge到期且未中断
调度检查 -> 后台review副本: 满足触发条件
后台review副本 -> *模型寻找纠正与经验
模型寻找纠正与经验 -> 用户偏好与事实
模型寻找纠正与经验 -> 方法与Skill
用户偏好与事实 -> {无人值守改写或删除旧记忆?}
{无人值守改写或删除旧记忆?} -> 待用户批准: 是
{无人值守改写或删除旧记忆?} -> 记忆写入工具: 新增，普通审批可配置
待用户批准 -> 记忆写入工具: 批准并复验原条目
方法与Skill -> Skill写入工具: 先读再改，审批可配置
记忆写入工具 -> [(记忆文件)]
Skill写入工具 -> [(Skill与变更记录)]
记忆文件 -> 后续会话加载快照
Skill与变更记录 -> 后续按索引选用
```

**怎样发现纠正。** 回合完成后的触发器决定是否启动 review。模型阅读对话来理解纠正。提示词列出风格、顺序、工具用法等信号，要求先更新已有方法，再考虑新增 Skill。

一条纠正就可能值得行动；调度中的迭代数不是偏好证据次数。[回合后触发器](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/turn_finalizer.py#L832-L860)、[纠正与 Skill 整理规则](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/background_review.py#L448-L570)。

**怎样保存。** 记忆工具有锁内重读、额度、精确修改和过期条目检查。无人值守的旧记忆 patch/remove 强制进入 pending；普通 memory/skills 写入审批开关默认关闭。不能把所有写入都画成“自动生效”或“必经人工批准”。[记忆修改实现](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/memory_tool_store.py#L350-L478)、[无人值守修改闸门](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/memory_tool.py#L179-L219)、[普通审批默认值](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/write_approval.py#L44-L52)。

**怎样影响未来。** 记忆 system 内容是会话加载时的冻结快照，写盘不会立即刷新这份快照。Skill 索引让后续提示能够发现相关方法，具体正文按需读取；历史消息可另用搜索工具查询。[冻结记忆读取](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/memory_tool_store.py#L649-L652)、[Skill 发现路径](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/prompt_builder.py#L1295-L1356)。

| 可以借鉴 | 不能据此推导 |
| --- | --- |
| 把“用户希望怎样处理”写成可复用方法，而非只存对话摘要 | 模型一定能识别拒绝 A 与后续 B 的因果关系 |
| 先读已有条目再修改，保留变更记录 | 文件写入成功就证明方法改善了任务表现 |
| 后台工具白名单限制 review 动作 | 有工具白名单就有完整系统沙箱 |

**当前版本的变化。** 后台 Skill 内容改写并不限于 agent-owned：当前代码允许改善读过的 Skill，ownership 主要限制自主删除。不要沿用旧资料中的更强保护结论。[当前 Skill guard](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/tools/skill_manager_guards.py#L164-L229)、[review 工具白名单](https://github.com/NousResearch/hermes-agent/blob/0c181d3782ce64fbb3b6835127732e53c948ebf1/agent/background_review.py#L1092-L1133)。

## C AgentConnect：当轮蒸馏与集中整理分成两条路径

**路径一：答复已经送达后，提取本轮值得保留的信息。**

```flow LR
(答复交付完成) -> 后台排队: 会话与capture过滤
后台排队 -> 本轮输入输出与旧主题: autoDistill开启
本轮输入输出与旧主题 -> 模型提取耐久信息
模型提取耐久信息 -> scoped记忆工具
scoped记忆工具 -> [(Markdown主题与索引)]
Markdown主题与索引 -> 新会话加载索引
新会话加载索引 -> 按需读取与词法搜索
```

实际链路是 finalizeDelivery → queueMemoryPostTurn → managed.recordTurn → extractor → writeMemory。模型被要求只追加耐久事实、跳过重复和临时信息；writer 本身也提供完整写入和精确编辑，因此“只追加”不能画成所有存储写入都被硬限制。[回合后调度](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/daemon.ts#L11912-L12017)、[蒸馏输入与规则](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/memory/distill.ts#L1-L74)、[writer 实现](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/memory/entries/writer.ts#L30-L116)。

**路径二：Dream 把一段时间的经历和旧记忆整理成完整新版本。**

```flow
(定时或手动Dream) -> 旧记忆快照与来源会话
旧记忆快照与来源会话 -> *空暂存库
空暂存库 -> 模型重建完整主题集合
模型重建完整主题集合 -> 宿主校验路径与来源
宿主校验路径与来源 -> 记忆采纳: 默认自动，也可人工看diff
记忆采纳 -> 活跃版本检查
活跃版本检查 -> [(新记忆与一次备份)]: 可发布
活跃版本检查 -> 留待处理: 冲突未能解决
模型重建完整主题集合 --> Skill候选: 独立产物
Skill候选 -> 独立显式接受: 至少2个真实来源会话
```

Dream 读取 live snapshot，在 staging 写出完整保留集合。宿主检查来源、空重建、live digest 和并发漂移；可选审核 UI 展示增加、修改和删除。发布用 replacement/backup 交换并尝试失败恢复。[快照与暂存](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/runner.ts#L517-L880)、[采纳与发布](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/runner.ts#L1193-L1355)。

**三个容易误解的默认值。**

| 问题 | 当前 managed 实现 |
| --- | --- |
| Dream 是否默认等用户确认？ | 否；effective policy 默认每日 04:00，autoAdopt=true。可配置人工审核 |
| 记忆和 Skill 是否一起自动启用？ | 否；Skill 是独立候选，使用单独的 accept 流程 |
| 是否默认每轮向量召回？ | 否；managed recallForTurn 返回空数组，启动加载索引，再按需读/search |

这些结论分别来自[默认策略](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/protocol/src/frames/memory-connection.ts#L44-L158)、[Skill 接受入口](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/runner.ts#L1540-L1598)、[managed provider](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/memory/providers/managed.ts#L103-L196)。

Skill 的“两次”是**两个不同且属于实际输入集合的会话 ID**，并非两个任务已验证成功。矛盾时偏向新证据也主要是模型规则；digest 验证不能替代事实判断。[Skill 来源门槛](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/dreamer.ts#L608-L642)、[模型整理政策](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/dreamer.ts#L143-L197)。

## D Mem0：新记忆写入了，旧结论也可能还在

这里核验的是当前 **Python OSS V3 主路径**。云服务、SDK 和集成插件的能力分开说明。

```flow
(应用提交对话和身份) -> Memory.add
Memory.add -> 最近消息与同身份旧记忆
最近消息与同身份旧记忆 -> *模型提取新条目: 兼做语义去重
模型提取新条目 -> 文本hash去重与embedding
文本hash去重与embedding -> [(追加新UUID记忆)]
追加新UUID记忆 -> search检索
search检索 -> 返回相关记忆
返回相关记忆 -> 应用注入下一次上下文
(应用明确修正旧ID) --> 显式update或delete
显式update或delete -> 旧项修改或移除
```

**当前实现与旧版文章有差别。** add 的真实调用链使用 ADDITIVE_EXTRACTION_PROMPT，生成新 UUID、插入并返回 ADD。它没有在这个提取流程里自动调用旧条目的 update/delete；代码中仍保留旧的更新 prompt 和相关说明，不能只读这些文字画旧架构图。[当前 V3 提取与写入](https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L918-L1098)。

例如旧项是“偏好整体重构”，用户后来纠正为“发版前先局部修复”。新项可能记录这个变化，但旧项不会因此自动失效。提取 prompt 提到 linked_memory_ids，当前 ADD payload 并未保存这个模型字段；实体检索中的同名关联也不是 supersedes 关系。[冲突记录政策](https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/configs/prompts.py#L605-L701)、[实际新增 payload](https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L1000-L1089)。

默认召回组合语义、BM25 和实体分数，没有“明确用户纠正必定压过旧事实”的排序保证。需要确定修改旧 ID 时，应用可以调用显式 update/delete。[召回打分](https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/utils/scoring.py#L60-L139)、[更新与删除](https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L2052-L2144)。

| 层次 | 已实现 | 应用仍需负责 |
| --- | --- | --- |
| Python OSS | 提取、分区、搜索、显式增删改、变更历史 | 何时触发、如何认证、A/B 是否是同一纠正、有效偏好状态 |
| 官方 OpenClaw 插件 | prompt 前召回注入；满足条件的 agent_end 异步 add | 适配后端与产品规则；短用户内容可能被跳过 |
| 云 SDK feedback | 向服务端发送正负反馈请求 | 云端如何利用反馈，不能从客户端代码证明 |

集成边界可查[自动召回](https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/integrations/openclaw/index.ts#L552-L590)、[回合后捕获与短内容过滤](https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/integrations/openclaw/index.ts#L877-L923)、[云 feedback 客户端](https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/client/main.py#L1232-L1254)。

对 Norma 的直接启示：原始经历可以追加，**当前生效规则必须有独立的替代与失效语义**。仅把 B 存进向量库，不能保证下次不再提 A。

## E LangMem：把学习拆成可组合的组件

**两条链路分别解决“记住什么”和“以后怎样回答”。**

```flow
(应用提供对话) -> {需要哪种学习?}
{需要哪种学习?} -> 记忆管理器
记忆管理器 -> 同namespace检索旧文档
同namespace检索旧文档 -> 模型生成结构化新增或补丁
模型生成结构化新增或补丁 -> [(BaseStore)]: put，删除按配置
BaseStore -> 下次检索或工具读取
{需要哪种学习?} -> 对话轨迹与反馈加旧prompt
对话轨迹与反馈加旧prompt -> *提示词优化器
提示词优化器 -> 是否值得修改: 默认gradient路径
是否值得修改 -> 新prompt候选: 是
是否值得修改 -> 保留原prompt: 否
新prompt候选 --> 应用评估和选择版本
```

**记忆部分。** MemoryStoreManager 先按 namespace 找旧文档，再让 extractor 根据 schema 提取或修改，用稳定 ID 映射回原 key，最后执行变化内容的 put 和允许的 delete。结构化补丁依赖 Trustcall；本次没有额外审计该依赖内部。[检索到写入调用链](https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/extraction.py#L1139-L1280)、[提取器装配](https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/extraction.py#L217-L339)。

公开 factory 的更新默认启用，删除默认关闭。默认记忆 schema 只有 content，证据 ID、明确纠正、适用条件和置信度不是强制字段；需要应用定义更合适的 schema。[factory 默认值](https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/extraction.py#L1666-L1707)、[默认记忆结构](https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/extraction.py#L64-L92)。

**后台部分。** ReflectionExecutor 支持延迟执行、同 thread 去抖和取消旧待执行任务。本地队列在进程内，不等于持久任务日志；新任务取消旧 Future，也不等于撤回已经写入的记忆。[本地反思调度](https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/reflection.py#L254-L329)。

**行为优化部分。** create_prompt_optimizer 接收旧 prompt、对话轨迹和可选反馈，支持 gradient、metaprompt、prompt_memory。默认 gradient 先判断 warrants_adjustment，再返回改进后的字符串；这里没有基础模型权重训练。[优化器分派](https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/prompts/optimization.py#L249-L269)、[反馈到候选 prompt](https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/prompts/gradient.py#L293-L399)。

主 optimizer 路径返回候选，由应用决定如何评估和投入使用。仓库另有 stateful graph 可直接把新 prompt 写进 Store，不能因此说整个仓库都只返回字符串。两者都不能自动证明新版优于旧版。[stateful prompt 写入](https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/prompts/stateful.py#L16-L54)。

对 Norma 的借鉴点是**把偏好整理与全局行为改写分开**。用户一次拒绝可以更新本事项的经验；是否修改全局决策规则，应单独生成候选并做回归验证。

## F Letta Code：反思写到独立分支，合并后再进入上下文

当前 Letta 主仓已把活跃源码指向 Letta Code，旧 V1 服务端归档。下面画的是当前 Code 的本地/CLI 反思与 MemFS 路径；云端接管反思时，本地启动器会跳过，不能把这张图当成完整云端内部结构。[官方源码迁移说明](https://github.com/letta-ai/letta/blob/8816d3a8516893bfdc0fa14bfb1235226012e295/README.md#L5)、[云端接管检查](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/cli/helpers/reflection-launcher.ts#L785-L805)。

```sequence num
participants: 主Agent, 反思调度, 记忆工作树, 反思子Agent, 记忆主版本
主Agent -> 反思调度: 回合结束，已有新转录
反思调度 -> 反思调度: 步数或上下文压缩触发
反思调度 -> 记忆工作树: 从固定父HEAD创建分支
反思调度 -> 反思子Agent: 转录与父记忆快照
反思子Agent -> 记忆工作树: 整理纠正、偏好和方法
反思子Agent -> 记忆工作树: 提交改动，或明确no_change
记忆工作树 -> 反思调度: 提交与状态
反思调度 -> 记忆主版本: 自动合并，或主Agent先审阅
记忆主版本 --> 反思调度: merged或冲突
反思调度 -> 主Agent: 合并成功后要求更新上下文
note 主Agent, 记忆主版本: 审阅者可以是Agent，并非人类批准
```

**触发和输入。** 回合结束检查 off / step-count / compaction-event。启动前检查新转录、并发运行和父记忆脏状态，再创建独立 Git worktree；没有新材料就跳过。[回合后触发](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/cli/helpers/post-turn-reflection.ts#L18-L78)、[工作树创建](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/agent/memory-worktree.ts#L125-L156)。

**归纳与发布。** reflection prompt 优先检查错误和用户纠正，再看偏好、事实、矛盾和可复用方法。模型解释纠正的含义，宿主核验 Git 状态和合并结果。冲突时 abort 并保留转录供重试；merged/no_changes 才消费材料，只有 merged 要求重编译。[反思规则](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/agent/subagents/builtin/reflection-v2.md#L15-L43)、[消费与重编译条件](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/agent/memory-worktree.ts#L235-L245)、[合并冲突处理](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/agent/memory-worktree.ts#L597-L629)。

**“explicit”不等于人工审批。** 默认 merge 回退为 auto；explicit 模式让主 Agent 在另一个后台对话审阅、修改并整合，提示词明确不等待用户输入。[默认合并设置](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/cli/helpers/reflection-launcher.ts#L57-L62)、[审阅对话规则](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/cli/helpers/reflection-integration.ts#L17-L35)。

**怎样“用上”。** MemFS V2 根记忆文件参与编译上下文，外部记忆和 Skill 按需访问。写文件不倒改当前轮次；提交并在后续重编译、新会话等边界加载后，才改变未来上下文。[记忆布局与行为规则](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/agent/prompts/letta_root_memfs.md#L26-L41)、[提交与生效边界](https://github.com/letta-ai/letta-code/blob/59df5424838419e083de0691fa4d08524f348937/src/agent/prompts/letta_root_memfs.md#L55-L58)。

对 Norma 最有价值的是“归纳、发布、加载”三个不同的完成状态。可以借鉴这个结构，不必为此把现有 MemoryStore 改成 Git 仓库。

## G 用你的例子，比较它们能学到哪一步

用户原场景：幕僚提议 A，用户不采用，自己输入 B。

```flow
(原建议A：先整体重构) -> 用户忽略
用户忽略 -> {下一条B与A明确相关?}
{下一条B与A明确相关?} -> 只保留未采纳事实: 不确定或换话题
{下一条B与A明确相关?} -> 替代方案与原话原因: 明确相关
替代方案与原话原因 -> *候选经验：保留情境
候选经验：保留情境 -> 下次同情境建议
```

上图是**我们应设计的业务关系**，不是五个项目都已经实现的内建事件协议。大多数实现把对话交给模型归纳，业务方仍需确保按钮、提议和后续输入的身份关系。

| 材料 | 例子 | 应怎样理解 |
| --- | --- | --- |
| 原建议 | “先整体重构 session 模块，再修复故障” | 是幕僚建议，不能当用户偏好 |
| 用户替代 | “先补复现测试，再最小修复；重构另开任务” | 有明确关联时，代表本次选择 |
| 明确原因 | “今天要发版” | 形成限定情境，不能省略 |
| 候选经验 | “发版前修复已有故障，优先复现和局部修复” | 推断需保留来源和例外 |
| 下次变化 | 同类发版故障建议先复现/局部修复 | 新功能架构设计不自动套用 |

**以下是基于源码的能力边界分析，未运行这些模型重放本例。**

| 项目 | 这段材料可能怎样处理 | 仍缺什么才能可靠实现你的需求 |
| --- | --- | --- |
| Hermes | review 明确关注纠正，可改用户偏好或相关 Skill | A/B 精确配对、情境适用和归纳正确性仍依赖输入与模型 |
| AgentConnect | 本轮蒸馏耐久信息；Dream 再整理记忆或提出 Skill | 忽略按钮语义、完整纠正证据与任务结果需业务绑定 |
| Mem0 V3 | 追加“从 A 改 B/在发版前选 B”的新记忆 | 明确失效旧 A；避免搜索再次同时带回 A 和 B |
| LangMem | 自定义 schema 可保存偏好并更新旧 key；反馈可送 optimizer | schema、反馈关联、发布评估和生命周期由应用组合 |
| Letta Code | 转录反思后修改记忆或方法，提交并合并 | 精确纠正关系与语义效果仍需验证，不由 Git 合并证明 |

对应证据见 B–F 的固定源码。共同边界是：**模型能够总结连续消息，不代表系统已经可靠知道“用户为什么拒绝、B 是否永久偏好”。**

**“自学习”还有三个不同强度。**

| 强度 | 能证明什么 | 本次看到的实现 |
| --- | --- | --- |
| 记录形成 | 有文件或条目保存成功 | 五个项目均有相应存储路径 |
| 发布生效 | 新记忆/方法确已成为未来输入 | 完整 Agent 项目有加载接线；库通常需应用组合 |
| 行为改善 | 同情境更符合用户，且其他任务未退化 | 所查通路普遍缺少统一的留出任务评测门禁 |

**删除也必须按层理解。** 当前条目删除、旧对话保留、版本历史/备份保留、外部模型已接收内容，是不同对象。

Mem0 delete 仍保存 DELETE history；LangMem 的 manager 主要调用 store.put/delete；Letta Git 和 AgentConnect backup 则有意保留恢复材料。所查链路不足以证明统一的“彻底忘掉且不再提取”。[Mem0 删除与历史](https://github.com/mem0ai/mem0/blob/753496cf1435d76c3a711f3af51303f7fdcb6420/mem0/memory/main.py#L2116-L2144)、[LangMem 存储路径](https://github.com/langchain-ai/langmem/blob/839c3fa0fc2c118734611d5d86503e52fc3a3dd1/src/langmem/knowledge/extraction.py#L1230-L1280)、[AgentConnect 发布备份](https://github.com/agentconnect-md/agentconnect/blob/b1ea82dd5d7eda74b4eb0eb1788a6e270083f242/packages/daemon/src/dream/runner.ts#L1250-L1355)。

## H 对 Norma 的建议：先可靠纠正，再逐步成长

下面是**调研后的建议架构**，不是已经实现的功能。

```flow
(用户纠正与真实结果) -> App宿主绑定来源: 提议、任务、对话身份
App宿主绑定来源 -> [(经历与纠正记录)]
经历与纠正记录 -> 后台幕僚归纳: 通过norma受控读取
后台幕僚归纳 -> 候选偏好
后台幕僚归纳 -> Skill改进候选
候选偏好 -> 范围证据与失效检查
范围证据与失效检查 -> [(MemoryStore新版本)]: 按已确认学习策略发布
Skill改进候选 -> 方法回归与启用审阅
方法回归与启用审阅 -> [(已启用Skill版本)]
MemoryStore新版本 -> 宿主更新可召回上下文
已启用Skill版本 -> 宿主更新可召回上下文
宿主更新可召回上下文 -> *下一次提议
下一次提议 -> 用户确认派发
```

**优先保留的四个部件。**

1. **真实纠正记录。** 借鉴各项目的真实对话输入，但 Norma 应额外保存原提议、替代方案、原因和关系是否明确。仅时间相近时不建确定关系。
2. **有界后台回顾。** 借鉴 Hermes 的回合后 review、LangMem 的延迟去抖。Norma 用自己的持久 job 和后台执行器，避免阻塞 GPUI，重复事件不增加证据。
3. **发布和使用分开确认。** 借鉴 AgentConnect 的 snapshot/staging/digest 与 Letta 的 commit/merge/recompile。Norma 可以继续用当前 MemoryStore 的版本发布，并记录哪一版进入了哪次建议。
4. **偏好与能力分开。** 个人/项目偏好按学习政策使用；Skill 或全局提示词修改另走验证与启用。行为变得熟悉，不能顺带扩大工具权限或取消派发确认。

现有接点：提议原文在 `crates/app/src/app/brain_host/proposals.rs`；忽略入口在 `crates/app/src/app/brain_host/proposal_actions.rs`；当前拒绝摘要在 `crates/brain/src/goal.rs`；来源协议在 `crates/norma/src/protocol/context.rs`；记忆写入规则在 `crates/agent/src/memory_access.rs`。brain 仍通过 norma CLI 读写宿主能力，不引入 brain → storage/agent/GPUI 依赖。

**对上一份方案应作的调整。**

| 上一份设想 | 调研后的判断 |
| --- | --- |
| 三个独立任务后作弱参考 | 是我们的待评审产品阈值，不是开源通则。明确用户纠正不应等满次数；推断偏好需看情境与反例 |
| 一个统一“自进化”流程 | 拆成当前任务纠正、跨任务偏好、可复用方法和全局提示词优化；采用不同启用条件 |
| 保存候选后下一轮就能用 | 增加加载完成与会话失效语义；Hermes 冻结快照和 Letta 重编译说明这一步不能省 |
| 模型整理时避免重复就够 | 仍需宿主身份、幂等、旧版本失效和原始来源；Mem0 V3 的新旧并存是具体反例 |
| 删除记忆即可忘掉 | 同时处理学习来源抑制、派生内容、在途任务和会话；不能承诺擦除外部副本 |

**推荐实施顺序。** 先完成“本事项明确纠正 → 保存证据 → 后台整理 → 下一次建议可追溯变化”。再增加跨任务偏好合并；可复用 Skill 和 prompt optimizer 放到有代表任务回归能力之后。无需先引入向量数据库或替换整个 Agent 框架。

**如何验证学会了。** 用相同情境对照旧版与新版建议：原问题是否少被重复提起，用户的限制是否保留，例外是否被误泛化；再用无关任务验证没有退化。保存成功、来源引用数量和 Git 合并成功都是必要证据的一部分，不能单独作为“效果提高”的结论。

**审阅依据与完整证据。** 本页是面向理解的简化图解。完整函数链、版本差异与删除限制分别保存在 `docs/research/evidence/2026-10-31-self-learning/companions.md`、`docs/research/evidence/2026-10-31-self-learning/memory-libraries.md`、`docs/research/evidence/2026-10-31-self-learning/letta-code.md`。此前待评审方案为 `docs/design-docs/2026-10-31-brain-correction-learning.html`；本轮未修改业务代码，也未把这些建议写成已生效的产品决定。
