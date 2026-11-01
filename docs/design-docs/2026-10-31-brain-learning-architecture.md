---
title: Norma 幕僚学习架构
subtitle: 完整设计草案 · 模块、数据、接口、代码结构与落地顺序
template: doc
theme: paper
lang: zh
source: 2026-10-31 开源源码调研 + 当前仓库接点
---

建议由宿主管理学习：记录真实纠正，后台归纳，发布有来源的反馈记忆。后续建议明确加载这些记忆。长期习惯、Skill 和全局提示词采用不同的生效条件。本文供评审，尚未修改业务代码。

## A 产品方案：自动记住经历，谨慎固定习惯

```flow
(用户明确纠正) -> [(真实纠正经历)]
真实纠正经历 -> 后台幕僚归纳
后台幕僚归纳 -> 项目内反馈记忆: 有据的本次选择
后台幕僚归纳 -> 长期偏好候选: 模型归纳的习惯
后台幕僚归纳 --> Skill改进候选: 后续阶段
项目内反馈记忆 -> *下一次同情境建议: 作为参考
长期偏好候选 -> 用户确认适用条件
用户确认适用条件 -> 已确认偏好
已确认偏好 -> 下一次同情境建议
Skill改进候选 -> 回归验证与启用审阅
下一次同情境建议 -> 用户确认派发
```

**三个产物，三种权威。**

| 产物 | 内容 | 何时使用 | 谁决定 |
| --- | --- | --- | --- |
| 纠正经历 CorrectionEpisode | 原提议、用户选择、原因、任务与结果 | 原事项立即使用当前明确要求 | 用户输入；宿主核验身份 |
| 反馈记忆 EpisodeFeedback | “在这个情境中，用户曾这样取舍” | 后续相似事项作为低优先级参考 | 开启学习后可自动发布有据事实 |
| 长期偏好 ConfirmedPreference | “遇到某类情况，通常优先这样处理” | 在适用条件内指导建议 | 用户确认候选或明确要求记住 |

Skill 是另一类可执行方法资产，先形成候选，再验证和启用。全局提示词优化只保留设计接口，首期不自动改人格、权限或系统指令。

**默认建议，均待本次评审。** 项目明确开启学习一次；默认限制在当前画布区域。自动整理已明确关联的纠正，不每次弹窗。

推断出的长期习惯等待确认；个人范围、其他项目与其他画布不自动共享。未确认经历建议 90 天无更新后清理，已确认偏好保留确认来源。

这比“凑满三次就生效”更贴合研究结果。证据数量用于排序和提示复核，不能替代关联质量、情境与反例。一次明确纠正就会改变原事项；一次临时取舍不自动变成永久习惯。

**贯穿设计的例子。**

| 阶段 | 用户能看到的结果 |
| --- | --- |
| 原建议 | “重构 session 模块后再修复重连丢字” |
| 用户纠正 | “先补复现测试，再最小修复；今天发版，重构另开任务” |
| 自动形成反馈记忆 | “这次发版前故障修复，用户选择先复现与局部修复，重构延后” |
| 归纳的偏好候选 | “发版前修复已有故障时，优先先复现、再局部修复” |
| 下次相似事项 | 可先建议复现与局部修复，并说明参考了哪条经历 |
| 用户点“以后参考” | 将候选的范围、条件和例外确认为长期偏好 |
| 新功能架构设计 | 不因这条反馈禁止重构；当前明确要求优先 |

本页承接 `docs/research/2026-10-31-open-source-correction-learning.html`，作为调研后的完整评审入口。此前 `docs/design-docs/2026-10-31-brain-correction-learning.html` 的“三次门槛”和 SQLite 候选参考方案保留为讨论历史，不作为本稿默认值。

## B 模块架构：推理、规则、存储各归其位

**运行架构；箭头表示调用和消息。**

```flow
(提议详情与Dock) -> LearningHost: 宿主绑定真实来源
LearningHost -> [(BrainStore)]: 经历、提议、任务与发布账本
LearningHost -> brain学习执行器: 已提交的job与短期许可
brain学习执行器 -> norma学习命令: 读证据和提交归纳
norma学习命令 -> LearningHost: 端点鉴权
LearningHost -> 纯规则模块: 关系、状态与发布资格
LearningHost -> MemoryAccess: 受控发布与召回
MemoryAccess -> [(Markdown MemoryStore)]
Markdown MemoryStore -> LearningHost: 已发布且可见的记忆视图
LearningHost -> 幕僚主会话: 本轮学习快照
幕僚主会话 -> norma提议与汇报
norma提议与汇报 -> LearningHost: 写入和交付前复核
LearningHost -> (用户确认派发)
```

**Cargo 依赖图；所有能力放回已有 crate。**

```flow LR
app -> brain
app -> storage
app -> agent
brain -> norma
storage -> agent
agent -> norma
```

brain 继续不依赖 agent、storage、runtime、GPUI。图只列本功能相关依赖，未改变其他已有依赖。storage 通过 agent 重导出的 norma DTO 取得协议类型，不新增 storage → brain。

| 模块 | Interface 与职责 | 隐藏的 Implementation |
| --- | --- | --- |
| norma 协议 | LearningOp、引用、DTO、回执；CLI 参数校验 | 序列化、版本兼容和输入限额 |
| agent 学习规则 | 纯函数判定关系、归纳状态、发布资格与召回优先级 | 去重、反例、状态迁移；无 IO/LLM |
| storage::BrainStore | 记录观察、领取 job、提交归纳、读取视图、修改学习状态 | SQLite 事务、CAS、租约、幂等、恢复与删除账本 |
| storage::MemoryAccess | 受控记忆视图和学习发布 | Markdown 版本、来源桥接、召回过滤与回执恢复 |
| brain 学习执行器 | 领取宿主已安排的 job，用 ACP 归纳 | 提取提示、模型调用、有限重试和 no_change |
| app::LearningHost | 面向 UI 与端点的小接口 | 可信身份、执行调度、许可、缓存失效和发布协调 |
| UI Adapter | 展示、编辑和提交用户意图 | GPUI 状态和异步反馈，不直接读写 SQLite |

