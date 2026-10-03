# AgentConnect 记忆与做梦机制：源码调查

## 结论

最吻合用户描述的候选是 **[agentconnect-md/agentconnect](https://github.com/agentconnect-md/agentconnect)**：它确实实现了跨会话记忆、后台蒸馏、定时做梦和待审核 Skill 生成。用户没有提供仓库链接，因此**尚不能确认它就是用户所指的 AgentConnect**。另有 [AKKI0511/AgentConnect](https://github.com/AKKI0511/AgentConnect)，官方描述为独立 Agent 的去中心化协作框架；不能把两者能力合并。候选官方主页是 [agentconnect.md](https://www.agentconnect.md)，仓库许可证是 Apache-2.0。[S1]

值得 Norma 借鉴的是「有来源的持久记忆 → 增量提取 → 后台重整提案 → 审核/采纳 → Skill 候选」闭环。第一版无需复制它的 Control Plane、分布式迁移或外部插件平台。它的默认记忆按 Agent 共享，**不等于用户已选择的个人偏好＋隔离项目记忆**；隔离维度必须由 Norma 自己定义。[S2][S3]

## 调查边界与版本

- 检索日与文件命名使用用户侧日期 `2026-10-02`；来源版本固定为实际下载并核对的 commit **`478aab4e0aceb2127acc53429236d25e74515ed6`**，不是把用户侧日期当项目更新时间。
- GitHub API 返回该 commit 的 committer 时间为 `2026-10-02T15:33:59Z`，提交说明是 `fix(compose): upgrade bundled Logto to 1.44.0 (#2769)`；本文所有源码链接固定到该 SHA。
- 证据来自官方仓库 README、设计文档与实现代码；未安装运行 AgentConnect，未执行其测试，也未验证真实 Mem0 Cloud 账户。因此「已实现」指存在可追踪代码，不意味着本文完成了产品 E2E 验收。
- 本文只解决「可借鉴的记忆实现」调查，不替用户决定设置页面视觉稿或最终架构。

## 真实实现

### 1. 四个 backend，不能混为一种记忆

| Backend | 已实现行为 | 与 Norma 的关系 |
|---|---|---|
| `managed`，默认 | 自有 Markdown 主题记忆、索引、工具、历史、蒸馏与 Dream | 最适合参考闭环和用户可见性 |
| `native` | 使用 Claude/Codex 等 harness 自己的记忆；AgentConnect 不搬迁或展示这些内容 | 不能当成 Norma 自有记忆数据库 |
| `external` | 通过独立进程 MCP profile 对接外部记忆；每轮召回和延后捕获 | 需要语义召回或接外部服务后再引入 |
| `none` | 关闭持久记忆，包括已验证的 harness 原生记忆开关 | 设置页必须明确「关闭」究竟影响哪些存储 |

Provider 选择保存到 session，切换时重建 harness/session 边界，避免两套记忆同时注入；外部 backend 的账号、endpoint、密钥没有直接暴露给模型。记忆正文可以由 daemon 磁盘持有，也可以由 Control Plane 的 managed home 持有；「AgentConnect 记忆永远只在本地」并不准确。[S2][S4]

### 2. Markdown 主题文件＋轻量目录，按需读正文

daemon 本地布局为 `~/.agentconnect/agents/<agent-id>/memory/`，与 workspace 分离。`MEMORY.md` 是索引，其他平铺 `.md` 文件是主题。主题 frontmatter 可包含 `name`、`description`、`type`、`modified`；`type` 为 `user / feedback / project / reference`。**type 是内容分类，不是访问隔离范围**，标成 `project` 不会自动实现项目隔离。[S2][S5]

legacy `readIndex` 注入上限是 **25,000 bytes**；单个主题写入上限 **256,000 bytes**。新统一接口还支持有大小边界的 session activation catalog，供模型先浏览摘要再读条目；文档明确连续刷新仍未完成。这个模式避免把整个长期记忆库塞进每轮提示词。[S2][S6][S7]

### 3. managed 不是向量 RAG，也没有已完成的 BM25

`ManagedMemoryProvider.recallForTurn()` 直接返回 `[]`；managed 的召回主要通过会话开始的索引/catalog 和模型主动调用记忆读取/搜索工具。统一 managed 搜索的当前实现是**空白分词、大小写折叠、每个 term 必须匹配、标题/描述/正文加权的子串搜索**，标题权重 3、描述 2、正文约 1；不是 BM25、embedding 或向量数据库。[S3][S8]

`memory-evolution.md` 把 BM25 列为后续升级，把 managed 向量检索明确排除在该版范围外。不能因为它接口里有 `recall`、`semantic` 等词，就宣称默认实现具有语义召回。[S9]

external provider 才会按当前输入调用 backend 的 recall，scope 由可信 Agent ID 生成。核心再次校验 scope、重复 ID、条目/字节预算，并把结果放进「untrusted reference only」数据区块。内存中的事实不能因此获得系统指令或工具授权地位。[S4][S10]

### 4. 自动蒸馏：回答之后提取耐久事实

`recordTurn` 只有在 `autoDistill` 开启且 extractor 存在时执行；代码默认回调为 false。模型读取已完成的输入/输出，把稳定偏好、纠正、决策、约定、精确名称和数值写入主题，跳过寒暄、临时进度、秘密和已存在事实。结果是 memory tool 写入，模型文字回复被忽略。[S3][S11]

「只追加、不改写/删除」是蒸馏 system prompt 的规则，不能夸大成普通文件 writer 的结构性 append-only 保证。writer 仍能替换文件，因此 Norma 若需保证旧事实不被错误覆盖，应让领域层验证 proposed mutations，不能仅靠提示词。[S11][S6]

提取需要 harness 具有已验证 `read-only` 或 `plan` 模式；不存在该模式就不执行。managed memory home 不可达时复用 durable capture outbox，后台恢复后重试，不应阻塞给用户的答复。[S11][S12]

### 5. 做梦：后台整理已有经验，生成暂存替换方案

实际链路如下：[S13][S14]

```text
记忆快照 + 本 Agent 参与过的近期会话
                  ↓
独立只读 extraction session
                  ↓
合并重复 / 处理矛盾 / 保留精确事实 / 删除临时信息
                  ↓
空 staging 中写完整主题集合 + 生成索引
                  ↓
格式与预算校验 → completed 提案
                  ↓
人工采纳，或配置允许的自动采纳
                  ↓
版本检查 → 替换 live store → 保留上一版备份
```

Dream 的「做梦」是一次 LLM 离线记忆重整，**不是模型权重训练，不会凭空增加工具能力，也不能证明智能水平持续上升**。Dream prompt 允许合并、改写和删除，但只写 staging；未写入 staging 的 live 文件在采纳时会删除。空结果不能把已有主题库全部清空，runner 有相应拒绝分支。[S14][S15]

每个 Agent 同时只允许一个 Dream。默认选取上次成功 Dream 基线之后有活动的 sessions，最多 **100 个**；边界时间用 `>=`，防止同毫秒活动漏掉。注意：若积压超过 100 个，代码明确承认旧的超额 sessions 会被跳过，基线推进后不会自动补处理；这是有上限的策略，不是完备的消费游标。[S13]

采用方案时验证已审核 staged bytes 的 digest，并比较 live store 与快照 digest。期间若仅有 distill 追加，可尝试 rebase；若有人工/工具写入冲突，自动采纳不会强行覆盖，提案留待审核。替换前保留备份，失败会尝试恢复；当前仅保留本次采纳的上一版备份，不能据此宣称无限版本一键回滚。[S16]

### 6. 默认策略必须以代码为准

该 SHA 的 `DEFAULT_MEMORY_DREAMING_POLICY` 明确为：

```ts
{ enabled: true, schedule: '0 4 * * *', autoAdopt: true, mineSkills: true }
```

每日 04:00 使用 daemon 本地时区，除非提供 IANA timezone。显式 enabled policy 没有 schedule 则是手动模式；channel scope、native、external、none 都不支持这套 Dream。[S17][S18]

官方 `product-conventions.md` 仍写「默认完成后待审核」，与当前 schema/source 的 `autoAdopt: true` 冲突。本文以可追踪源码为当前行为依据；**不能把旧文档当作默认安全策略**。Norma 的默认行为应按用户已确认的「自主学习，新增能力需确认」单独确定；涉及删除或合并用户记忆的 Dream 首版建议保留 diff 审核，而不是照抄 auto-adopt。[S17][S19]

### 7. 由经验生成 Skill：候选需审核，不能自动安装

`mineSkills` 从跨会话重复步骤中生成 Skill 候选，要求至少 **2 个不同会话**作为来源，单次至多 **5 个** Agent-local candidates；通常包含 `SKILL.md`，可携带至多 4 个平铺脚本。候选结构与字节边界由代码验证；「这个流程有效」本身仍需执行验证，来源引用不是行为测试。[S14]

Skill acceptance 是独立显式接口，发布 immutable、digest-addressed bundle，用原子替换的 `accepted-skills.json` 选择活动修订。memory autoAdopt 不会自动安装 Skill。这种「提出 → 审核已见字节 → 启用」流程与用户授权边界吻合。[S20]

### 8. 隔离与遗忘的实际边界

- managed 默认 `agent` scope：同一个 Agent 的所有使用者共享该库。可选 `channel` scope 采用「频道写入层＋Agent 只读共享 base」，频道内容不写入其他频道；同名主题由更具体层覆盖。**channel 没有 Dream**。[S3][S17]
- external v1 scope 当前是 Agent-only，不具备本文可证实的个人/项目分层。不要把接口声明的 user/session/shared 枚举误认为所有维度都已落地。[S4][S9]
- 当前 `DreamRunner.reserve` 明确读取本 Agent 参与过的各种会话，**不应用 per-turn capture-visibility gate**。对个人隐私内容不要进入共享 Agent/组织知识的限制主要写在 Dream prompt；「Dream 永远硬排除 private transcripts」在该 SHA 已不成立。来源本身仍按 Agent ID 限定，防止读入另一个 Agent 未送达的会话。[S13][S14]
- managed 具备能力发现与条件删除；删除 channel override 可能重新显露 shared base 条目。managed `.history` 保存每文件最新 100 条、总 2 MiB；正文快照最多 4,000 bytes，因此历史不是完整备份。[S6][S21]
- 在所查 memory/dream 实现中，没有建立「删除事实后历史、转录、Dream 输入、备份、外部副本也全部删除且永不再提取」的可验证保证。**普通 delete 不能等同完整遗忘**；Norma 必须单独规定 purge/tombstone、来源回溯和再提取抑制。

## 对 Norma 的建设建议（建议，非已实现事实）

**修订结论（2026-10-03）：长期记忆以 Markdown 为权威，SQLite 保留已有会话经历、Dream 运行记录，以及确有需要时可重建的辅助索引。** 上一版因 Norma 已有 SQLite 就直接推荐它保存记忆正文，跳过了 AgentConnect 已验证的「主题 MD＋生成目录＋完整暂存重建」路径，依据不足。以下建议已纠正；MD 与数据库不保存两份可以各自修改的长期事实。

| Norma 需要的能力 | 最小实现建议 | 不直接照搬的原因 |
|---|---|---|
| 个人偏好＋隔离项目记忆 | `personal` 与可信 `project_id` 两个目录范围；同一项目的 worktrees 共享该项目记忆；会话仅读 personal＋当前项目，跨项目推广显式执行 | AgentConnect 的 Agent/channel scope 不表达这个权限模型；工作路径也不能直接等同项目身份 |
| 持久化与可编辑 | 主题 `.md` 保存事实与来源，frontmatter 保存摘要/分类；宿主生成 `MEMORY.md`，设置页编辑主题后重新发布一致版本；SQLite 不作为正文权威 | 参考对象已有可读、可审核的 MD 流程；Norma 的 `agent` 仍不得反向依赖 `storage/app/ui` |
| 召回 | 会话开始读取有预算的目录，按 description 选择主题，再通过受 scope 约束的工具读正文；先采用现有可行的词法查询，不预设向量库 | AgentConnect managed 的每轮自动 recall 为空，按需读取才是主要路径 |
| 自动学习 | 完成轮次送达之后后台提取；稳定事实与 source turn/session 一起写入 MD 候选，由宿主校验并更新目录；只追加规则由代码验证 | 单靠 distiller prompt 没有确定性写入保证；不把完整转录直接当长期事实 |
| 做梦 | 从 MD 快照＋受 scope/隐私约束的经历生成完整 staging；展示新增/修改/删除 diff，采纳前同时检查 live 与已审核提案 digest，冲突保留重审 | 宿主需要协调多文件与并发发布；macOS 不能照抄固定凌晨 cron 或丢弃超额经历的窗口策略 |
| 自进化 | 已授权范围内总结经验；Skill 草稿→实际验证→用户启用；新增工具/网络能力/权限仍需确认 | 从经验生成文本不是模型训练，也不是工具授权 |
| 可解释与有温度 | 让用户看到「记住了什么、来自何处、为何本次用到」，并支持纠正、撤回、私密轮次 | 温度应来自稳定偏好和体贴行为，不能靠虚构记忆与自称有意识 |
| 真正遗忘 | 从活动 MD 删除事实，同时清理/抑制历史、备份、来源衍生物与未完成任务；索引从剩余 MD 重建 | 删除 SQLite 辅助索引或单个主题都不构成完整遗忘 |

这些建议遵循当前 [AGENTS.md](../../AGENTS.md) 的依赖方向、SQLite 热路径和 GPUI 前台禁阻塞约束。解析/目录生成/候选校验可留在 `agent` 的纯逻辑；文件读写、模型调用、发布协调由宿主后台执行，SQLite 接线复用现有存储适配。本文未验证 Norma 的实际接线点；应与单独的项目现状调查合并后再定文件与类型。

## 2026-10-03 聚焦复核：MD 权威如何贯穿读取、学习和做梦

本轮复用同一 SHA 的本地源码 `/tmp/agentconnect-source-20261002`，没有重新下载 main 或把新版行为混入此前事实。下表是可直接阅读的绝对路径，定位均已重新检索。

| 路径与定位 | 所负责的真实机制 |
|---|---|
| [frontmatter.ts](/tmp/agentconnect-source-20261002/packages/daemon/src/memory/frontmatter.ts:1) | 主题头部；description 是目录召回线索，type 是内容分类 |
| [store.ts](/tmp/agentconnect-source-20261002/packages/daemon/src/memory/store.ts:276) | `ensureMemory` 建目录；288 读索引；622 写主题；737 重生成索引 |
| [managed.ts](/tmp/agentconnect-source-20261002/packages/daemon/src/memory/providers/managed.ts:64) | 可信范围与读取 overlay；111 注入索引；131 `recordTurn` |
| [activation.ts](/tmp/agentconnect-source-20261002/packages/daemon/src/memory/entries/activation.ts:5) | 8,192-byte context catalog，不是所有正文 |
| [distill.ts](/tmp/agentconnect-source-20261002/packages/daemon/src/memory/distill.ts:13) | 增量提取规则、只读模式和有预算的输入 |
| [daemon.ts](/tmp/agentconnect-source-20261002/packages/daemon/src/daemon.ts:7418) | post-turn 调度；15867 送达后排队；8038 Dream 只读 gate |
| [runner.ts](/tmp/agentconnect-source-20261002/packages/daemon/src/dream/runner.ts:608) | reserve/snapshot；712 提取与 staging；1217 采纳；1436 rebase；1567 Skill 接受 |
| [DreamPanel.tsx](/tmp/agentconnect-source-20261002/packages/web/src/components/console/DreamPanel.tsx:604) | live＋staged 路径并集、逐文件 diff、review token 与采纳 |
| [dream-skills.ts](/tmp/agentconnect-source-20261002/packages/daemon/src/skills/dream-skills.ts:188) | 独立 immutable Skill bundle 发布，非记忆采纳 |

### A. 索引是导航，主题 MD 才保存长期内容

`writeMemoryFileHoldingLock` 规范并 stamp 主题 frontmatter，写入正文，记录 write marks/history，再生成目录。`renderMemoryIndex` 把主题的 `name/description` 渲染为 Markdown 链接；排序同时考虑 name 与文件名，保证 staged/live 使用相同 renderer 时可得到相同索引。没有任何描述的 legacy 手写索引有保留路径，不会凭空重建为空。[S22]

新 session 可以拿到旧 `MEMORY.md` 注入与新 catalog；catalog 的 `context({maxBytes:8192})` 明确提示数据可能不完整，必须通过 `listMemoryEntries/getMemoryEntry` 刷新后再依赖正文。`managed.recallForTurn()` 不自动把全部主题附加到每轮输入。Norma 应复制的是「小目录 → 找主题 → 按需读正文」，而非把所有 MD 每轮拼起来。[S3][S7][S22]

### B. 蒸馏来自已经送达的经历，结果落在 MD

`Daemon.finalizeDelivery` 先完成用户可见答复/continuation，再调用 `queueMemoryPostTurn`；该函数不等待昂贵 managed extraction 完成，按 Agent 串联后台任务。真正处理时读取可信会话范围并调用 provider。新的会话读取索引前会等待此前 post-turn chain，避免读到尚未提交的提取结果；home 暂不可达时转 durable outbox。普通 per-turn 捕获仍有 private 等 exclusion gate，与此前已记录的 Dream corpus 策略不同。[S23]

managed 的 `recordTurn` 不返回一个“记忆 JSON”给数据库存储，而是构建 distillation prompt，让专用提取 session 调用绑定范围的 memory tools 写 MD；其文字回复没有产品作用。事实进入主题后，由 writer 派生 index。Norma 的 SQLite 会话记录可继续作为经历来源，但长期事实应经过蒸馏后落在 MD，不能把转录表改名为记忆库。[S3][S11][S22]

**实际调用链（文件路径均相对固定源码根）：**

```text
daemon.ts:14005 调用 finalizeDelivery (:15818)
  → :15870 答复已送达后 queueMemoryPostTurn (:7418)
  → :7437 isCaptureExcluded；:7453 recordTurnForBinding
  → memory/providers/dispatching.ts:122 按本轮绑定选择 provider
  → memory/providers/managed.ts:131 recordTurn
  → memory/distill.ts:50 buildDistillationPrompt
  → daemon.ts:1100 注入的 extract → :7540 runMemoryExtraction
  → :7554 runMemoryExtractionPass
      :7577 register 仅 MEMORY_TOOLS，memoryBinding=可信 scope/source:distill
      :7611 验证并设置 read-only/plan；:7639 host.prompt
  → mcp/ops/memory.ts:253 writeMemory
      memoryScopeFor + enforceBindingPolicy；:296 deps.memory.write
  → memory/providers/dispatching.ts:151 write → managed.ts:196 write
  → memory/store.ts:622 writeMemoryFileHoldingLock
      ├ 有 atomicTransaction/stageTransactionFile → atomic-write.ts:32/54
      └ 普通文件 → fs.ts:267 单文件发布 → history → store.ts:737 重生成索引
```

这里的 “read-only” 是限制 harness 原生副作用；绑定的专用 memory MCP 写入仍被允许，因此才能在受控范围内提取 MD。蒸馏的 `additive only` 仍是提示词规则，MCP writer 的源码允许完整替换/精确替换；不要将其表述为底层已禁止删除。[S27]

### C. 做梦是对一个完整 MD 集合的暂存重建

| 阶段 | 已复核实现 | Norma 适配要点 |
|---|---|---|
| 快照 | `reserve` 在范围锁内读取 live 主题与 write marks，保存 snapshot digest | 每个 personal/project scope 单独快照；来源不能跨项目捞取 |
| 只读提取 | memory snapshot 和 session 文件先保留在 callback，验证 harness read-only/plan 后才 materialize | 本轮只读既有 MD/经历；仅授予写入候选区的专用 memory tools |
| 完整 staging | `run` 建空 staged store，模型逐主题写入所有保留文件，索引由同一 renderer 生成 | 原样保留未改主题；缺失主题意味着拟删除，不能当作增量补丁 |
| 校验 | runner 检查 staged topic 是否来自绑定 memory tool；没有写主题却有旧主题时拒绝全删 | 校验路径、UTF-8、frontmatter、预算、来源范围和完整候选集合 |
| 审核 | UI 列出 live 与 staged 的路径并集，以空文本展示新增/删除；提供逐文件 `LineDiff` | 必须显示删除项；记录审核对象 digest，不只记“点过同意” |
| 采纳检查 | review token 检查 staged bytes；snapshot digest 检查 live 是否改变 | 任一改变就保留重审；不强推覆盖人的修改 |
| rebase | 同一 daemon generation 的 write ledger 证明仅 distill 写入时，按行找新增内容并去重重放 | 这是受限文本算法，不是语义三方合并；首版可以不实现 rebase，冲突后重跑/重审 |
| 发布与备份 | 准备 replacement，live 移到 backup，再把 replacement 改名为 live；异常尝试恢复 | 所有宿主 writer/readers 协调，并设计崩溃恢复；不声称一条 rename 提交所有文件 |
| Skill | mining 读取工具轨迹，候选至少 2 次不同会话；`skillAccept` 校验已审 digest 并独立发布 | Skill 草稿与记忆 diff 分开审核/验证/启用；采纳记忆不授予新能力 |

以上对应 runner `608/712/1217/1436/1567`、daemon `8038` 和 DreamPanel `604/726`。[S13][S14][S15][S16][S20][S24][S25][S26]

**实际 Dream 调用链：**

```text
定时：scheduler/dream-scheduler.ts:33 sync / :40 Cron 回调
  → daemon.ts:4273 onFire → :23226 onDreamScheduleFire → :23271 runner.start
手动：cp/control/dream.ts:28 dreamStart
  → cp/dream-reader.ts:52 createDreamReader.start → DreamRunner.start
两者汇合：dream/runner.ts:581 start → :608 reserve
  → :647 withMemoryDirLock 快照 / digest / write marks
  → :662 store.insertDream（状态，不是 staged 正文）
  → :676 后台 run (:712)
      :728 dreamTranscriptText；:753 materializeInputs callback
      :794 extractWithBackstop (:970) → deps.extract
  → daemon.ts:8432 注入 runDreamExtraction (:7826)
      :8006 register，root=context.stagedStore，source=dream，topicPattern
      :8038 只读 gate → :8045 materializeInputs → :8068 runDreamExtractionSession
  → 模型写 memory tools → 同一 MD writer，但目标为空 staging
  → runner.ts:867 校验 bound topics；:880 拒绝空库；:1005 stage / :1026 生成索引
  → :907 finish(completed) → UI DreamPanel.tsx:604 路径并集 / :726 LineDiff
  → cp/dream-reader.ts:79 adopt → runner.ts:1217 adopt
      Agent 锁 → :1248 review digest → :1271 memory-dir 锁 / live digest
      :1283 rebaseDistillWrites (:1436) 或冲突拒绝
      :1333 live→backup → :1335 replacement→live → :1371 updateDream(adopted)
Skill 另一路：cp/dream-reader.ts:135 skillAccept → runner.ts:1567 skillAccept
  → dream-skills.ts:188 publishAcceptedDreamSkill → 活动 Skill index
```

这说明 Dream model 写入的是绑定后的 MD staging，而 SQLite/LocalStore 保存 job 的 status、snapshotDigest、executionSessionId、skills 状态与 usage；`runDreamExtractionSession` 另记录执行 session/transcript。采纳记忆与接受 Skill 是不同入口。[S28][S30]

**原子性的边界：** 当前文件 home 的采纳调用先 `rename(memory, backup)`，再 `rename(replacement, memory)`，这是两个操作；它利用 Agent/目录进程锁防止自家 writer 插入，并用异常回滚保护已知失败。不能由“rename 是原子的”推出“多文件集合切换及任意 reader 观察永远原子”，更不能推出进程崩溃后的完整事务保证。普通 topic＋生成 index＋history 的兼容文件写入也存在多个 awaited writes；home 有单独事务能力时才走 atomic port。[S22][S26]

| 原子/协调机制 | 源码实际保证与限制 |
|---|---|
| 本地单文件 | `LocalMemoryFs.writeFile` 调 `atomicWriteContainedMemoryFile`：独占随机临时文件，条件版本复验，再 rename 发布一个文件；不能覆盖整个主题＋index＋history 集合；该函数没有 fsync 调用，不能由此声称断电持久性保证 |
| 进程内范围锁 | `memoryDirLocks` 是 `Map<string, Promise>`；DreamRunner 另有 per-agent promise lock。保护经同一宿主 writer 路径的临界区，不是跨进程 OS 锁，也不能自动阻止外部编辑器 |
| 事务 home | `store.ts:645` 检查 atomic ports 后走 `atomicMutateMemoryFileHoldingLock`；snapshot 后准备主题/索引的 revision/staged changes，使用 operationId＋expected root revision 提交；未知结果只重放同一操作，不改用普通无条件 writer |
| CP 事务服务 | `CpMemoryFs.atomicTransaction` 经 feature negotiation 发送 `memory/transaction/v1`，由 `AgentMemoryTransactionService` 验证范围；`PgAgentMemoryTransactionRepo` 在 PostgreSQL transaction 与 advisory lock 中校验并发布 topic/index、history、receipt，跨 daemon 的排他靠 DB 实现 |
| Capture 抑制 | CP 事务 repo 根据同 sourceTurnId 已完成 tool/console mutation，拒绝后来的 distill commit；managed.recordTurn 也先询问可用的 captureStatus。此能力是 negotiated home feature，不能当成所有本地文件都有的保证 |
| 运行状态 | daemon LocalStore 的 `dreams`/`dream_runs`/`memory_capture_outbox` 保存生命周期与待处理工作；它与 CP PostgreSQL 保存 managed 正文是不同角色。数据库存在不意味着所有记忆正文默认在 SQLite |

因此 AgentConnect 的 **Markdown 内容模型**与**可选数据库 home/事务协调**并不矛盾：它的 CP home 实际可以在数据库保存逻辑 MD 文件；Norma 本次选择「本地 MD 正文权威」是针对自己的产品取舍，不能反过来宣称 AgentConnect 全部只靠裸文件。Norma 可以保留 SQLite 运行记录/重试/抑制元数据，但若增加跨 MD 发布记录，必须定义它与活动 MD revision 的恢复关系，不让 DB 成为另一套事实正文。[S29][S30]

### D. Norma 的最小正确路径

1. **一个正文权威。** 放在现有 App 数据根下 `norma/memory`，不进仓库 checkout。`personal` 和受宿主管理的项目范围各维护主题 MD 与生成的 `MEMORY.md`。第一票接口与版本 manifest 见 [MemoryStore 接入说明](../references/norma-memory-store.md)；真实经历绑定、用户纠正及遗忘抑制继续在接入票落实。
2. **项目身份独立于 worktree。** 宿主把 session/worktree 绑定到可信项目身份；同一项目的不同 worktrees 共享同一项目库，互不相关项目隔离。远端 URL、分支名或当前目录路径都不能单独充当通用身份规则；跨 clone 是否共享由现有项目身份或用户显式关联确定。模型不能自己改 scope 或选择任意文件路径。
3. **复用既有存储保存经历，不复制事实。** SQLite 继续存原始会话/工具轨迹、Dream job 状态、来源消费进度；必要时存从 MD 可重建的搜索索引，并以 scope＋revision/digest 标识版本。查询发现版本落后时回到 MD/重建，不能把旧 SQLite 结果重新覆盖文件。第一版如果直接词法读取已够用，就不建辅助索引。
4. **宿主统一写入和发布。** 普通蒸馏、设置页编辑与 Dream 都产出候选，由同一 scope 的宿主后台提交。一个简单且明确的多文件发布方法是：先写完新的不可变 MD revision 目录（topics＋index），验证完成，再切换一个活动版本指针；一次 reader 固定同一 revision，旧 revision 保留用于恢复。原子指针切换只发布已经完整的集合，并不让 topic/index 的逐文件写入变原子。若采用直接目录交换，则需宿主排他协调＋发布恢复记录，明确两个 rename 之间以及崩溃重启时怎样恢复；不要同时维护两种首版发布协议。
5. **只保留本轮需要的升级。** 第一版从手动 Dream、完整 diff 审核、digest 冲突后重跑、独立 Skill 启用开始；不照搬 Control Plane、外部 Mem0 或复杂 rebase。外部编辑检测到 digest 变化必须转为新候选/重新读取，不能静默覆盖；多个 Norma 进程若可同时访问同一库，也必须共享单写宿主或跨进程锁，单个进程里的 mutex 不足够。

这条路径保留 AgentConnect 最关键的 MD 导航、经验蒸馏、完整暂存做梦和独立 Skill 采纳，同时使 SQLite 的角色与 Norma 现有经历记录一致。多文件发布与隐私硬隔离是必须说明的实现边界，不能被“先做最小版本”省略。

## 一手来源索引

以下均固定到本次读取的 commit，方便复核并防止 main 漂移。

- **[S1]** [官方 README：多 runtime、独立记忆和 Skill](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/README.md#L122-L146)；[仓库 API](https://api.github.com/repos/agentconnect-md/agentconnect)。
- **[S2]** [Managed memory 系统文档、backend、目录及边界](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/docs/designs/memory-system-plan.md)。
- **[S3]** [ManagedMemoryProvider：读写 scope、standingContext、空 recall、recordTurn](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/providers/managed.ts#L58-L149)。
- **[S4]** [ExternalMemoryProvider：trusted Agent scope、每轮 recall、outbox](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/providers/external.ts#L62-L164)；[第一方 Mem0 Cloud/OSS plugin](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/memory-plugin-mem0/README.md)。
- **[S5]** [记忆 frontmatter 与分类枚举](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/frontmatter.ts#L1-L29)。
- **[S6]** [Store 字节与历史上限](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/store.ts#L54-L102)；[文件 writer](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/store.ts#L600-L652)。
- **[S7]** [统一接口实现状态](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/docs/designs/unified-memory-interface.md#L1-L15)；[activation catalog 的不可信数据说明](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/entries/activation.ts)。
- **[S8]** [lexicalMatch 子串评分](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/entries/managed.ts#L45-L65)；[有边界的全文扫描](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/entries/managed.ts#L228-L267)。
- **[S9]** [Memory evolution：范围与后续 BM25](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/docs/designs/memory-evolution.md#L982-L1024)。
- **[S10]** [召回 record scope、大小、重复校验与不可信提示区块](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/recall.ts#L33-L116)。
- **[S11]** [蒸馏 policy、只读模式、输入预算](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/distill.ts)。
- **[S12]** [Managed distillation 的 durable outbox 适配](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/managed-distill-outbox.ts)。
- **[S13]** [Dream source 选择、100-session 上限、当前隐私规则和后台 reservation](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/dream/runner.ts#L527-L698)。
- **[S14]** [Dream 与 Skill mining 的模型 policy](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/dream/dreamer.ts#L141-L203)；[参数常量](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/dream/dreamer.ts#L106-L135)。
- **[S15]** [Dream extraction 在输入落盘前验证只读模式](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/daemon.ts#L8032-L8062)；[拒绝空重建删除整个库](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/dream/runner.ts#L870-L894)。
- **[S16]** [Dream 采纳、digest fence、rebase、备份及 auto-adopt](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/dream/runner.ts#L1198-L1409)。
- **[S17]** [Dream 默认值与 channel 禁用策略](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/protocol/src/frames/memory-connection.ts#L43-L160)。
- **[S18]** [DreamScheduler 使用 schedule/timezone](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/scheduler/dream-scheduler.ts)。
- **[S19]** [与当前源码冲突的旧 Dream 默认说明](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/docs/product-conventions.md#L1065-L1077)。
- **[S20]** [显式 Skill acceptance](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/dream/runner.ts#L1564-L1608)；[immutable Skill bundle 与原子索引发布](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/skills/dream-skills.ts#L186-L259)。
- **[S21]** [条件删除与 backend capabilities](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/entries/managed.ts#L98-L126)；[删除 override 会显露 inherited memory 的工具说明](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/entries/tools.ts#L63-L70)。
- **[S22]** [主题写入与目录生成的职责及兼容/事务分支](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/store.ts#L622-L765)。
- **[S23]** [post-turn exclusion gate 与后台 chain](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/daemon.ts#L7418-L7522)；[用户答复已送达后排队](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/daemon.ts#L15848-L15882)；[读取索引前等待此前 chain](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/daemon.ts#L14219-L14221)。
- **[S24]** [Dream 完整 staged store、来源校验与候选完成](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/dream/runner.ts#L712-L930)。
- **[S25]** [真实 UI 审核 live/staged 并集与 LineDiff](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/web/src/components/console/DreamPanel.tsx#L587-L730)。
- **[S26]** [两个 rename 与失败恢复](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/dream/runner.ts#L1329-L1343)；[rebase 的 generation/write-marks/按行增量边界](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/dream/runner.ts#L1436-L1506)。
- **[S27]** [recordTurnForBinding 的真实 dispatch](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/providers/dispatching.ts#L122-L134)；[蒸馏 extractor 的 scoped tools/permission gate/prompt](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/daemon.ts#L7540-L7645)；[writeMemory 工具实施写入](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/mcp/ops/memory.ts#L253-L301)。
- **[S28]** [手动 Dream 协议调用](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/cp/dream-reader.ts#L51-L86)；[定时 Dream admission 与 start](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/daemon.ts#L23226-L23280)；[Dream MCP 绑定到 staging](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/daemon.ts#L7995-L8046)；[staged 索引生成](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/dream/runner.ts#L1005-L1030)。
- **[S29]** [单文件原子发布实现](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/fs.ts#L263-L316)；[进程内目录锁](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/store.ts#L224-L250)；[主题＋目录事务准备与 ambiguous replay](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/memory/atomic-write.ts#L54-L168)。
- **[S30]** [CP atomic/capture ports](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/cp/memory-fs.ts#L74-L158)；[PostgreSQL 事务、抑制、history 与 receipt](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/control-plane/src/persistence/repositories/agent-memory-transaction.repo.ts#L49-L198)；[LocalStore 保存 Dream 状态](https://github.com/agentconnect-md/agentconnect/blob/478aab4e0aceb2127acc53429236d25e74515ed6/packages/daemon/src/store/local-store.ts#L4646-L4682)。
