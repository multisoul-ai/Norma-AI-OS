# Ideas Vault：基于现有 Canvas 的原生化重设计

- 日期：2026-08-04
- 状态：Redesign Proposal / Decision Support
- 替代方案：本文件取代 [`2026-08-04-SPEC-ideas-vault-design-and-prototype-plan.md`](2026-08-04-SPEC-ideas-vault-design-and-prototype-plan.md)
- 范围：只做架构审视、产品设计、交互原型与未来改动定位；本轮不修改产品代码
- 设计基线：现有 Canvas/Card/Workspace 机制、`codebase-design`、Apple Human Interface 原则

配套画布原型与逐屏交互说明见 [`Ideas Vault Canvas-native 原型`](../design-docs/2026-08-04-ideas-vault-canvas-native/README.md)。

## 1. 核心结论

旧方案的根本错误，是把 Ideas Vault 设计成 Canvas 外的一套“对话页 + 浏览器资料库 + Preview 文档”。这同时虚构了当前不存在的 Norma 聊天卡片、绕开了卡片编排，并把一个高频工作入口降级成需要间接打开的文件视图。

重新设计后的核心形态是：

> **每个 workspace 只有一张可关闭、可重新打开的 `Ideas Vault` 静态卡片。卡片本身就是这个工作区想法的收件箱、检索入口和详情视图；Norma 是语音/对话控制面，不是另一张 AgentCard。**

关键决策如下：

1. **专用卡片成立。** Ideas Vault 是持续存在、高频浏览、需要卡内状态和直接操作的工作对象，仅靠 Preview/Browser 无法表达。未来实现应新增 `NodeKind::IdeasVault`，但仍沿用现有 Canvas 节点/Card registry，不新增 LiveNode 或 Runtime session。
2. **workspace 级单例。** 数据跟当前项目走，权威数据进入 `<workspace>/.multisoul/project.sqlite`；卡片只是同一 workspace Idea 集合的视图。P0 不做脱离 workspace 的应用级总库。
3. **一库一卡，不是一想法一卡。** 搜索结果和 Idea 详情在同一 Vault 卡片内切换，避免每条 Idea 变成节点导致网格整体缩放、画布污染和恢复膨胀。
4. **Norma 负责捕获和命令，Canvas 负责可见结果。** 用户对底部 Voice/Norma 说“记一下”；真实落盘后更新或创建 Vault 卡片。绝不在 AgentCard 内伪造 Norma 对话 UI。
5. **不抢正在工作的焦点。** Vault 已存在时，语音捕获只更新 badge、列表顶部和一次轻量 attention，不改变 AgentCard 的 `NodeFocused`；用户说“打开它”或手动点击时才选中 Vault。
6. **富内容复用现有卡片能力。** 图片/Markdown/Text 用 Preview，网页用 Browser；Idea 详情只保存稳定路径/URL/source reference。打开附件时使用现有 Preview/Browser 创建和 master-detail 布局。
7. **关闭卡片不删除数据。** 关闭只移除 Canvas 投影；从 New 菜单或 Norma 重新打开时恢复同一 workspace 的 Vault。

## 2. 为什么旧方案与当前画布不搭

| 旧方案假设 | 当前系统事实 | 重设计纠正 |
|---|---|---|
| Norma 在一张富交互聊天卡里给保存回执 | Norma 是唯一 Master Agent，明确不是 Canvas AgentCard；AgentCard 正文是 terminal grid | 回执走底部 Voice/Norma 状态与语音回复，保存结果落到 Vault 卡片 |
| P0 用 Browser-backed Library 管理 Ideas | Browser 是网页 surface，脱离原生 Card 语义和 GPUI 状态管理 | Vault 使用专用静态 Card，列表/详情由 app 层原生渲染 |
| 每条 Idea 用 Preview 打开、编辑和导航 | Preview 只持久化路径并只读渲染 Markdown/Text/Image | Preview 只负责附件或导出文档，Idea 导航留在 Vault 卡片 |
| 新内容可“出现在 Agent 右侧空位” | 新节点默认进入全量网格 reflow；master-detail 也有既定布局 | 首次创建 Vault 走 `next_tiled_node_bounds + apply_layout_for_new_node`；附件走已有 master-detail |
| Vault 应先做应用级跨项目统一库 | Runtime、ProjectStore、Canvas snapshot 和 Norma turn 都以当前 workspace 为边界 | P0 以 workspace 为权威边界；跨项目总览留到 P2 |
| 保存时先弹结构化预览卡确认 | 当前语音入口以状态反馈和最终回复为主，没有通用聊天 sheet | 明确“记一下”默认直接保存 Draft；仅在缺少想法正文时问一个最小问题 |
| 打开多个搜索结果就是排列多个 Preview | 每新增一张卡都会参与全量布局，数量增加会缩小所有卡片 | 搜索结果只在一个 Vault Card 内展示；用户明确打开附件才新增 Preview/Browser |