按 codebase-design 的原则，把跨入口都需要知道的复杂性收进 LearningHost 和 BrainStore。首期各有一个实际存储实现，不预建泛化 MemoryBackend 插件层，也不新建学习微服务。

LearningHost 句柄按数据库和学习 scope 复用同一进程内 coordinator，参照现有 shared_context 的注册方式。调度、许可和失效状态由该 coordinator 统一拥有，避免两个窗口各启动一份相同学习任务。视图关闭只解除订阅；App 主动退出取消模型调用，已持久化 job 留待下次核验后恢复，后台项目生命周期沿用现行规则。

**宿主对调用者的最小接口草图。** 下列 Rust 仅说明设计，尚未加入源码。

```rust title="LearningHost · 拟定 Interface"
impl LearningHost {
    async fn observe(&self, input: TrustedObservation)
        -> Result<ObservationReceipt, LearningError>;
    async fn prepare_turn(&self, input: AuthorizedTurn)
        -> Result<LearningFrame, LearningError>;
    async fn control(&self, action: UserLearningAction)
        -> Result<ControlReceipt, LearningError>;
    async fn handle_cli(&self, request: AuthorizedLearningRequest)
        -> Result<LearningReply, LearningError>;
}
```

Trusted/Authorized 类型由宿主内部构造，不直接反序列化模型 JSON。接口承诺后台 IO、稳定操作身份、明确回执；调用方无需了解 SQLite 与 Markdown 的提交顺序。

**当前基线。** 重新核对 HEAD `17c8a99e2a5d2943fdec0c06bebc0e3b4112aa2e` 及工作树。提议和问答界面存在其他未提交修改，本设计需在实施时合入那些修改，不能覆盖它们。

| 当前接点 | 已有能力 | 需要补齐 |
| --- | --- | --- |
| `crates/app/src/app/brain_host/proposals.rs` | 标题、完整任务、理由、JSON 快照 | UUID、精确来源、历史状态、事务持久化 |
| `crates/app/src/app/brain_host/proposal_actions.rs` | 接收、派发与忽略提议 | 纠正入口与稳定派发操作 ID |
| `crates/brain/src/service.rs` | 主 lane 与审批 lane | 按需学习执行器及角色隔离 |
| `crates/norma/src/protocol/context.rs` | ContextScope、ContextRef | 学习快照与操作许可协议 |
| `crates/storage/src/memory_access/publication.rs` | 记忆发布、操作预留、丢回执核验 | 学习来源与发布/撤销闸门 |
| `crates/app/src/norma/endpoint/brain_report.rs` | 汇报先保存再交付 | 在写入事务与最终交付都验证学习快照 |

## C 数据与持久化：经历可追溯，正文只有一个权威

**领域关系图；表结构是它的持久化投影。**

```er LR
提议快照
  proposal_id PK
  task_and_reason text
  subject_and_sources refs
纠正经历
  episode_id PK
  proposal_id FK -> 提议快照
  replacement_and_reason text
  task_conversation_outcome refs
归纳草稿
  draft_id PK
  episode_id FK -> 纠正经历
  conditions_and_uncertainty text
记忆发布
  publication_id PK
  draft_id FK -> 归纳草稿
  entry_and_revision refs
  state enum
本轮使用
  frame_id PK
  publication_id FK -> 记忆发布
  turn_and_epoch refs
```

| 记录 | 必需字段与约束 |
| --- | --- |
| ProposalSnapshot | UUID、版本、展示编号、ContextScope、目标 Agent 稳定 ID、显示名称、完整 task/why、创建时间、提议来源；保存当时实际内容 |
| CorrectionEpisode | episode_id、revision、proposal_ref、用户 action_id、反馈意图、完整替代文本、明确原因原话、关系依据、不确定项、结果引用 |
| EvidenceRef | 来源类型、ContextRef/HostActionRef/TaskReceiptRef、角色、原回合/消息定位、内容 hash、完整性、捕获时间 |
| TaskSubject | task_id、root_task_id、parent_task_id、run/session/turn、conversation_id；已知值与缺失原因分开 |
| LessonDraft | kind、statement、情境/条件/例外、支持与反例引用、差异、模型版本、输入快照；模型不得直接填写 active |
| LearningJob | job_id、输入 episode 版本、scope、epoch、状态、lease_owner/lease_seq、到期时间、尝试数、错误与输出 hash |
| LearningPublication | publication_id、目标 MemoryScope 绑定、audience、entry_id、expected_revision、正文 hash、source lineage、发布状态和真实回执 |
| LearningFrame | frame_id、consumer/session/turn、当前 ContextRef、MemoryRevision、scope epochs、实际注入的 publication/version、过期时间 |
| Suppression | tombstone_id、目标 lineage、来源身份、阻断范围、删除代次；不保留已删正文 |

**关系与来源。** intent 使用 dismissed / deferred / rejected / replaced；link_state 使用 verified / ambiguous / unrelated。明确原因与模型解释分字段。来源文本不完整时保留标记，不能用截断标题冒充全文。结果区分未派发、Accepted、Running、Agent 自报结果、宿主回执和用户评价。

**身份映射。** 原始经历沿用 ContextScope，不能从目录显示名推导身份。发布目标通过已有 MemoryAccess binding 解析成 MemoryScope。两类 project_id 不假定相等；模型只持宿主授予的 opaque scope_ref，不能自填个人或其他项目路径。

首期 audience 精确到 workspace + area，正文物理上放在对应 worktree 记忆集合中。所有模型目录、search/read 和学习召回都必须按 audience 过滤；其他画布不能因为共用同一 worktree 读取这些条目。后续推广需用户明确选择新的 audience。

**拟新增的 SQLite 逻辑表。**

