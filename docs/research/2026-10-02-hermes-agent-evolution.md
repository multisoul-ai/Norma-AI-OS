# Hermes 的记忆、反思与自主学习：给 Norma 的建设依据

研究日期：2026-10-02（Asia/Shanghai）。对应决策票：[Hermes 的记忆与自主学习怎样形成闭环？](https://github.com/multisoul-ai/Norma-AI-OS/issues/77)。本文是研究证据与建议，不代表 Norma 已实现这些能力。

## 结论

Hermes 最值得借鉴的是「少量常驻事实＋按需查历史＋经验沉淀为 Skill＋后台审阅」这一组合。其自进化实际改变的是记忆、提示与程序性知识，没有证据表明这个闭环训练基础模型权重。插件拓展工具，与验证学习效果是两件独立的事。[H1][H2][H3][H4]

Norma 第一版应建立个人偏好与隔离的项目记忆，把“做梦”定义成空闲时的经验整理任务：自动追加有来源的低风险事实，生成或修订 Skill 草案；新能力启用、权限扩大和破坏性的记忆合并需要用户确认。这符合已确认的「自主学习，新增能力需确认」，也比直接复制 Hermes 的 Python 插件宿主更适合现有 Rust／SQLite 分层。

## 取证范围与版本

| 对象 | 固定证据版本 | 使用方式与限制 |
|---|---|---|
| NousResearch/hermes-agent | `b78931e3b0959c42dca7400c78a4dffd1bb48575`，提交时间 2026-10-02 15:16:58 UTC | 直接读取 GitHub 官方源码与仓库内官方文档；以下 H 链接均固定到此提交 |
| Hermes 最新发布 | `v2026.9.24`，发布时间 2026-09-24 | 只核实发布元数据；未逐一验证发布标签是否包含 main 上的所有功能，不能把本文全部能力归属于该 release。[H18] |
| Mem0 OSS | `abb81c88e1f738a8117d8293530fbc31a5ef8fd9`，提交时间 2026-10-01 16:16:18 UTC | 只作记忆粒度／作用域对照，不采用其宣传中的效果数字。[M1] |
| Norma | `55ca610182c6093135019587d7b7e225e8197584` | 核对 Master prompt、会话、工具预算与后台存储；完整架构调查由另一决策票承担。[N1][N2][N3][N4] |

外部实现结论限定于实际读取的模块；未运行 Hermes、未复现其性能数字，也未审计所有第三方插件。

## 1. “DeepSeek Hermes”需要澄清名称

官方仓库把 Hermes Agent 明确归属于 **Nous Research**，DeepSeek 在代码中是 `plugins/model-providers/deepseek` 下的模型供应商，注册 API 地址、模型与推理参数，并非 Hermes 记忆／学习系统的所有者。[H1][H17]

因此目前最可靠的解释是「以 DeepSeek 模型驱动 Hermes Agent」，不是一个已确认存在的“DeepSeek 官方 Hermes 框架”。用户若指另一仓库，需提供链接再比较；本次不把两者混称。

## 2. 内置记忆：小常驻区，而非把全部聊天塞进上下文

| 机制 | 已实现的事实 | 对 Norma 的启示 |
|---|---|---|
| `MEMORY.md` | 环境事实、项目约定与经验，默认上限 2,200 字符 | 常驻信息必须有预算；字符上限不能直接当作中文 token 上限 |
| `USER.md` | 用户身份、沟通方式与偏好，默认上限 1,375 字符 | “用户是谁／喜欢什么”与“项目如何工作”分开存储 |
| 注入时机 | 会话开始加载为冻结 system prompt 快照，保存后下一会话可见；工具回执显示实时状态 | 利于前缀缓存，但需明确“已保存”与“本会话已加载”的区别 |
| 更新操作 | `add`／`replace`／`remove`，唯一子串匹配；过量写入失败而非静默清空 | Norma 使用稳定记录 ID＋版本，更容易审阅与避免模糊替换 |
| 持久化 | 文件锁下重新读盘、检测外部漂移，再写回；读取失败不会当作空文件覆盖 | 用 SQLite 事务和修订号实现相同的数据保护 |
| 写入权限 | `memory.write_approval` 默认关闭；开启后前台可提示、后台进入 pending | 低风险学习与破坏性更新可采用不同规则 |

来源：[H2][H5][H6]。文档中的 token 近似值是英文经验估计，不作为 Norma 中文预算依据。

一个重要细节：当前源码的无人值守 `_background_delete_gate` 即使通常允许自由写记忆，也会把 `replace`／`remove` 转为待审提案；自动 review 可直接 `add`，不能自行删除原事实。这比笼统的“后台可改记忆”说明更严格。[H6]

作用域也不能照搬：Hermes 默认按 **profile／HERMES_HOME** 区隔记忆；`MEMORY.md` 可以混存项目事实。它不天然满足用户要求的「同一 Norma 身份拥有全局个人偏好，但项目知识默认隔离」。`session_search` 还支持显式打开其他 profile 的数据库；profile 不是 Norma 所需的项目授权边界。[H2][H7]

## 3. `session_search`：用可追溯原文补常驻区的缺口

当前源码提供四种形状：关键词发现、围绕消息的窗口滚动、读取会话、浏览会话。SQLite FTS5 负责查找，返回实际数据库消息，不进行 LLM 摘要；发现模式按会话 lineage 去重，压低 cron 结果的排序，避免定时任务淹没人类对话。[H7]

其收益是事实回查：记忆摘要说“用户偏好 X”时，可以查出是哪次对话说的；代价是关键词无法保证找到语义改写。中文也要专门处理，Hermes 当前具有 `cjk_unicode61` CJK bigram tokenizer 与降级路径，说明“SQLite 有 FTS5”不等于中文检索已经好用。[H8]

**官方文字与代码存在两处差异，采信代码：**

- README 仍称 “FTS5 session search with LLM summarization”，当前 `session_search_tool.py` 明确 `No LLM calls`。[H1][H7]
- memory 文档写 “no truncation”，当前源码设置读会话每消息 2,000 字符上限，也对发现与滚动输出设上限，并返回 `content_truncated`／原始长度；不能承诺查一次就得到完整原文。[H2][H7]

Norma 初期可使用 SQLite 历史＋关键词检索，不必先引入向量服务；必须保留消息锚点、来源、时间、项目筛选与明确的截断状态。若需要用户措辞变化后的语义召回，再通过中文评测决定是否补 embedding。

## 4. Skill 自创建与修改：程序性知识，不是权重训练

`skill_manage` 已实现创建、定点 patch、整体重写、删除、支持文件写入／移除；`SKILL.md` 存步骤和坑点，`references/` 存按需深度材料。`/learn` 也是让活跃 agent 读取来源、经现有工具写 Skill，而非触发模型训练。[H3][H9]

来源质量、用户纠正、多步骤成功路径，都可触发经验提炼；更新已有 Skill 优先于为每次事故新建一个 Skill。后台更新必须先读取当前目标文件，且仅能修改 agent／curator 管理的资产；内置、hub 安装、外部目录、pinned、用户所有的 Skill 都受后台 guard 保护。[H4][H10]

`skills.write_approval` 默认关闭，开启后所有 Skill 写操作进入持久 pending 区，用户查看 diff、批准或拒绝。`skills.guard_agent_created` 是独立的内容扫描开关，源码默认关闭；扫描、审批、功能评测分别解决不同问题，不能互相替代。[H3][H9]

**已实现的是知识与文件变更闭环。** 所读写入／反思／整理器模块中，没有“新 Skill 对留出任务胜过旧版本才发布”的统一质量门禁。可记录使用、patch、归档、回滚，并不能证明任务成功率提高；该评价是本次取证的限制，不是对全部 Hermes 模块的否定。[H9][H10][H11]

## 5. “做梦”：实际有反思和整理，但不是夜间幻想生成器

Hermes 可以分成三条实际通路：

1. **回合后的 background review**：主答复完成后，根据记忆／Skill 触发条件 fork agent，读取对话快照并决定持久化；不是每个回合必然都调用模型。后台 fork 与主会话持久化、外部记忆 provider 分离，限制派发工具，只允许相关记忆／Skill 管理和只读文件工具；普通 terminal／文件写工具不在默认白名单内。[H4][H12]
2. **本地推理空闲队列**：受管理的本地 runtime 默认延迟 review，同一 session 合并为最新快照；前台新请求可抢占。队列在内存中、尽力而为，默认等到约 30 分钟也会派发，不是持久可靠任务队列。[H13]
3. **Curator**：空闲触发、没有独立 cron daemon；默认间隔 `24×7=168` 小时，最小空闲 2 小时。确定性的过期／归档逻辑默认开启，LLM consolidation 默认关闭。只处理 curator 管理资产，保护 pinned 与 cron 引用，最大破坏动作是可恢复归档，并有 ledger／报告／rollback。[H11]

Hermes 另外有 cron 调度器，运行隔离任务、保存输出、可把前次任务输出作为后续上下文；这些是自动化基础设施，不等于跨项目记忆整理。[H14]

在所检查的上述核心模块中，没有独立的“dream”协议／产品状态机。“做梦”应被产品化为 **审阅真实经历、整理冲突、生成可验证提案**；想象得到的新思路可以作为灵感，不能自动进入“已证实事实”。

值得避免的直接复制：当前 review prompt 强调“多数 session 至少应有一次 Skill 更新，什么也不做是错过机会”。这种措辞会激励低质量积累；Norma 应允许明确 `no_change`，只有证据与复用价值足够才出提案。[H4]

## 6. 插件与权限：能扩展能力，但默认不是隔离

Hermes 插件可注册工具、hook、模型供应商与 UI 扩展。第三方 general plugins 通过显式 enabled allow-list 启用；声明特权 capability 后可记录用户 consent、时间与声明集合 hash，升级增加 capability 时需要重新授权。[H15][H16]

源码中的 capability 集中在覆盖内置工具、切换 LLM provider／model／profile／task，以及 gateway platform actions；这是**宿主 API 的准入边界**，不是完整的文件／网络资源沙箱。[H16]

官方文档明确：插件是进程内 Python，恶意代码可以绕过这些 gate，和当前用户一样访问系统、启动子进程。安装时静态扫描和固定 commit provenance 能减少风险，不能证明插件安全、不能证明它改善了 Norma 的表现。[H15]

Norma 首版宜保留现有 typed tool／Adapter 作为能力宿主，用现成 Skill 文件表达学到的流程；需要真实新工具时接入受控 MCP／独立子进程能力并显式授权。独立进程本身也不是完整 sandbox，仍需资源与作用域限制；无需为了“自进化”先建设任意动态代码插件市场。

## 7. “有温度”：稳定人格、用户偏好与事实边界

Hermes 的 `SOUL.md` 是 profile 身份与表达基线，放在 system prompt 首位，仅从 HERMES_HOME 加载，避免切到不同工作目录人格随之变更；`/personality` 是会话 overlay。`USER.md` 负责用户偏好、期待和沟通方式，项目约定留在项目上下文。[H2][H19]

Norma 已有固定姓名、唯一 Master 身份、自然柔软语气，并要求依据真实回执表达状态。[N1] 因此“有温度”可以扩展为稳定说话方式、准确记住偏好、理解用户当前关注什么，不能靠假称记住、猜测私人事实、把推断的人格当成确定事实。

人格编辑与学习记忆应分开：学习不能悄悄改系统安全边界或抹掉用户制定的表达偏好；长期人格修改应可见、可撤回。只有存储成功，才可以说“我记住了”；失败需真实回执。这是 Norma 现有工具诚实性原则在记忆上的延伸。[N1]

## 8. 简洁对照：Mem0 补的是条目化记忆

Mem0 OSS 以细粒度记忆记录、metadata 和 filters 区分 `user_id`／`agent_id`／`run_id`，要求至少一种身份；提供 `add`／`search` 及更新、删除历史，向量存储与 LLM 提取分别作为可配置部件。当前源码仅把确认成功存入 vector store 的记录报告为 ADD，而非把抽取结果直接视为持久化成功。[M1]

可借鉴的是**稳定记忆 ID、作用域和成功回执**，不必搬整个 SDK：它不替 Norma 决定哪些事实跨项目共享、哪些经验能自主启用，也不自带 Norma 的 Skill 评测与人格产品政策。Mem0 与 Hermes 的职责互补；把“有 memory SDK”当成“自进化 agent 已完成”仍会缺提案、审阅和验证。

## 9. 迁移到现有 Norma 的最小路线（建议）

现状：`NormaSession` 保存内存里的 protocol history、project_id、工具 runtime 与预算，每 turn 生成 master preamble；已有 conversation-tail SQLite 表与后台 writer，但它们服务其他 Agent 的近端上下文，不应直接等同于 Master 长期记忆。`crates/agent` 不能反依赖 storage，后台存储模式已经存在。[N2][N3][N4][N5]

| 最小部件 | 建议放置与职责 | 必须守住的边界 |
|---|---|---|
| Memory 条目／快照 | `agent` 定义纯类型、作用域、选取与预算；`storage` 负责 SQLite 事务；`app` 组装 | host 指定 scope，模型不能自选别人的 project_id；个人偏好＋当前项目才可注入 |
| Master 历史 | 保存实际 user／assistant／tool 回执，并开放只读查找 | 保留 source session／turn／message；失败、中断与 unknown 不标记为成功经历 |
| 经验整理任务 | 回合完成标 dirty，空闲后台按 checkpoint 处理新材料 | 不阻塞 GPUI、不会自动回复／派发其他 Agent；项目关闭与取消要有生命周期 |
| Skill proposal | 复用 Agent Skills 的文件格式；SQLite 存版本、状态、来源与评测记录 | 生成草案 ≠ 启用；旧版本 hash 校验后才能应用 diff；用户所有资产默认保护 |
| 能力启用 | 沿用 typed tools 与现有 Adapter；后来按需接 MCP | 宿主执行硬权限，不能只靠 prompt；新工具／扩权需确认 |

最少的状态即可覆盖：`draft → checked → pending_confirmation → active`，失败可 `rejected`，已启用可 `disabled`／回退。记忆自动追加可独立于能力发布；用户已授权的范围内，现有 Skill 的经验改进通过测试后可自动生效，但不能顺带扩大工具权限。

记忆建议至少有 `id`、`scope`、`project_id`、`kind`、`content`、`source_message_id`、`created_at`、`revision`、`status`；用条目修订记录保留合并前事实，不先建知识图谱。个人偏好允许跨项目，项目经验默认不跨项目；清理项目记忆不应删用户全局偏好。任务状态／临时路径不能自动升级为长期事实。

“做梦”首版输出只需三类：新增有据记忆、冲突／过期候选、Skill 改进提案。可为空；一次只处理有限的新会话，记录 watermark 与预算，重启后可以继续，而非反复重读全部历史。可见报告回答“整理了什么、依据是什么、哪些等待确认”，不创造虚假的睡眠／情绪事实。

检索先用现有 SQLite。中文验证至少覆盖短词、混合英文代码标识符、同义改写与跨项目同名；若原生 FTS tokenizer 不能满足，再选择 trigram／预分词或 embedding，避免直接把 Hermes 专用 CJK 扩展作为必需依赖。

## 10. 应进入后续决策票的验收标准

- **记住与忘掉**：保存有回执，重启后可查，删除后不再注入；修订、删除与整库导出都可由用户操作。不要承诺文档缺乏证据的性能数字。
- **项目隔离**：项目 A 的私有事实／历史不能进入 B 的 prompt 或搜索结果；全局偏好仍可见。作用域过滤应在 SQL／宿主层执行，不依靠 LLM 自律。
- **反思边界**：前台新任务抢占；取消／失败不变成功记忆；无新经验时不产出；超预算停止；没有批准的新能力不进入 active。
- **学到的东西能验收**：对代表任务重放旧版和新版，检查成功率、工具次数与权限变化；通过标准要预先确定，单纯“写了新 Skill”不算改善。
- **温度可信**：个性与偏好可编辑；推断与用户明确事实分开；引用记录有来源；没有保存成功不能说已记住，不能生成用户未说过的个人经历。

本次只形成证据与可实施的最小方向，不设计页面视觉、不接入插件、不修改生产代码。设置页的信息结构、默认预算与学习发布策略仍应由其决策票确认。

## 一手来源索引

所有源码链接固定提交；行号可直接复查。H 文档与源码描述不一致时，上文已明确标注。

- [H1] [Hermes 官方 README](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/README.md#L19)：所有者与 learning loop 宣传。
- [H2] [Persistent Memory 官方文档](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/website/docs/user-guide/features/memory.md)：文件、限制、冻结快照、profile、审批与 session search。
- [H3] [Skills 官方文档](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/website/docs/user-guide/features/skills.md#L610)：程序性记忆、写入操作、pending／diff／审批。
- [H4] [background_review.py](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/agent/background_review.py#L440)：review 提示、fork 隔离、dispatch 白名单（同文件 L1109）。
- [H5] [MemoryStore](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/tools/memory_tool_store.py#L90)：冻结快照、文件锁、漂移检查、条目操作。
- [H6] [memory_tool.py](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/tools/memory_tool.py#L166)：无人值守 replace／remove 强制进入 pending。
- [H7] [session_search_tool.py](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/tools/session_search_tool.py#L1)：四类读取、无 LLM、限额、lineage、其他 profile 只读访问。
- [H8] [hermes_state_fts.py](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/hermes_state_fts.py#L1)：CJK bigram tokenizer 与 FTS5 降级。
- [H9] [skill_manager_tool.py](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/tools/skill_manager_tool.py#L42)：agent-created 扫描默认关闭、写入与原子回滚。
- [H10] [skill_manager_guards.py](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/tools/skill_manager_guards.py#L164)：后台资产所有权与读后写限制。
- [H11] [agent/curator.py](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/agent/curator.py#L1)：空闲调度、默认周期、生命周期、归档与整理；LLM consolidation 默认关闭。
- [H12] [turn_finalizer.py](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/agent/turn_finalizer.py#L738)：真实后台 review 触发与取消／cron 抑制。
- [H13] [review_idle_queue.py](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/agent/review_idle_queue.py#L1)：受管理本地 runtime 的空闲合并队列。
- [H14] [Cron 官方文档](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/website/docs/user-guide/features/cron.md#L944)：隔离任务与 `context_from`；另见 [Cron internals](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/website/docs/developer-guide/cron-internals.md)。
- [H15] [Plugins 官方文档](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/website/docs/user-guide/features/plugins.md#L532)：consent、升级重授权、不是沙箱（L587）与安装扫描（L745）。
- [H16] [plugin_capabilities.py](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/hermes_cli/plugin_capabilities.py#L27)：实际 capability 集合、grant 与声明 hash。
- [H17] [DeepSeek provider](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/plugins/model-providers/deepseek/__init__.py)：供应商注册与原生 API 参数。
- [H18] [最新发布 v2026.9.24](https://github.com/NousResearch/hermes-agent/releases/tag/v2026.9.24)：发布元数据，不能代替 main 功能验证。
- [H19] [Personality 官方文档](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/website/docs/user-guide/features/personality.md#L9)与 [SOUL 使用指南](https://github.com/NousResearch/hermes-agent/blob/b78931e3b0959c42dca7400c78a4dffd1bb48575/website/docs/guides/use-soul-with-hermes.md#L68)：人格位置、加载作用域、审批与安全扫描。
- [M1] [Mem0 OSS memory/main.py](https://github.com/mem0ai/mem0/blob/abb81c88e1f738a8117d8293530fbc31a5ef8fd9/mem0/memory/main.py#L314)：身份／filter 构造；同文件 L760 add、L1054 持久化成功过滤、L1393 search。
- [N1] [Norma Master prompt](https://github.com/multisoul-ai/Norma-AI-OS/blob/55ca610182c6093135019587d7b7e225e8197584/crates/agent/src/master_prompt.rs#L3)：现有身份、语气与真实回执规则。
- [N2] [NormaSession](https://github.com/multisoul-ai/Norma-AI-OS/blob/55ca610182c6093135019587d7b7e225e8197584/crates/agent/src/master.rs#L91)：内存 history、project_id、turn preamble 与取消。
- [N3] [ToolCallPolicy](https://github.com/multisoul-ai/Norma-AI-OS/blob/55ca610182c6093135019587d7b7e225e8197584/crates/agent/src/tool_policy.rs#L5)：单回合预算、连续失败、unknown 停止。
- [N4] [BackgroundConversationTailStore](https://github.com/multisoul-ai/Norma-AI-OS/blob/55ca610182c6093135019587d7b7e225e8197584/crates/storage/src/background_conversation_tail.rs#L20)：后台串行存储模式。
- [N5] [Storage migrations](https://github.com/multisoul-ai/Norma-AI-OS/blob/55ca610182c6093135019587d7b7e225e8197584/crates/storage/src/migrations.rs#L280)：Agent conversation tails；[项目边界](https://github.com/multisoul-ai/Norma-AI-OS/blob/55ca610182c6093135019587d7b7e225e8197584/AGENTS.md)。