因此本重设计不沿用旧方案的 UI 信息架构、打开方式、全局存储边界或原型图。

## 3. 现有 Canvas 逻辑：不可违背的事实

### 3.1 节点是“类型描述 + 空间状态”

`CanvasNode` 只包含 `id/kind/bounds/z_index/selected`；Canvas snapshot 只持久化节点描述、位置、层级和 viewport。节点不是通用文档容器，也没有任意业务关系字段。

- [`crates/canvas/src/node.rs:59-75`](../../crates/canvas/src/node.rs#L59-L75)：当前固定五类 `NodeType/NodeKind`
- [`crates/canvas/src/node.rs:115-122`](../../crates/canvas/src/node.rs#L115-L122)：统一节点空间字段
- [`crates/canvas/src/document.rs:7-19`](../../crates/canvas/src/document.rs#L7-L19)：snapshot 结构
- [`crates/canvas/src/document.rs:45-85`](../../crates/canvas/src/document.rs#L45-L85)：保存与恢复
- [`crates/canvas/src/document.rs:88-100`](../../crates/canvas/src/document.rs#L88-L100)：新增节点分配 ID/z-index

设计含义：`IdeasVaultSpec` 只能描述“这是当前 workspace 的 Vault 投影”，不能承载 Idea 全文、附件二进制或搜索索引。

### 3.2 所有节点通过同一 Card 壳层渲染

`Card` trait 负责 title chrome、body、resize 和内部滚动所有权；registry 按 `NodeType` 静态分发。Canvas 统一绘制卡片外壳、选中/焦点、标题栏、关闭、拖动和内容区。

- [`crates/app/src/cards/mod.rs:23-38`](../../crates/app/src/cards/mod.rs#L23-L38)：Card contract 与滚动模式
- [`crates/app/src/cards/mod.rs:65-79`](../../crates/app/src/cards/mod.rs#L65-L79)：固定 Card registry
- [`crates/app/src/canvas/view.rs:143-183`](../../crates/app/src/canvas/view.rs#L143-L183)：节点选择 renderer 和共享外壳
- [`crates/app/src/canvas/view.rs:202-314`](../../crates/app/src/canvas/view.rs#L202-L314)：共享标题栏、状态和关闭动作
- [`crates/app/src/canvas/node_label.rs:5-12`](../../crates/app/src/canvas/node_label.rs#L5-L12)：Card 类型标签

设计含义：Ideas Vault 不是浮在 Canvas 上方的一张独立“应用页面”。它必须使用同样的 `IDEAS / Ideas Vault / badge / close` chrome，并拥有与其他卡片一致的 hover、selected、drag 和 close 行为。

### 3.3 AgentCard 不是 Norma 聊天卡

AgentCard 的正文直接复用 terminal renderer，滚轮进入 terminal scrollback；标题栏才显示任务标题和 Ready/Thinking/Tool/Approval/Done/Failed。

- [`crates/app/src/cards/agent/card.rs:18-42`](../../crates/app/src/cards/agent/card.rs#L18-L42)：AgentCard chrome
- [`crates/app/src/cards/agent/card.rs:44-79`](../../crates/app/src/cards/agent/card.rs#L44-L79)：terminal-backed body 与 scrollback
- [`crates/agent/src/master_prompt.rs:3-6`](../../crates/agent/src/master_prompt.rs#L3-L6)：Norma 不是 Canvas AgentCard

设计含义：Ideas 捕获不能再画成“用户在 Norma AgentCard 里聊天”。语音 transcript、Thinking/Replying 和 spoken reply 继续属于底部 Voice/Norma 入口；Canvas 只显示被操作后的对象。

### 3.4 静态卡片和 LiveNode 的边界已经明确

只有 Terminal/Agent 进入 live registry、接收 node input、拥有 PTY/session/cwd。Preview/Browser 没有 LiveNode；未知于 live registry 的节点返回 `Caps::NONE`。

- [`crates/runtime/src/runtime.rs:353-357`](../../crates/runtime/src/runtime.rs#L353-L357)：静态节点 `Caps::NONE`
- [`crates/runtime/src/runtime.rs:372-385`](../../crates/runtime/src/runtime.rs#L372-L385)：input/scroll 只路由到 live node
- [`crates/app/src/workspace/flow.rs:417-463`](../../crates/app/src/workspace/flow.rs#L417-L463)：恢复时只有 Term/Agent 恢复 live session

设计含义：Ideas Vault 是静态 Card。搜索框、筛选、行点击和编辑表单由 app 层 GPUI entity 处理，不加入 PTY、checkpoint、node session 或 cwd。

### 3.5 新卡片会重排画布，不是自由插入

默认布局以 document 顺序收集全部节点，新增一张卡后对所有卡片重新计算网格；当前默认是 column-first、两条 track、fit viewport。用户也可把一张选中的卡设为 master。

- [`crates/app/src/canvas/layout_strategy.rs:19-34`](../../crates/app/src/canvas/layout_strategy.rs#L19-L34)：默认布局策略
- [`crates/app/src/canvas/layout_strategy.rs:89-125`](../../crates/app/src/canvas/layout_strategy.rs#L89-L125)：全量 reflow
- [`crates/app/src/canvas/master_layout.rs:184-213`](../../crates/app/src/canvas/master_layout.rs#L184-L213)：从选择进入 master layout

设计含义：首次创建 Vault 只新增一张单例卡；保存第 2、3、100 条 Idea 都不能继续新建节点。查看大列表时，用户可用现有 master layout 放大 Vault；打开附件才让 Preview/Browser 进入 detail 区。

### 3.6 选中、聚焦和静态卡片交互不同

Live 卡片可进入 `NodeFocused`；Browser/Preview 等静态卡创建后只选中并 `exit_to_canvas`。卡片内部原生控件需要阻止事件继续冒泡，像 Agent queue sidecar 一样独立管理 input focus。

- [`crates/app/src/app/mod.rs:286-305`](../../crates/app/src/app/mod.rs#L286-L305)：Browser 新建后的 selection/focus
- [`crates/app/src/canvas/master_layout.rs:203-207`](../../crates/app/src/canvas/master_layout.rs#L203-L207)：切 master 时退出其他节点焦点
- [`crates/app/src/cards/agent/queue.rs:132-169`](../../crates/app/src/cards/agent/queue.rs#L132-L169)：sidecar 阻止冒泡并自行滚动
- [`crates/app/src/cards/agent/queue/form.rs:40-103`](../../crates/app/src/cards/agent/queue/form.rs#L40-L103)：GPUI Input、焦点与提交

设计含义：点击 Vault 背景是“选中卡片”；点击搜索框/列表行/按钮是“卡内操作”。捕获完成不得无条件把正在输入的 AgentCard 踢回 Canvas。

### 3.7 数据和恢复以 workspace 为边界

ProjectStore 位于 `<workspace>/.multisoul/project.sqlite`，并聚合 Canvas、layout、session、voice、Agent 等 store。`.multisoul/` 会被保护为 Git 本地状态。

- [`crates/storage/src/project_store.rs:18-25`](../../crates/storage/src/project_store.rs#L18-L25)：项目数据库位置
- [`crates/storage/src/project_store.rs:50-80`](../../crates/storage/src/project_store.rs#L50-L80)：ProjectStore 子 store 边界
- [`crates/storage/src/database_paths.rs:72-74`](../../crates/storage/src/database_paths.rs#L72-L74)：生产/测试数据库路径
- [`crates/workspace/src/git_clone/security.rs:7-8`](../../crates/workspace/src/git_clone/security.rs#L7-L8)：`.multisoul/` 排除规则

设计含义：P0 Ideas 与 Canvas、Voice 来源和当前 Agent 上下文共享 workspace 边界。关闭工作区后数据仍在项目库；复制源码仓库不会意外提交私有想法。

## 4. 新信息架构：Vault Card 是工作集入口，IdeaStore 是权威数据

```text
当前 Workspace
├── ProjectStore
│   ├── Canvas snapshot
│   ├── Layout / session / voice / agents
│   └── IdeaStore                         ← Ideas 权威数据与检索
│       ├── IdeaRecord
│       ├── IdeaAsset
│       └── IdeaOperationReceipt
│
└── Canvas Document
    ├── Terminal / Agent / Preview / Browser ...
    └── IdeasVaultSpec                   ← 单例、轻量投影
         └── app::cards::ideas 状态      ← inbox/detail/search/editor
```

三条边界必须稳定：

- **Canvas snapshot 负责“卡片存在在哪里”**，不负责 Idea 正文。
- **IdeaStore 负责“这个 workspace 有哪些 Idea”**，不负责节点几何。
- **app 卡片状态负责“此刻看的是列表还是详情、搜索框是否聚焦”**，瞬态筛选不进入领域模型。

## 5. Vault Card 设计

### 5.1 卡片身份和单例规则

- 类型标签：`IDEAS`
- 标题：`Ideas Vault`
- badge：默认显示总数，例如 `12 IDEAS`；有新捕获且未查看时显示 `+1 NEW`
- workspace 内最多一个 `IdeasVault` node
- New 菜单点击 `Ideas Vault`：已有则选择并滚回可见区；没有则创建
- Norma 的 `open_ideas_vault` 同样执行“复用或创建”，不产生重复卡片
- 关闭只移除 node；IdeaStore 不删除、不归档

单例行为直接借鉴现有 Ego：先找既有 node，找不到才创建，然后选择并回到 Canvas 焦点。现有先例位于 [`crates/app/src/app/mod.rs:307-327`](../../crates/app/src/app/mod.rs#L307-L327)。

### 5.2 卡片内三种主模式

#### Inbox

默认模式，适应现有固定长宽比：

```text
┌ IDEAS  Ideas Vault                         12 IDEAS  × ┐
│ [搜索想法…]       [全部] [草稿] [待推进]             │
│                                                        │
│ 语音命令的实时转写                    草稿   刚刚       │
│ 保存网页选区到想法                    待推进 昨天       │
│ Canvas 卡片分组                        草稿   8月2日      │
│                                                        │
│ 3 个标签 · 当前项目                        ＋ 新想法     │
└────────────────────────────────────────────────────────┘
```

- 默认按 `updated_at DESC`
- 一行呈现标题、状态、更新时间和最多两个标签
- body 使用 `NativeElement` 内部滚动，滚轮不缩放 Canvas
- 空状态只给两个主动作：“对 Norma 说记一下”与“＋ 新想法”
- 搜索和筛选只改变此卡片列表，不创建结果节点

#### Detail

点击一行后在同一张卡内替换 body，不新增 Idea 节点：

```text
┌ IDEAS  Ideas Vault / 语音命令实时转写       DRAFT  × ┐
│ ‹ 返回想法列表                                   编辑 │
│                                                        │
│ 让 Voice 在录音时显示稳定的实时转写…                  │
│                                                        │
│ 设计说明                                               │
│ 限制 transcript 为两行，完整内容可展开…               │
│                                                        │
│ 素材  预览图.png  ·  交互说明.md  ·  来源 Agent       │
│ #voice  #canvas                 8月4日 14:32            │
└────────────────────────────────────────────────────────┘
```

- 返回只切回 Inbox，不改变 Canvas selection
- 点击图片/Markdown/Text 附件：复用现有 Preview
- 点击网页：聚焦已有 Browser，或用 URL 重建 Browser
- 点击来源 AgentCard：若 NodeId 仍存在则 focus/select；否则显示“来源卡片已关闭”并保留快照元数据
- “展开”沿用现有 master layout；不是自创全屏页面

#### Editor sidecar

新增/编辑使用锚定 Vault 卡片的非模态 sidecar，复用 Agent queue 的空间规则和输入模型：

```text
┌ Ideas Vault card ┐  ◁  ┌ 新想法 / 编辑想法 ─────────┐
│ inbox or detail  │     │ 标题                         │
│                  │     │ 描述（多行）                 │
│                  │     │ 标签 · 状态                  │
│                  │     │ 素材 [添加文件] [关联当前卡] │
└──────────────────┘     │              取消   保存草稿 │
                         └──────────────────────────────┘
```

- 空间足够显示在右侧，否则显示在左侧
- 不盖住整张 Canvas，不制造新的窗口层级
- `Esc` 关闭且保留未提交草稿到本次 app state；显式“丢弃”才清空
- 保存按钮在内容为空时 disabled
- 文件复制、哈希和数据库写入走后台任务；完成前显示局部 saving，不阻塞 Canvas

### 5.3 状态模型

P0 只保留最小、可理解的工作状态：

| 状态 | 用户含义 | 主要动作 |
|---|---|---|
| `draft` / 草稿 | 刚捕获，尚未整理 | 补充、关联素材 |
| `shaping` / 待推进 | 正在形成设计 | 打开详情、继续设计 |
| `adopted` / 已采纳 | 已进入实现或决策 | 关联产物、归档 |
| `parked` / 暂存 | 现在不做但保留 | 恢复推进 |
| `archived` / 已归档 | 从默认列表隐藏 | 恢复 |

不设置强制线性流转；新 Idea 默认 `draft`。删除用“移到最近删除/可恢复”，不作为主状态 badge。

## 6. 完整交互流程

### 6.1 语音快速捕获：Vault 尚不存在

```text
用户按 Voice 说：
“记一下：Preview 图片支持在同一张卡里左右切换。”

→ Voice 显示 Listening / transcript / Thinking
→ Norma 识别明确 capture 意图，调用 typed `capture_idea`
→ app adapter 在后台写入当前 ProjectStore::IdeaStore
→ 事务提交成功
→ 若 Canvas 没有 Vault node：
   创建唯一 IdeasVaultSpec
   使用 next_tiled_node_bounds
   与全部现有卡片一起 reflow
   选中 Vault，但保持 CanvasSelect
→ Vault 顶部出现新 Idea，badge 短暂变为 +1 NEW
→ Norma 根据真实回执说：
   “记好了，放在当前项目的 Ideas Vault，标题是‘Preview 图片切换’。”
```

只有数据库事务成功后才说“记好了”。若写入失败，不创建假记录、不显示成功 badge，原 transcript 保留供重试。

### 6.2 语音快速捕获：用户正在 AgentCard 工作

```text
前置：Codex AgentCard 正在 NodeFocused，Vault 已存在

用户说：“把这个点记下来：先做 workspace 级，不做全局库。”
→ capture_idea 真实落盘
→ Vault 列表和计数更新，卡片边缘一次轻量 cool attention
→ Codex AgentCard 仍保持 NodeFocused
→ 不移动、不重排任何卡片
→ Norma 给 spoken receipt；用户继续在 AgentCard 输入
```

这是避免“记录想法反而打断当前工作”的关键行为。

### 6.3 关联当前卡片上下文

Norma 每回合当前只知道卡片的 `node_id/position/kind/title/selected/focused`，并不知道 Preview 正文或 Agent terminal 内容。未来设计必须遵守这个边界：

- “记下这个想法”只保存用户本轮明确说出的正文
- “把当前 Preview 关联进去”由 app adapter 根据实时 NodeId 解析稳定路径
- “把当前网页关联进去”解析实时 Browser URL/title
- “把这张 Agent 卡的结论记下来”若用户没有说出结论，Norma 应追问结论；不得静默抓取完整 terminal scrollback
- 来源同时保存稳定元数据和可选 ephemeral NodeId；NodeId 不能成为长期唯一引用

当前 Canvas context 证据：[`crates/agent/src/current_context.rs:22-39`](../../crates/agent/src/current_context.rs#L22-L39)。

### 6.4 手动新增

```text
New 菜单 → Ideas Vault
→ 已有：选中并确保可见
→ 没有：创建单例并参与标准 reflow

Vault 右下角“＋ 新想法”
→ 打开卡片锚定 sidecar
→ 填写最少一个描述
→ 保存草稿
→ 事务成功后插入 Inbox 第一行
→ sidecar 关闭，Vault 仍选中
```

New 菜单现有创建入口位于 [`crates/app/src/app/new_menu.rs`](../../crates/app/src/app/new_menu.rs)，未来只增加一个同级项目，不新增独立侧边栏导航。

### 6.5 浏览、搜索与打开

```text
用户点击 Vault 或说“打开想法库”
→ 复用唯一 Vault card
→ 用户可将其设为 master 获得更宽列表
→ ⌘F / 搜索框输入“语音”
→ 后台查询当前 workspace IdeaStore
→ 同一 Card 内显示结果数量和列表
→ 点击一项切入 Detail
→ 点击附件才创建/聚焦 Preview 或 Browser
```

Norma 查询“找我关于语音的草稿”时：

- 若用户只问有哪些，Norma 可只读回答摘要，不改变 Canvas
- 若用户说“在画布里打开”，则打开/复用 Vault、写入其 search view state 并选中
- 不为每条结果创建 Preview，不触发 N 次画布 reflow

### 6.6 富内容进入系统

| 入口 | P0 行为 | 与 Canvas 的关系 |
|---|---|---|
| sidecar 文件选择 | 复制到 `.multisoul/ideas/assets/<idea-id>/`，保存 hash/mime/display name | 附件点击后创建 Preview 或系统打开 |
| 拖到 Vault 卡片/sidecar | 显示 drop target，逐项复制并报告结果 | 事件由 app card 处理，不进入 runtime input |
| 当前 Preview | 保存 workspace 相对路径和源标题；可选复制为 managed asset | 点击来源优先 focus 已有 Preview，否则按路径重建 |
| 当前 Browser | 保存 URL、标题和捕获时间 | 点击来源优先 focus 已有 Browser，否则按 URL 重建 |
| Browser 下载文件 | P1 由明确“添加到想法”动作接管 | 不自动吞掉所有下载 |
| 视频 | P0 管理文件和元数据，使用系统播放器打开 | 不走 Preview 的图片 CPU render 路径 |

P0 附件目录属于 Norma workspace state，不进入 Git。若团队需要把设计资产提交到仓库，用户应显式“导出到项目目录”，而不是默认把 `.multisoul` 当共享目录。

### 6.7 关闭、重新打开和 workspace 恢复

- 关闭 Vault card：沿用共享 close route，移除 node 并 reflow；IdeaStore 不变
- 再次 New → Ideas Vault：重新创建单例并加载 IdeaStore
- 关闭 app 前如果 Vault card 存在：Canvas snapshot 保存 `IdeasVaultSpec + bounds + z-index`
- 打开 workspace：先恢复静态壳层，再后台 hydrate 列表；不进入 Terminal/Agent session restore
- 切换 workspace：Vault 和 Ideas 数据跟随新 ProjectStore，不跨项目串数据
- 删除/归档单条 Idea：是卡内领域操作，不调用 `delete_canvas_card`

Canvas close 与 Runtime cleanup 证据：[`crates/app/src/app/mod.rs:372-399`](../../crates/app/src/app/mod.rs#L372-L399)、[`crates/runtime/src/runtime.rs:417-437`](../../crates/runtime/src/runtime.rs#L417-L437)。

## 7. 数据模型与持久化边界

### 7.1 Canvas 投影

```rust
pub struct IdeasVaultSpec;
```

P0 不在 `IdeasVaultSpec` 内保存查询、选中的 Idea、正文或数量。它只声明这个 workspace 的 Vault 卡片存在。原因是 Runtime 本身已经绑定 workspace root，额外复制 workspace ID 没有收益；瞬态搜索也不应污染 Canvas snapshot。

### 7.2 权威领域记录

```rust
pub struct IdeaRecord {
    pub id: IdeaId,
    pub title: String,
    pub description: String,
    pub raw_capture: String,
    pub design_notes: Option<String>,
    pub status: IdeaStatus,
    pub tags: Vec<String>,
    pub source: IdeaSource,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
    pub version: u64,
}

pub struct IdeaSource {
    pub input_source: IdeaInputSource, // voice / text / manual
    pub norma_turn_id: Option<String>,
    pub source_node: Option<SourceCardRef>,
}

pub struct SourceCardRef {
    pub node_id_at_capture: Option<NodeId>,
    pub kind: SourceCardKind,
    pub title_at_capture: String,
    pub stable_locator: Option<String>, // workspace path / URL / agent definition id
}

pub struct IdeaAsset {
    pub id: IdeaAssetId,
    pub idea_id: IdeaId,
    pub kind: IdeaAssetKind,
    pub display_name: String,
    pub managed_path: Option<String>,
    pub source_uri: Option<String>,
    pub mime_type: Option<String>,
    pub content_hash: Option<String>,
    pub created_at: i64,
}
```

### 7.3 SQLite 与文件结构建议

```text
<workspace>/.multisoul/
├── project.sqlite
│   ├── ideas
│   ├── idea_tags
│   ├── idea_assets
│   └── idea_operation_receipts
└── ideas/
    ├── assets/
    │   └── idea_<ulid>/
    │       ├── <asset-ulid>--prototype.png
    │       └── <asset-ulid>--walkthrough.mov
    └── thumbnails/              # 可重建，不是权威数据
```

索引策略：

- P0：普通 SQLite 索引 `status/updated_at`，标题/描述用转义后的 `LIKE`；先验证真实规模
- P1：达到数千条或检索延迟不达标后再迁移 FTS5；标签使用 join table，不存逗号字符串
- `idea_operation_receipts` 以 Norma `operation_id/tool_call_id` 唯一约束，实现 capture 幂等
- 文件先复制到受控临时名、完成 hash 后原子提升，再提交 asset row；失败不能留下“数据库声称有附件但文件不存在”的记录
- 二进制不放 SQLite blob，不进入 Canvas JSON

### 7.4 与 Workspace 模型的关系

- Idea 不需要额外 `workspace_id` 列作为主边界，因为每个 `project.sqlite` 已天然隔离 workspace
- 若 P2 做跨项目总览，AppStore 只维护可重建 catalog：`workspace_identity → idea_count/updated_at`；实际内容仍留在各 ProjectStore
- 项目迁移/改名由 workspace 根目录携带 `.multisoul`，不依赖绝对路径定位 Idea
- Git clone 默认不会带上本地 Ideas；团队共享需显式 export/import，不能悄悄改变现有隐私边界

## 8. Norma 工具和数据流

### 8.1 P0 typed tools

| Tool | 副作用 | Canvas 行为 |
|---|---|---|
| `capture_idea` | 创建 Draft，支持可选 source/attachments | 首次创建单例；已有只更新 |
| `search_ideas` | 无，查询当前 ProjectStore | 默认不改 Canvas |
| `open_ideas_vault` | 无 Idea 数据修改 | 复用/创建并选择 Vault，可带 query/filter |
| `update_idea` | 修改标题/说明/标签/状态 | 更新当前详情和 badge |
| `attach_idea_source` | 增加文件/URL/card source | 详情素材区更新 |

P0 可将 `open_ideas_vault` 作为 `search_ideas(open_in_canvas=true)` 的内部行为，但 Agent 对外工具应保持语义明确，避免“搜索”偷偷改布局。

### 8.2 执行链

```text
Voice FinalTranscript / text turn
→ NormaSession + Rig typed tool
→ AgentCardAdapter command + ToolContext
→ app adapter 校验当前 runtime/workspace/实时 node
→ background IdeaStore / asset IO
→ transaction receipt
→ GPUI update Vault app state
→ 必要时创建/选择 Canvas node
→ AdapterResult 返回稳定 idea_id / node_id / outcome
→ Norma 只基于真实回执答复
```

现有 typed tool 创建 Preview 的 app seam 可参考 [`crates/app/src/app/agent_runtime/adapter_pump.rs:304-329`](../../crates/app/src/app/agent_runtime/adapter_pump.rs#L304-L329)。但是 IdeaStore IO 不能直接照搬到 `update_in` 内同步运行；adapter pump 目前在 UI context 执行命令（[`crates/app/src/app/agent_runtime/adapter_pump.rs:7-38`](../../crates/app/src/app/agent_runtime/adapter_pump.rs#L7-L38)），未来需要先 background commit，再短暂回前台更新 UI。

## 9. 与现有画布逻辑的一一对应

| 新设计对象/行为 | 复用的现有机制 | 不做的事 |
|---|---|---|
| `IdeasVaultSpec` | `NodeKind` 轻量可持久化描述 | 不存正文/附件/索引 |
| `IdeasVaultCard` | `Card` trait + fixed registry + shared chrome | 不开独立窗口或网页壳 |
| workspace 单例 | Ego 的 find-or-create 行为 | 不重复创建 Vault |
| 首次打开 | next tiled bounds + motion + full reflow | 不硬编码 Agent 右侧坐标 |
| 静态输入/滚动 | GPUI Input + `NativeElement` | 不注册 LiveNode/PTY |
| 编辑面板 | Agent queue anchored sidecar | 不用全屏 modal |
| 大列表 | 现有 master layout | 不自建全屏 Library |
| 附件查看 | Preview/Browser + current master-detail | 不把附件复制成 Idea node |
| 关闭 | shared close route + reflow | 不删除 IdeaStore |
| workspace 恢复 | Canvas snapshot 恢复静态卡壳 + app hydrate | 不进入 session/checkpoint/cwd |
| Voice 捕获 | Norma Master typed tools + truthful receipts | 不伪造 AgentCard chat |
| 搜索结果 | 同一 Vault Card 的 list state | 不生成一排 Preview |

## 10. Apple 设计原则如何落地

- **内容优先：** Vault body 先显示想法标题和状态，不展示数据库、ID 或技术字段。
- **直接操作：** 点击 Idea 就在原卡切入详情；拖文件直接关联；不经过 Browser 网页中转。
- **即时反馈：** Voice 继续显示 Listening/Thinking；Vault 只在真实提交后显示 `+1 NEW`。
- **用户控制：** 语音捕获不抢 AgentCard 焦点；关闭 Vault 不删数据；归档和删除可撤销。
- **一致性：** 使用共享 Card chrome、New 菜单、master layout、Preview/Browser 和 anchored sidecar。
- **可访问性：** `⌘F` 搜索、`↑/↓` 选行、`Return` 打开、`Esc` 关闭 sidecar；hover/pressed/focus/selected/disabled 均有明确状态，结果数和保存状态不只靠颜色。
- **Reduce Motion：** 新增 Idea 只做 badge/edge 的短 cross-fade；首次创建 Vault 复用当前卡片 motion；不使用飞行素材或庆祝动画。

## 11. P0 / P1 / P2

### P0：Canvas-native 闭环

- workspace 单例 Vault Card
- Voice/文字 capture、真实落盘回执、幂等
- Inbox、Detail、关键词/状态筛选
- 手动 sidecar 新增/编辑
- 文件/Preview/Browser source 关联
- 图片/Markdown/Text 打开 Preview，URL 打开 Browser，视频系统打开
- close/reopen/workspace restore
- 不抢 AgentCard 焦点

### P1：规模与整理

- FTS5 与标签建议
- 重复提示但不自动合并
- Browser 下载到 Idea 的受控桥接
- 缩略图和附件去重/容量管理
- Adopted Idea 关联 Agent task/issue/产物
- 批量归档、恢复和导出

### P2：跨 workspace 发现

- AppStore 维护只读、可重建的 workspace Ideas catalog
- 从全局结果打开目标 workspace，再复用该 workspace 的 Vault Card
- 显式 export/import 或同步策略
- 经验证后再评估视频原生 Preview；必须遵守低拷贝视频路径

P2 仍不建议把所有 workspace 内容复制成一个 app-global canonical Vault，否则会破坏现有项目隔离、删除/移动语义和隐私边界。

## 12. 未来实现改动位置

以下是实施时的预期位置，不是本轮已修改代码：

| 位置 | 预期改动 | 理由 |
|---|---|---|
| `crates/canvas/src/node.rs` | 新增 `IdeasVaultSpec`、`NodeType::IdeasVault`、`NodeKind::IdeasVault`、title dispatch | 让 Vault 成为第一等静态节点描述 |
| `crates/app/src/cards/mod.rs` | 注册 `cards::ideas::IdeasVaultCard`，滚动模式为 `NativeElement` | 接入现有 Card registry |
| `crates/app/src/cards/ideas/` | Inbox/detail/body、badge、空态、附件行、sidecar | UI 逻辑上浮到 app，不污染域层 |
| `crates/app/src/canvas/node_label.rs` | 增加 `IDEAS` label | 复用共享 chrome |
| `crates/app/src/app/new_menu.rs` | New 菜单增加 Ideas Vault | 与 Terminal/Browser/Preview 同级发现入口 |
| `crates/app/src/app/mod.rs` 或独立 `app/ideas/` | find-or-create、select、reflow、sidecar state | 对照 Ego 单例和 Browser 静态卡创建 |
| `crates/runtime/src/runtime.rs` | 只增加 `spawn_ideas_vault`/`ideas_vault_node_id` 的 Document 操作 | 不增加 live entry/session |
| `crates/runtime/src/session_restore.rs` | exhaustive match 明确忽略 IdeasVault | 维持静态节点边界 |
| `crates/storage/src/migrations.rs` | 新增 ideas/tags/assets/receipts 表与版本迁移 | ProjectStore 权威数据 |
| `crates/storage/src/idea_store.rs` | 深模块 API：capture/search/update/archive/attach/idempotency | 隐藏事务和索引细节 |
| `crates/storage/src/project_store.rs`、`lib.rs` | 暴露 `idea_store()` 与 DTO | 与现有 store 聚合一致 |
| `crates/agent/src/adapter.rs`、`rig_tools.rs` | typed Idea commands/tools/schema/receipts | 接入 Norma 既有工具链 |
| `crates/agent/src/master_prompt.rs` | 明确 capture/open/search 意图和真实回执规则 | 防止普通讨论静默保存 |
| `crates/agent/src/current_context.rs` | 增加 `CanvasCardKind::IdeasVault` | 让 Norma 知道 Vault 是否已打开 |
| `crates/app/src/app/agent_runtime/adapter_pump.rs` | 后台执行 store/asset IO，前台更新卡片/布局 | 避免阻塞 GPUI |
| `crates/app/src/workspace/flow.rs` | workspace 切换时 reset/hydrate Ideas app state | 不触碰 PTY/session/cwd |
| `crates/e2e/` | capture、focus preservation、singletons、close/reopen、restore、attachment flows | 验收用户可观察闭环 |

### 不应改动的位置

- 不新增 `runtime::LiveNode` 实现
- 不给 Ideas 分配 PTY、Terminal grid、cwd 或 Agent session
- 不把二进制附件写进 `CanvasNodeKind` 或 canvas snapshot
- 不改 Terminal/Agent input route 来承载 Ideas 表单
- 不新增 Browser local Library 作为主入口

## 13. 验收标准

### P0 用户闭环

- [ ] 用户说“记一下：X”后，只有当前 workspace ProjectStore 事务成功才收到“已保存”回执。
- [ ] 首条 Idea 会创建一张且仅一张 Vault Card，并按当前全量布局规则参与 reflow。
- [ ] Vault 已存在时，连续保存 20 条 Idea 不新增节点、不触发画布 reflow。
- [ ] 用户在 AgentCard 输入时语音保存 Idea，AgentCard 仍保持 `NodeFocused`。
- [ ] New → Ideas Vault 和 Norma “打开想法库”都复用已有 node；不存在才创建。
- [ ] 点击 Idea 在同一 Vault Card 内进入 Detail，不创建 Idea Preview node。
- [ ] 搜索和状态筛选只更新 Vault body，并显示命中数量。
- [ ] 点击图片/Markdown/Text 附件创建或聚焦 Preview；点击 URL 创建或聚焦 Browser。
- [ ] 关闭 Vault Card 后 Idea 数据仍可查询；重新打开后列表完整。
- [ ] workspace 重启后 Vault Card 的存在和布局由 Canvas snapshot 恢复，Idea 列表由 IdeaStore hydrate。
- [ ] 切换到另一个 workspace 不显示前一 workspace 的 Ideas。
- [ ] 保存失败、附件部分失败、来源卡片已关闭都有明确恢复动作，不产生虚假成功状态。

### 自动化证据规划

- Canvas/Runtime 单测：序列化新 NodeKind、静态 caps、单例查找、删除 node 不删除 Idea
- Storage 单测：事务、search/filter、operation id 幂等、附件原子提升和 partial failure
- App headless E2E：New 菜单单例、Inbox/Detail、sidecar focus、search、close/reopen
- 真实 App L3 + Computer Use：Voice capture、AgentCard focus preservation、master layout、拖入附件、workspace restore、所有主题下的 hover/selected/focus/disabled
- 性能护栏：Idea IO 不出现在 `Runtime::drain_all`、mouse move、node input、无脏 flush 热路径

## 14. 本轮交付边界

本轮只新增/修订设计文档和原型图片：

- 没有修改任何 Rust 产品代码
- 没有新增实际 NodeType 或 Runtime 行为
- 没有新增 migration、SQLite 表或附件目录
- 没有运行产品实现测试；实施时按第 13 节建立自动化证据