| 表 | 权威内容 | 唯一性与恢复依据 |
| --- | --- | --- |
| brain_proposals | 待处理提议及不可变版本 | proposal_id；scope + 展示编号 |
| learning_episodes / learning_events | 完整纠正、来源引用和用户修订 | event_id / action_id；相同 ID 不同内容报冲突 |
| learning_drafts / learning_evidence | 待发布归纳、支持与反例关系 | draft version；独立根事项 + 关系去重 |
| learning_jobs | 可靠待处理工作 | input revision + extractor version；租约 fencing |
| learning_publications | 发布意图、状态与回执 | publication_id；entry_id 与 expected revision |
| learning_frames / learning_uses | 每轮实际接触的记忆版本 | frame_id；session/turn |
| learning_scopes / learning_tombstones | 范围绑定、epoch、停止召回规则 | scope_id；lineage 与来源唯一键 |

正式记忆正文仅以 Markdown MemoryStore 为权威。SQLite 草稿是发布前的候选；发布后不再编辑一份独立的“当前正文”。已发布条目被用户修改时，重新走 MemoryAccess 的版本写入，再更新引用和状态。

原始用户输入、原提议和任务结果属于经历证据，保留原样不等于第二份偏好正文。用于展示的发布前 diff 不是独立可编辑事实库。

**提议迁移。** 待确认队列从 JSON 一次性迁入同一 App SQLite，使“原提议状态 + 用户纠正 + job”可同事务提交。保留现有短展示编号，内部改用 UUID；旧编号只在原 scope 内解释。导入记录固定文件摘要和 scope，重复启动不重复导入；旧文件停止写入，损坏时保留并报错。

已被旧版本忽略的内容无法凭空补回。

## D 工作流：怎样关联、归纳并积累经验

**关联判定；精确引用优先，时间不能证明因果。**

```flow
(忽略或输入新方案) -> {有精确提议引用?}
{有精确提议引用?} -> 核验scope版本与原文: 有
{有精确提议引用?} -> 待关联或独立输入: 无
核验scope版本与原文 -> {用户表达什么?}
{用户表达什么?} -> 本次未采纳: 只忽略或拒绝
{用户表达什么?} -> 本事项暂缓: 晚些再做
{用户表达什么?} -> *完整替代经历: 明确改用B
待关联或独立输入 --> 核验scope版本与原文: 用户选定目标后
完整替代经历 -> 事务保存并安排归纳
```

首期支持三个入口：提议详情中“改用其他方案”、忽略后的历史详情、带提议引用的 Dock 输入。只有同 Agent、相似文字或相近时间时，保留 ambiguous，不计入已验证的偏好证据。忽略后换话题，只记录忽略。

TaskSubject 来自真实任务回执和宿主谱系。root_task_id 相同的重试、子任务与替代派发只算一个根事项；两个真实独立任务即使文本相同，仍是不同证据。缺 task_id 的显式提议纠正可以保存，但不得用新 UUID 假装独立任务次数。

**从保存到发布的时序。**

```sequence num
participants: 用户, LearningHost, BrainStore, brain归纳, MemoryAccess
用户 -> LearningHost: 纠正原提议，携带proposal_ref
LearningHost -> BrainStore: 事务保存经历、提议状态、job
BrainStore --> LearningHost: Recorded与job_ref
LearningHost --> 用户: 纠正已保存，整理中
LearningHost -> brain归纳: 输入引用与job许可
brain归纳 -> LearningHost: norma读取完整证据
LearningHost --> brain归纳: 限定范围的证据分页
brain归纳 -> LearningHost: norma提交差异或no_change
LearningHost -> BrainStore: 校验lease、输入版本与来源
BrainStore --> LearningHost: DraftSaved与发布资格
LearningHost -> MemoryAccess: 发布符合政策的反馈记忆
MemoryAccess --> LearningHost: Published或待核验
LearningHost --> 用户: 已保存反馈记忆，或有候选待确认
```

**归纳执行器的输出。** 一个 job 返回 NoChange 或 ExtractionBundle。P1 的 bundle 可同时包含一条有据事实反馈和零到一条偏好候选，各自执行发布政策；不能因产生候选就漏掉本次反馈。SkillCandidate 数组留到 P3，首期不接收能力变更。

输出必须引用读过的原文位置，不能提交任意文件路径、直接删除旧主题或设定工具权限。宿主验证引用存在、角色、UTF-8 定位、完整性与版本；语义概括仍标为模型推断。

自动发布的 EpisodeFeedback 以可核实选择为主体：“在情境 X 中，用户选 B 而非 A，明确原因 R”。没有说出的原因不补写；模型推测单列为解释，不能自动升级成“以后必须”。归纳“长期习惯”只生成待确认候选，确认后才形成 ConfirmedPreference。

**证据累计与冲突。**

| 情况 | 状态变化与后续建议 |
| --- | --- |
| 一次明确替代 | 保存本次选择；相似任务可把它作为参考 |
| 几个独立事项重复选择 | 合并同一候选的证据，提升复核排序；不靠固定次数自动变成规则 |
| 用户点“以后参考” | 保存确认来源、范围、条件、例外，发布偏好版本 |
| 同情境有相反选择，原因未知 | 候选 needs_review；相关固定偏好暂停套用，当前要求优先 |
| 用户明确说“这次例外” | 保存例外来源，不抹掉原偏好；条件不明时不套用 |
| 用户说理解错了 | 新版本 supersedes 旧版，撤销旧快照并重建相关上下文 |
| 缺原文、结果不明、关联歧义 | 明确保留 unknown；不伪造成功或稳定偏好 |

情境和自然语言方法可以由模型归纳，首期不建设全量语义归并系统。先按真实 proposal/root task 与用户确认的候选 ID 归组。模型提出跨事项合并时暂存 MergeSuggestion；用户确认或有相同确认目标才合并。无法确定是否重复，不通过换名自动启用另一条长期规则。

**job 状态机。**

```flow LR
(Pending) -> Leased: 后台原子领取
Leased -> DraftSaved: 版本与租约有效
Leased -> NoChange: 无值得保存内容
Leased -> Pending: 失败或租约到期
Leased -> Cancelled: 纠正输入更新或删除
DraftSaved -> Done: 后续发布交接已持久化
Pending -> Failed: 达到重试上限
```

建议首期每个项目最多一个学习模型调用，全局并发另有限额。初始预算建议 90 秒总耗时、最多 3 次失败尝试；这些是可配置上限，不是性能实测。前台和审批优先，学习不占审批 worker。重启按租约到期恢复；lease_seq、episode revision、epoch 任一过期，迟到提交都拒绝。

调度器只在主会话空闲时启动归纳。新的主任务到来时取消可重试的学习调用，保留 job 输入并重新排队；不取消用户已派出的 Coding Agent。退出和取消使用现有后台生命周期收口，不在 GPUI 前台 join 模型进程。

## E 发布与召回：写进去、可使用、已注入是三件事

**SQLite 与 Markdown 之间使用可恢复发布协议。**

```flow
(归纳草稿通过政策) -> SQLite预留Publication
SQLite预留Publication -> Markdown按基线提交
Markdown按基线提交 -> {结果明确?}
{结果明确?} -> SQLite确认回执: 提交成功
{结果明确?} -> 核验已发布版本: 丢回执或同步不明
核验已发布版本 -> SQLite确认回执: 正文hash与来源一致
核验已发布版本 -> 保留待核验: 无法确认
SQLite确认回执 -> *Published可召回: epoch仍有效
SQLite确认回执 -> Suppressed并清理: 期间已删除
Published可召回 -> 后续准备LearningFrame
```

1. 在 SQLite 中预留 publication_id、entry_id、expected MemoryRevision、正文 hash、来源、audience 和 epoch。草稿转 Publishing，重试复用同一操作身份。
2. MemoryAccess 调用现有 MemoryStore.commit。新增 MemorySourceReference::LearningPublication，不能冒用 UserTurn 或 SettingsEdit。来源详情由宿主映射回真实纠正经历。
3. 提交成功后，SQLite CAS 确认 Published 和实际 revision，同时递增目录代次。若此时已删除，则保持不可召回，再安排清理活跃 Markdown 条目。
4. 丢回执时，沿 CURRENT 的已发布祖先核对正文 hash、entry ID 与 publication 来源。未证明不存在前不重复提交；不能采纳孤立候选目录。

这不是跨 SQLite 与文件系统的原子事务。新增学习条目在 SQLite 仍为 Publishing/Unknown 时，所有模型读取入口必须隐藏它。现有发布 journal 提供可复用基础，但学习的 epoch、audience 和状态检查需要新增。

**唯一记忆读取规则。** MemoryAccess 对学习来源条目统一执行 visibility 检查，覆盖 activate/catalog、list/search/read、设置导出后的模型导入、历史 restore 和学习 recall。目录也须先过滤再生成，不能先展示标题再拦正文。reader 缺少 workspace/area 身份时，默认不暴露这些学习条目。

普通人工记忆继续使用现有来源规则。首期学习条目一条反馈或偏好对应一个 entry，不把无关主题自动合并到一起。人工合并若混入被撤销 lineage，整个学习条目暂不召回，直到用户重写并确认保留内容。

**编辑不能丢掉学习身份。** publication 账本按 MemoryScope + entry_id 保留条目分类、audience 与 lineage，不仅依赖正文或当前 source 数组。设置编辑、memory_write、合并、导入遇到受管学习条目，均走同一变更入口，保留适用范围；不能把来源改成 SettingsEdit 就变成所有画布可读的普通条目。原生手工新增条目不自动冒充既有学习条目。

**最后一个条目的删除。** 当前 MemoryStore 禁止把已有集合提交成空集合。P1 需为“已核验且全部属于本次撤销的学习条目”增加受控空集合发布入口，保留普通 commit 的保护。该入口带 expected revision 与明确删除 ID，不向模型开放任意 clear；否则删除最后一条学习记忆会一直停在 CleanupPending。

**如何准备本轮上下文。**

```flow
(新的用户请求或幕僚轮次) -> 宿主绑定当前事项
宿主绑定当前事项 -> 过滤scope与audience
过滤scope与audience -> 排除未发布与已撤销项
排除未发布与已撤销项 -> 词法检索与情境选择
词法检索与情境选择 -> *LearningFrame
LearningFrame -> 幕僚主会话: 本轮加载确认
幕僚主会话 -> 提议与汇报
提议与汇报 -> 宿主复核frame和epoch
```

先过滤，再检索。初版复用有界标题/描述/正文词法检索；中文定向验收不过再考虑可重建索引或 embedding。建议每轮最多 3 条、合计 8 KiB，来源按需补读，超限不把截断证据标成完整。

优先级是当前明确要求 → 同事项纠正 → 有适用条件的确认偏好 → 相似经历参考。反馈记忆以数据和引文进入上下文，不成为系统权限。LearningFrame 记录实际提供的条目、版本和来源；模型声明的 used_refs 只是解释子集，不能替代宿主清单。

每轮先准备并加载 LearningFrame，再启动推理。影响规则或删除时递增 scope epoch，取消相关旧输出，重建接触过内容的主会话。旧会话不能仅靠下一条“请忽略旧记忆”继续工作。

**与现有 worker 的握手。** 目前 brain 内部 Dispatcher 会自行排队，轮次号和 ACP session 到 worker 执行时才产生。新增队列项只携逻辑 request_id、来源 ContextRef 和 generation，不在入队前猜 session/turn。

```sequence num
participants: brain主worker, LearningHost, 后台存储, ACP后端
brain主worker -> ACP后端: 按需创建或恢复session
brain主worker -> brain主worker: 分配本次turn与attempt
brain主worker -> LearningHost: PrepareTurn请求，含来源与session/turn
LearningHost -> 后台存储: 读开关、最新epoch与可见记忆
后台存储 --> LearningHost: 授权后的LearningFrame
LearningHost --> brain主worker: frame与当轮permit
brain主worker -> ACP后端: prompt附frame与受信许可上下文
brain主worker --> LearningHost: PromptDispatched，记录实际附带版本
ACP后端 --> brain主worker: 本轮完成或失败
brain主worker --> LearningHost: TurnFinished，关闭permit
```

BrainConfig 增加可选的宿主控制往返 channel。请求 DTO 携带 role、request_id、ACP session_id、turn_id、attempt 和来源引用；回复用单次 channel。App 后台 prepare_turn 后回复，worker 只在自己的后台线程有界等待。纯生命周期握手不执行产品动作；模型读证据、提议和写入仍经 norma CLI。

启用学习但没有可用握手通道时停止这一轮并报告未就绪，不降级到无许可推理。App 验证进程注册身份后才绑定其 session/turn。重试、新建 ACP session、排队期间 epoch 变化，都须取得新 permit；旧许可关闭。PromptDispatched 仅证明 frame 随 prompt 成功提交给后端，不证明模型理解或行为改善。超时和退出也必须关闭许可。

默认只把 LearningFrame 接到幕僚主会话。审批 lane 使用独立角色凭据和既有审批协议，不等待学习握手。后续前台 Norma 接入时复用 prepare_turn/撤销 Interface，不另建学习存储。

**统一回执语义。**

| 状态 | 能向用户说明什么 |
| --- | --- |
| Recorded | 原始纠正已持久化 |
| DraftSaved | 已整理为候选；还未成为可召回记忆 |
| Publishing / Unknown | 保存结果待核验，不重复操作 |
| Published | 正文与发布账本已确认，可按范围读取 |
| FrameLoaded | 某一轮确实得到这版记忆，尚不证明行为改善 |
| Proposed / Accepted | 建议已展示 / 任务已接收；均不等于执行成功 |

## F CLI、许可、删除与用户控制

**拟新增命令与结构化回执。** 参数名用于说明设计，实施时由 clap 和 DTO 同时校验。

| 调用 | 核心参数 | 宿主执行的检查 |
| --- | --- | --- |
| norma brain learning evidence | job_ref、cursor、permit | 本 job 的精确来源、分页、scope、epoch |
| norma brain learning submit | job_ref、lease_seq、expected_revision、result_json、permit | 输入完整性、租约、来源角色、删除状态、幂等 |
| norma brain learning recall | frame_ref、可选 query、permit | 只读取已准备的候选视图，不扩张范围 |
| norma brain propose 扩展 | 原参数、subject_ref、frame_ref、permit | 仍生成待确认提议；保存实际学习来源 |
| norma brain report 扩展 | 原参数、frame_ref、permit | 写入事务及 Dock/TTS 最终交付都检查 epoch |
| 设置/提议 UI 的 control | 用户操作 ID、目标版本、动作 | 真实用户来源；模型没有 approve/delete/promote 管理入口 |

回执使用 Recorded、DraftSaved、Published、Suppressed、Conflict、OutcomeUnknown 等类型及错误参数，由 UI 翻译。保留现有 Brain 结果信封的兼容解析；新调用方按 schema_version 识别。学习已开启的会话缺 permit 或 frame 时拒绝，不能降级到旧版无学习路径。

**三类执行身份。**

| 身份 | 可以做什么 | 必须拒绝 |
| --- | --- | --- |
| Learner | 读取本 job 证据，提交归纳或 no_change | 直接 Read 项目文件、普通 shell、assign/decide/answer、发布或管理记忆 |
| LearningMain | 读取授权项目资料和 LearningFrame，propose/report | 绕过用户确认直接 assign；从学习规则获得审批或答题授权 |
| Approval | 既有审批范围和高风险规则 | 读取学习偏好、使用学习 permit 扩权 |

App 按 role/session 签发独立凭据；job/turn permit 再绑定 scope、输入版本和 epoch。长期进程 token 只确认进程身份，不能替代当轮许可。permit 通过受信执行上下文交给 worker；持久化的 prompt 快照、日志和活动页必须脱敏。模型不能仅修改 JSON 的 role 切换身份。

宿主按 E 的出队握手准备 frame 和 permit。后台排队期间 epoch 变化，按最新版本重新准备；不在模型中猜测版本。轮次结束或租约到期后关闭许可。提取器另用工具白名单与 Gate 禁直接文件读取，不能只依赖 CLI token。

**派发确认的实现影响。** 当前 assign_followups=true 时主会话能直接 Assign。启用本设计后，接触学习的 LearningMain 只能提议，即使旧自治开关开启也不例外。设置中需明确显示这一限制。前台用户明确发出的派活仍走已有用户授权链，不把后台学习提议当成同一授权。

用户确认提议时，宿主先生成稳定 dispatch_operation_id 并持久化 reservation，再接入任务协调器的幂等提交。当前提议派发路径尚未证明具备这个恢复保证，不能直接套用其他全局派发入口的结论。任务已接收而来源保存失败，只补来源；结果未知时先查原 operation_id，不自动重派。

**删除流程。**

```sequence num
participants: 用户, LearningHost, BrainStore, MemoryAccess, 旧会话与输出
用户 -> LearningHost: 删除记忆或纠正关联
LearningHost -> 旧会话与输出: 立即阻止相关输出交付
LearningHost -> BrainStore: 事务写tombstone、epoch并取消job
BrainStore --> LearningHost: 撤销已持久化
LearningHost -> 旧会话与输出: 许可失效、取消归纳、清理队列
LearningHost -> MemoryAccess: 移除活跃条目并刷新可见目录
MemoryAccess --> LearningHost: 清理完成或待重试
LearningHost -> 旧会话与输出: 重建不含旧内容的会话
LearningHost --> 用户: 已停止使用，清理状态明确
```

删除检查必须在 suggest、publication finalize、report 落盘、提议写入、Dock 显示和 TTS 播放前生效。仅拦最后的播报，会留下删除之后新写入的派生正文。已保存的派生内容按 frame lineage 清理；原提议/用户输入作为经历单独管理。

tombstone 阻止旧 source IDs 和派生 publication 再提取。已知 lineage 不得通过换名、合并或旧版本恢复绕过。内容关联无法确定时不得自动恢复固定偏好；新用户明确请求可按当前任务处理，重新永久记住需独立的恢复动作。

删除状态分 RevokePending、Suppressed、CleanupPending、DeletedFromActiveUse。事务失败显示未完成；活跃 Markdown 清理失败仍保持禁止召回，并在后台重试。不得宣称清除了外部模型已收数据、全部原始对话、旧物理备份或系统整库回滚前的副本。

普通 MemoryStore 历史版本继续服务完整性与恢复；产品级读取/导入/restore 必须保留当前删除账本。未经标记的原始 Markdown 导入无法保证识别所有改写副本，需提示用户并按新的人工确认处理。整库恢复若连 tombstone 一起回滚，不属于首期遗忘保证。

学习 trace、已保存汇报和活动详情也要登记 frame lineage。默认不额外持久化完整学习 prompt；必要的诊断内容可脱敏并按来源撤销。只清正文、忽略这些派生副本，不算完成产品级停止使用。

**开关矩阵；控制改变也属于撤销事件。**

| 控制 | 捕获/归纳/发布 | 召回与在途输出 | 重新打开 |
| --- | --- | --- | --- |
| 项目学习 Off | 停止额外学习捕获，取消未完成job；未发布草稿保留可查看 | 递增epoch，旧frame/permit失效并重建会话 | 不自动补学关闭期间的历史；待处理草稿需重新安排 |
| 项目“仅观察” | 可保存纠正与草稿，不自动发布新反馈 | 已发布记忆仍由召回开关控制 | 切到“辅助建议”后只处理重新核验过的草稿 |
| 项目“辅助建议” | 按A的政策记录、整理和发布 | 按有效frame使用 | 正常处理新材料 |
| 全局 memory.save=false | 停止新增学习来源与发布；已有业务任务历史仍按原流程保存 | 不自动关闭旧记忆召回 | 关闭时的job不自动恢复，显式重排并复核 |
| 全局 memory.recall=false 或项目“用于建议”关闭 | 可继续整理和保存，是否发布仍受save约束 | 递增epoch，撤销已加载frame、许可、Dock/TTS在途输出 | 新轮次重新准备frame，不复用旧会话 |

memory.save=false 必须在领取 job、提交归纳、预留发布、Markdown 提交前与 SQLite finalize 时复核。若关闭时文件已经提交，状态保持 Suppressed，安排清理，不把它恢复成 Published；恢复流程执行相同检查。memory.recall=false 在所有模型读取入口检查；人工查看和删除仍可用。

保留期清理先撤销对应自动反馈 publication，再清理原始经历正文。确认偏好保留独立的用户确认来源；不能删掉唯一依据却继续作为已证实规则召回。删除账本不随普通保留期回收。

**用户控制界面。** 提议详情增加“改用其他方案”和“为什么这样建议”；设置 → Norma → 记忆增加“学习记录”。展示原建议、用户原话、差异、来源、未知项和适用情境。提供纠正关联、以后参考、编辑条件、停用、删除和范围推广。

采用 apple-design 的清晰层级与用户控制原则：常用操作靠近原提议，完整证据按需展开；保存中和错误状态即时可见。gpui-component 覆盖 hover、pressed、focus、selected、disabled，浅深主题和键盘可用。系统文案中英文，用户原话不翻译。

## G 代码结构：新增哪些文件，接口落在哪里

以下是**目标结构**，不是已经创建的业务文件。标注“修改”的文件已有实现；其余按阶段新增。目录按职责组织，Rust 文件超过 500 行前拆分。

**协议与纯规则。**

```tree list
norma 协议
  `crates/norma/src/protocol/learning.rs` | 模块入口、LearningOp、协议版本
  `crates/norma/src/protocol/learning/records.rs` | 纠正、来源、草稿与引用 DTO
  `crates/norma/src/protocol/learning/permits.rs` | 轮次/job许可与LearningFrame DTO
  `crates/norma/src/protocol/learning/results.rs` | 类型化回执与错误
  `crates/norma/src/cli/brain_learning.rs` | 学习子命令参数
agent 纯规则
  `crates/agent/src/learning.rs` | 重导出协议和纯函数入口
  `crates/agent/src/learning/relations.rs` | 精确关联、根事项归组与完整性
  `crates/agent/src/learning/policy.rs` | 证据状态、冲突、发布资格
  `crates/agent/src/learning/selection.rs` | 召回优先级、预算、可用状态
```

norma 的协议模块只表达数据与基本合法性，不承载偏好业务推断。agent 的规则模块不创建数据库、不执行模型；BrainStore 在事务中调用它，避免各入口各自计算不同状态。

```rust title="agent::learning · 拟定纯函数"
pub fn classify_relation(input: &RelationInput) -> RelationDecision;
pub fn reduce_evidence(state: &EvidenceState, event: &VerifiedEvent)
    -> EvidenceTransition;
pub fn publication_policy(input: &PublicationInput) -> PublicationDecision;
pub fn select_context(input: &SelectionInput) -> LearningSelection;
```

**存储与后台推理。**

```tree list
storage
  `crates/storage/src/brain_store.rs` | BrainStore外部Interface、数据库句柄
  `crates/storage/src/brain_store/proposals.rs` | 提议队列、版本与JSON迁移
  `crates/storage/src/brain_store/episodes.rs` | 纠正事件、来源与事务归组
  `crates/storage/src/brain_store/jobs.rs` | 原子领取、租约、重试与恢复
  `crates/storage/src/brain_store/drafts.rs` | 草稿、支持/反例、用户修订
  `crates/storage/src/brain_store/publications.rs` | 发布预留、核验与回执
  `crates/storage/src/brain_store/revocation.rs` | epoch、tombstone、派生清理
  `crates/storage/src/brain_store/frames.rs` | 轮次快照、实际使用记录
  `crates/storage/src/memory_access/learning.rs` | 学习来源、audience过滤、受控发布
brain
  `crates/brain/src/learning.rs` | 学习执行器入口和取消句柄
  `crates/brain/src/learning/worker.rs` | 按需ACP任务、预算与结束
  `crates/brain/src/learning/prompt.rs` | 提取契约、来源约束、no_change
```

BrainStore 是一个存储模块，名称不代表它依赖 brain crate。提议与纠正共用数据库事务；MemoryAccess 仍是正式记忆发布和读取的入口，避免再写一套 MemoryStore。

```rust title="storage::BrainStore · 拟定Interface"
impl BrainStore {
    pub fn record(&self, event: VerifiedObservation) -> StorageResult<RecordReceipt>;
    pub fn claim(&self, request: ClaimRequest) -> StorageResult<Option<JobLease>>;
    pub fn submit(&self, result: VerifiedSubmission) -> StorageResult<DraftReceipt>;
    pub fn publication(&self, step: PublicationStep) -> StorageResult<PublishReceipt>;
    pub fn query(&self, request: AuthorizedQuery) -> StorageResult<LearningView>;
    pub fn control(&self, action: VerifiedControl) -> StorageResult<ControlReceipt>;
}
```

ClaimRequest 显式传入 now、owner 和预算，测试可控制时间。所有方法有阻塞 IO，调用者必须放到后台。跨方法的一致性约束在存储内部实现，不能由 UI 通过调用顺序碰运气。

**宿主、CLI Adapter 与界面。**

```tree list
App宿主
  `crates/app/src/app/learning.rs` | LearningHost小接口与进程内生命周期
  `crates/app/src/app/learning/capture.rs` | 从提议/UI/真实回执绑定证据
  `crates/app/src/app/learning/scheduler.rs` | 项目队列、公平调度与重启恢复
  `crates/app/src/app/learning/publication.rs` | 协调BrainStore和MemoryAccess
  `crates/app/src/app/learning/context.rs` | 准备frame、注入和可用版本
  `crates/app/src/app/learning/permits.rs` | 角色、scope、job/turn许可
  `crates/app/src/app/learning/revocation.rs` | 会话、输出队列和缓存失效
CLI与UI Adapter
  `crates/app/src/norma/endpoint/brain_learning.rs` | 学习命令异步端点
  `crates/app/src/settings/norma/learning.rs` | 学习列表与详情入口
  `crates/app/src/settings/norma/learning/detail.rs` | 原文、差异、来源、状态
  `crates/app/src/settings/norma/learning/actions.rs` | 用户确认、纠正与删除
```

LearningHost 放在 app 的独立职责目录，而非挤进已有 brain_host；它同时服务幕僚、设置和后续前台对话。只有有多个实际调用者的 Interface 才公开，内部调度细节保持私有。

**已有文件的修改清单。**

| 文件 | 预期修改 |
| --- | --- |
| `crates/norma/src/protocol/brain.rs` | LearningOp 分支，propose/report 的 frame 与许可引用 |
| `crates/norma/src/cli/brain_command.rs` | 挂载学习参数与 permit，拆出子命令实现 |
| `crates/agent/src/memory.rs` | LearningPublication 来源类型 |
| `crates/storage/src/memory_access/mod.rs` | 统一可见视图，过滤 catalog/list/search/read |
| `crates/storage/src/memory_access/publication.rs` | 扩展发布来源与学习恢复资格 |
| `crates/storage/src/migrations.rs` | 注册学习表迁移，不阻塞输入热路径 |
| `crates/brain/src/service.rs` | 可取消学习执行器与角色环境 |
| `crates/brain/src/acp/gate.rs` | Learner 专用工具/命令策略 |
| `crates/brain/src/lane/worker.rs` | 主轮次绑定 permit/frame，处理失效重建 |
| `crates/brain/src/event.rs` | 纯数据轮次状态，记录PromptDispatched与TurnFinished |
| `crates/app/src/app/brain_host/startup.rs` | 创建宿主控制往返channel，绑定学习生命周期 |
| `crates/app/src/app/brain_host/config.rs` | 各role独立启动凭据，不再共用一个环境token |
| `crates/app/src/app/brain_host/lifecycle.rs` | 学习取消与后台退出收口，保留持久化待恢复job |
| `crates/app/src/app/brain_host/proposals.rs` | 变为存储视图与展示状态，保留现有文案验证 |
| `crates/app/src/app/brain_host/proposal_actions.rs` | 异步保存反馈，稳定确认派发身份 |
| `crates/app/src/app/brain_host/ops.rs` | 学习会话禁止直接派发，统一许可验证 |
| `crates/app/src/norma/endpoint/brain_report.rs` | 保存和交付两次核验，失效派生清理 |
| `crates/app/src/app/memory.rs` | 传入模型reader身份、加载代次和撤销状态 |
| `crates/app/src/settings/norma/revocation.rs` | 把学习删除与现有前台会话失效接通 |
| `crates/storage/src/memory/mod.rs` | 受管学习清理的空集合发布，普通commit保护保留 |
| `crates/storage/src/memory/revision.rs` | 允许受管空集合的发布/读取校验，历史链仍可核验 |

后续功能按真实需求再加文件：P2 跨事项合并与可选自动采用；P3 Skill draft/evaluation/accept；P4 多入口与个人范围。首期不为这些阶段创建空实现或通用插件 trait。

## H 实施顺序、验收与待评审选择

```timeline
P1a | 可靠经历 | 协议、提议迁移、精确关联、幂等和job
*P1b | 完整学习 | 反馈发布、实际召回、角色许可、管理与删除
P2 | 经验整理 | 跨事项合并、冲突处理、成熟后再开放自动固定习惯
P3 | 方法成长 | Skill候选、代表任务回归、版本启用和回退
P4 | 范围与入口 | 显式推广、语音/手机/终端真实回合；前台共用
```

P1a 是内部里程碑，不单独宣称已完成自学习。推荐首阶段是 P1a + P1b：明确关联的纠正能保存、形成受控反馈记忆、重启后召回、改变建议，并能纠正和删除。正式 MemoryStore 发布和全读取路径过滤属于 P1，不推迟到已经启用学习之后。

**逐项验收映射；表中是未来计划，尚未执行。**

| 场景 | 正向证据 | 必须同时验证的反例 | 层次 |
| --- | --- | --- | --- |
| 忽略后换话题 | 忽略动作和原提议持久保存 | 新任务不被绑定为替代，不出现新偏好 | agent + storage + App/E2E |
| 同任务明确替代 | 原文、理由、替代、原因、来源均可恢复 | 不自动派发，不把原因未知填成猜测 | norma + storage + E2E |
| 暂缓/拒绝/替代 | 意图和关系分别保存 | 只说下周做不形成反对重构的习惯 | agent + App |
| 跨项目/画布隔离 | 本 scope 可召回 | 其他 scope 的 catalog、search、read、frame 均无泄漏 | storage + agent + App |
| 独立证据计数 | 独立根任务可分别保存 | hook重发、子任务、重试和改写不重复计数 | storage + App |
| 重启和租约 | Pending与过期Leased可恢复 | 旧lease迟到提交不能覆盖新结果 | storage + brain |
| 发布丢回执 | 已发布祖先可核验并补回执 | 不生成重复条目，不读取孤立候选目录 | storage |
| 发布途中删除 | 立即禁止可见，后续清理完成 | 提交后的旧正文不能因finalize迟到变Published | storage + App |
| 关闭开关与发布竞态 | 关闭后阻断相应阶段，开关状态跨重启保持 | 已提交文件不能迟到finalize为Published，旧frame不能交付 | storage + App/E2E |
| worker授权握手 | 每个实际session/turn在推理前获得新frame与permit | 排队旧版本、重试、会话重建不能复用旧许可 | brain + App |
| 同时形成反馈与候选 | 一个bundle可靠保存两种产物 | 候选待确认不能阻止事实反馈的独立发布 | agent + storage + App |
| 删除最后一条记忆 | 可发布受管空集合并完成活跃清理 | 普通模型写入仍不能任意清空集合 | storage |
| 人工编辑学习条目 | 正文更新，audience和lineage仍受控 | 替换source或合并不能扩大读取范围 | storage + App |
| 召回真实使用 | frame记录确切条目与版本，建议保留发版条件 | 不以prompt包含一句话冒充行为改善 | brain + App/E2E + 模型样例 |
| 冲突与用户修正 | 保存反例，新版本替代旧版 | 不简单采用最后一句覆盖全部情境 | agent + storage + App |
| 旧token与旧frame | 新轮次可继续 | 删除后旧submit/report/propose不落盘、不交付 | norma + App/E2E |
| 自治已开启 | 学习主会话仍只生成提议 | assign_followups=true不能绕过确认 | brain + App/E2E |
| 学习越权 | job只读授权证据 | 直接Read、shell、decide、answer与管理命令被拒 | norma + brain + App |
| 任务已接收后保存失败 | operation_id指向原任务，可补来源 | 不恢复为待派发并重复执行 | storage + App/E2E |
| 删除后重学/恢复 | lineage与来源抑制生效 | 重放、恢复、已知派生副本不能复活 | storage + agent + App |
| 开关与界面 | 保存/读取开关、状态和错误可见 | 全局memory.save/recall关闭时项目设置不能绕过 | ui + App/E2E |

单元测试按 crate 放在 unit 目录，集成测试跨公开 Interface：`crates/agent/tests/learning_rules.rs`、`crates/storage/tests/brain_learning.rs`、`crates/storage/tests/learning_publication.rs`、`crates/brain/tests/learning_worker.rs`、`crates/norma/tests/brain_learning_cli.rs`。App 测试放 `crates/app/tests/unit/brain_learning_tests.rs`，按来源、撤销、派发拆职责。

真实场景放 `test/desktop-e2e/Scenarios/brain/BrainLearning.swift`，由 brain 模块清单注册，配套该模块的夹具与检查清单。先失败测试再实现；真实 App 检查原提议、学习详情、下一条建议和目标 Agent 是否收到输入。夹具模型用于边界；少量真实模型轨迹验证归纳和相似情境效果，两者不相互替代。

按实际改动执行受影响 crate 测试、App check、跨 crate 验证和 GPUI release 构建；执行 i18n、存储/性能、行数和 no-allow 护栏。此轮只写设计文档，不把上述未来检查标成通过。

**建议首期用户操作体验。** 原提议详情中输入替代方案后，立即看到“保存中 → 纠正已保存 → 反馈记忆已保存”。“以后参考”只在候选确有内容时出现，旁边展示适用范围。删除显示停止使用与清理状态。不要让用户接触 job、lease、epoch 或数据库术语。

**本次评审的三个选择。** 星标是推荐方案，不代表已经批准。

```ask
首期自动学习到什么程度？
* 自动存事实，习惯需确认 | 明确纠正自动成为情境反馈；长期规则确认后固定。
- 只整理候选，不自动发布 | 所有跨任务使用都需要“以后参考”。
```

```ask
首期记忆对哪些事项可见？
* 当前画布区域 | 与现有ContextScope一致，明确推广后才共享。
- 同一工作树共享 | 更快复用经验，需要同时审查跨画布来源展示与隐私规则。
```

```ask
首期是否同时做Skill成长？
* 先完成反馈记忆与召回 | P1做完整可用流程，Skill留到具备评测时。
- 同时建设Skill验证与启用 | 扩大到P3，需先定义代表任务和成功标准。
```

**评审建议。** 先看 A 产品行为、B 模块职责、G 代码结构，再检查 E/F 发布和删除契约。需要调整时可按章节评论；页面中的选择只形成评审反馈，不修改 App 设置，也不启动实现。
