# Ideas Vault（想法收集与管理）设计与原型规划

> **已废弃（2026-08-04）：** 本方案以应用级资料库、Preview/Browser 主入口和伪对话式回执为核心，与当前 Canvas/Card/AgentCard/Workspace 逻辑不一致。请勿作为实施依据。替代方案见 [`Ideas Vault：基于现有 Canvas 的原生化重设计`](2026-08-04-SPEC-ideas-vault-canvas-native-redesign.md)。

- 日期：2026-08-04
- 状态：Proposal / Decision Support
- 范围：只做产品分析、交互设计与架构规划；不修改产品代码，不新增节点类型，不改 Runtime
- 设计基线：`apple-design`；信息层级、即时反馈、用户控制、可逆操作、平台熟悉性与可访问性优先

配套高保真原型、端到端交互图、异常恢复状态和逐项交互说明见：
[`Ideas Vault 原型交互与设计稿`](../design-docs/2026-08-04-ideas-vault-prototype/README.md)。

## 1. 核心结论

### 1.1 是否值得做

值得做，但第一版不应从“新建一种 Ideas 节点”开始。用户真正需要的不是更多卡片类型，而是一个可靠闭环：

```text
低摩擦捕获
  → 原始想法不丢失
  → 可渐进补充结构和素材
  → 可跨时间、跨项目找到
  → 可在画布中打开和继续思考
```

Canvas 是 Ideas Vault 的展示面，不应成为 Idea 数据的唯一存储位置。Idea 的生命周期通常长于某张卡片和某个 workspace；把完整 Idea 塞进 `NodeKind` 会把业务数据错误地绑定到项目级 Canvas snapshot。

### 1.2 推荐方案

1. **采用应用级、local-first 的统一 Ideas Vault。** Canonical 内容放在 Norma OS 的应用数据目录；`app.sqlite` 保存可重建索引。Idea 可关联零个或多个 workspace，但不从属于某个 Canvas node。
2. **P0 不新增节点类型。** Idea 详情和图片复用 Preview；网页素材复用 Browser；检索结果先由 Norma 对话返回，必要时生成只读 Markdown 结果并用 Preview 打开。
3. **明确捕获意图才保存。** “记一下”“我有个想法”“存到想法库”等显式表达触发保存；普通讨论不静默入库。
4. **低风险捕获默认先保存、后确认。** 保存是可逆动作，不使用阻断式确认框。系统在真实落盘成功后给出 Idea ID、标题、状态和“打开/撤销/继续补充”入口。
5. **原文永远保留，结构化字段允许修正。** 标题、摘要、标签可以由 Norma 建议，但不得替换或丢弃用户原始表达。
6. **附件默认复制入受管目录。** 外部文件链接作为高级选项；这样截图、原型图和视频不会因原路径移动而失效。
7. **视频 P0 只管理，不内嵌渲染。** 当前 Preview 仅支持 Markdown/Text/Image；视频先作为附件保存并由系统播放器/Quick Look 打开，后续再评估低拷贝视频 Preview。
8. **P1 再验证 Browser-backed Library。** 如果用户确实高频用鼠标浏览/筛选，再在 Browser 卡片中加载受控的本地 Ideas Library 页面；在验证前不承担新节点和原生列表编辑器的成本。

### 1.3 需要用户决策的三个问题

| 决策 | 推荐 | 影响 |
|---|---|---|
| Vault 范围 | 应用级统一库 | 满足“统一位置”和跨项目回看；需要显式 workspace 关联 |
| P0 浏览形态 | Norma 检索 + Preview 详情 | 复用最多、架构风险最低；鼠标批量筛选体验留到 P1 |
| 附件策略 | 默认复制，允许显式链接 | 稳定性优先；会增加磁盘占用，需哈希去重和容量提示 |

## 2. 第一性原理审查

### 2.1 用户要解决的根问题

用户的问题不是“缺一张 Ideas 卡片”，而是：

- 灵感发生时上下文切换成本太高，所以没有被记录。
- 对话记录、截图、原型、视频分散，之后找不到或无法还原当时思路。
- 原始想法与后续设计、评审和采纳状态没有稳定关联。
- Canvas 擅长空间化查看，但当前节点模型不是通用知识库。

因此成功指标应优先衡量“捕获成功率、找回速度、素材完整性”，而不是“新增了多少 UI”。

### 2.2 最小充分解

最小充分解由四个原语组成：

1. `capture`：把原文和来源可靠保存为 Draft。
2. `enrich`：补标题、摘要、标签、设计说明和附件。
3. `search`：按关键词、标签、状态、workspace 关系找回。
4. `present`：用现有 Preview/Browser 在 Canvas 上查看。

这四个原语不要求新增 NodeKind。只有当“持续在 Canvas 内用鼠标管理大量 Idea”被验证为高频任务时，专用 Ideas Card 才有足够收益。

### 2.3 明确拒绝的早期复杂度

- 不自动保存所有 Norma 对话。
- 不要求每次保存前填写完整表单。
- 不在 P0 建立强制分类体系或复杂工作流。
- 不在 P0 做相似 Idea 自动合并；只提示可能重复。
- 不把二进制素材写入 Canvas snapshot 或 SQLite blob。
- 不把 Idea 正文写入 `session_checkpoints`。
- 不在 P0 新增同步、协作、分享、版本分支或向量数据库。
- 不因“未来可能需要”提前新增专用节点类型。

## 3. 需求拆解

### 3.1 核心用户任务

| 用户任务 | 期望结果 | 首期优先级 |
|---|---|---|
| 对 Norma 说“记一下这个想法” | 原文可靠落盘，返回 Idea ID | P0 |
| 快速补一句设计说明 | 追加到刚保存的 Idea，保留历史 | P0 |
| 添加截图/原型图/文档 | 素材复制入库并与 Idea 关联 | P0 |
| 找“语音输入”相关想法 | 返回按相关度和更新时间排序的结果 | P0 |
| 在 Canvas 打开某个 Idea | 复用 Preview 展示详情或图片 | P0 |
| 按标签、状态、项目浏览 | 可组合筛选并保留当前视图 | P1 |
| 从 Browser 把页面/下载物加入 Idea | URL 引用 P0；受控下载入库 P1 | P0/P1 |
| 管理视频素材 | 保存、列出、系统打开 P0；内嵌播放以后评估 | P0/P2 |
| 将 Idea 标记为已采纳并关联实现 | 状态变更并记录目标 workspace/issue | P1 |

### 3.2 功能需求

- 支持语音和未来文字对话入口。
- 明确的捕获意图可创建 Idea；默认状态为 `draft`。
- 保存原始输入、输入方式、时间、workspace 来源和可选会话线索。
- 支持标题、摘要、正文、类别、标签、状态、附件和 workspace 关系。
- 支持新增、查看、更新、归档、恢复；硬删除不是主流程。
- 支持关键词、标签、状态、更新时间和 workspace 过滤。
- 支持 Idea 详情与素材在 Canvas 中展示。
- 工具副作用必须返回真实回执，并具备 durable idempotency。

### 3.3 非功能需求

- **可靠性：** 原始输入成功落盘后才允许说“已保存”。
- **性能：** 文件读写、索引、哈希、缩略图和搜索不得阻塞 GPUI 前台。
- **隐私：** 默认仅本机；日志不记录完整正文、完整绝对路径或敏感 URL query。
- **可恢复：** 索引损坏时可从 manifest 重建；附件和正文不因索引失败丢失。
- **可移植：** 单个 Idea 可导出为自包含目录/归档。
- **可访问：** 键盘可完成搜索、筛选、打开、状态修改；焦点和结果数量可感知。

## 4. 现有系统契合点与可复用模块

### 4.1 Canvas 节点和快照

当前 `CanvasNode` 统一承载 `id/kind/bounds/z_index/selected`；`DocumentSnapshot` 序列化节点描述、位置和 viewport。Idea 详情只要以 Preview/Browser 节点呈现，就能自动复用选择、布局和项目级 Canvas 恢复。

- 节点类型和 Preview/Browser 描述：[`crates/canvas/src/node.rs:38-75`](../../crates/canvas/src/node.rs#L38-L75)
- 节点标题与类型分派：[`crates/canvas/src/node.rs:80-103`](../../crates/canvas/src/node.rs#L80-L103)
- Canvas 节点字段：[`crates/canvas/src/node.rs:115-122`](../../crates/canvas/src/node.rs#L115-L122)
- Snapshot 与恢复：[`crates/canvas/src/document.rs:7-85`](../../crates/canvas/src/document.rs#L7-L85)
- 节点 ID/z-index 分配：[`crates/canvas/src/document.rs:88-101`](../../crates/canvas/src/document.rs#L88-L101)
- Snapshot 保存为项目 SQLite JSON：[`crates/storage/src/canvas_store.rs:16-43`](../../crates/storage/src/canvas_store.rs#L16-L43)

结论：Canvas snapshot 只保存“某个 Idea/库视图在此 workspace 打开了”的展示引用，不保存 Idea 的完整业务数据。

### 4.2 Preview：最直接的 Idea 详情展示面

Preview 已经采用“轻量节点描述 + app 层异步加载”的模式：

- `PreviewSpec` 只保存一个或多个路径和显式类型：[`crates/canvas/src/node.rs:38-57`](../../crates/canvas/src/node.rs#L38-L57)
- Runtime 创建 Preview 时只加 Document 节点，不注册 LiveNode：[`crates/runtime/src/preview.rs:20-61`](../../crates/runtime/src/preview.rs#L20-L61)
- PreviewStore 按 NodeId 管理 Loading/Ready/Error：[`crates/app/src/preview/mod.rs:15-90`](../../crates/app/src/preview/mod.rs#L15-L90)
- 相对路径以 workspace root 解析；绝对路径也可读取：[`crates/app/src/preview/mod.rs:156-174`](../../crates/app/src/preview/mod.rs#L156-L174)
- Markdown/Text/Image 类型推断与读取：[`crates/app/src/preview/mod.rs:176-255`](../../crates/app/src/preview/mod.rs#L176-L255)
- 文件选择器已支持多选，多个文件当前只接受图片：[`crates/app/src/preview/actions.rs:89-128`](../../crates/app/src/preview/actions.rs#L89-L128)
- Preview 加载在 background task：[`crates/app/src/preview/actions.rs:206-225`](../../crates/app/src/preview/actions.rs#L206-L225)
- Markdown/Text/Image renderer：[`crates/app/src/cards/preview/card.rs:57-109`](../../crates/app/src/cards/preview/card.rs#L57-L109)

可复用方式：

- 每个 Idea 生成稳定的 `idea.md`，用 Preview 打开。
- 一个 Idea 的多张原型图可复用多图 Preview。
- 当源文件更新时复用现有刷新动作。

限制：

- Preview 是只读文件视图，不是列表编辑器。
- Markdown renderer 当前没有可点击的 Idea 导航契约。
- 只支持 Markdown/Text/Image；没有 PDF、音频、视频。
- Preview 的路径目前会进入项目级 Canvas snapshot，因此 app-global Idea 文件应使用稳定绝对路径，并避免被移动。

### 4.3 Browser：网页素材与后续 Library 表面

- `BrowserSpec` 持久化 URL、标题和 surface：[`crates/canvas/src/node/browser.rs:4-29`](../../crates/canvas/src/node/browser.rs#L4-L29)
- Runtime 创建 Browser 同样只增加 Document 节点：[`crates/runtime/src/runtime.rs:281-295`](../../crates/runtime/src/runtime.rs#L281-L295)
- Browser URL 只明确拒绝 `data:` 和 `javascript:`，解析器接受 `file:`：[`crates/norma/src/presentation.rs:72-99`](../../crates/norma/src/presentation.rs#L72-L99)
- Canvas 渲染时按持久化 URL 重建 native Browser view：[`crates/app/src/canvas/view.rs:367-386`](../../crates/app/src/canvas/view.rs#L367-L386)
- Browser 已具备 back/forward/reload、地址编辑和页面交互事件：[`crates/app/src/app/browser.rs:40-94`](../../crates/app/src/app/browser.rs#L40-L94)

可复用方式：

- P0 将网页保存为 URL attachment，并用 Browser 打开。
- P1 可以验证由 Norma 生成、带严格 CSP 的本地 Ideas Library HTML，并用 Browser 卡片承载搜索和筛选。

限制与风险：

- 当前 BrowserEvent 没有“下载完成并移交给 Ideas Vault”的事件。
- 当前 IPC 只报告页面交互，不具备受控的 Idea action bridge。
- 本地 HTML 必须防止 Idea 文本注入脚本；不能直接拼接未转义内容。
- `file:` 页面、资源访问和跨文件加载需要在钉死的 Wry/WebKit 组合上做 L3 验证。

### 4.4 Card registry、布局与焦点

- Card trait 和静态 registry 已覆盖 Term/Agent/Preview/Browser：[`crates/app/src/cards/mod.rs:23-79`](../../crates/app/src/cards/mod.rs#L23-L79)
- Preview/Browser 创建可复用统一 placement、selection、motion 和布局流程：[`crates/app/src/norma/server.rs:311-406`](../../crates/app/src/norma/server.rs#L311-L406)
- 非 LiveNode 的 Preview/Browser 不进入 Terminal 输入路由；其交互由 app/GPUI 自己管理。

结论：P0 复用 Preview/Browser 不触碰 `LiveNode`；如果未来做可编辑 Ideas Card，其文本输入也应像 Browser 地址栏一样由 app 层 GPUI entity 管理，而不是伪装成 Terminal input。

### 4.5 Workspace 持久化、恢复、session、checkpoint 与 cwd

Workspace 打开流程已经分开处理三类状态：

1. Canvas snapshot 和 layout 先恢复。
2. `session_checkpoints`、`node_sessions` 随后加载。
3. 只有 Term/Agent 延迟恢复 live session；Preview/Browser 是静态描述节点。

证据：

- 打开 workspace 时加载 Canvas、layout、checkpoint、node session：[`crates/app/src/workspace/flow.rs:309-450`](../../crates/app/src/workspace/flow.rs#L309-L450)
- Preview 恢复后重新后台读取文件，Browser 瞬态 view 会清空重建：[`crates/app/src/workspace/flow.rs:111-126`](../../crates/app/src/workspace/flow.rs#L111-L126)
- checkpoint 只收集 Term/Agent，排除 Rect/Preview/Browser：[`crates/runtime/src/session_restore.rs:39-78`](../../crates/runtime/src/session_restore.rs#L39-L78)
- node session 只记录 Terminal cwd 和 Agent session ID：[`crates/runtime/src/session_snapshot.rs:19-59`](../../crates/runtime/src/session_snapshot.rs#L19-L59)
- Terminal 恢复 cwd、Agent 恢复 provider session 的边界：[`crates/runtime/src/session_restore.rs:175-243`](../../crates/runtime/src/session_restore.rs#L175-L243)
- Canvas/session 脏标记使用 360ms debounce；无脏数据快速返回：[`crates/app/src/workspace/persistence.rs:8-115`](../../crates/app/src/workspace/persistence.rs#L8-L115)

衔接结论：

- Idea 实体不属于 session/checkpoint/cwd。
- 在某个 workspace 打开的 Idea Preview/Browser 由现有 Canvas snapshot 恢复。
- workspace 关闭不应删除或归档 Idea。
- Idea 写入需要独立 background writer，不能复用 PTY/session 热路径。

### 4.6 Norma 语音/对话入口与工具路由

当前连续语音链路已经把最终 transcript 交给唯一的 Norma Agent：

- ASR FinalTranscript 进入 Dialogue：[`crates/app/src/app/dialogue/chat.rs:235-279`](../../crates/app/src/app/dialogue/chat.rs#L235-L279)
- Voice turn 以 `AgentInputSource::Voice` 提交给 Norma：[`crates/app/src/app/dialogue/reply_loop.rs:103-126`](../../crates/app/src/app/dialogue/reply_loop.rs#L103-L126)
- Norma 每回合生成 turn id，并把原始 user input、input source 和 operation context 交给工具：[`crates/agent/src/master.rs:177-207`](../../crates/agent/src/master.rs#L177-L207)
- 当前 12 个 typed tools 已包含创建 Browser/Preview：[`crates/agent/src/rig_tools.rs:169-180`](../../crates/agent/src/rig_tools.rs#L169-L180)
- 工具 JSON schema 的标准位置：[`crates/agent/src/adapter.rs:249-349`](../../crates/agent/src/adapter.rs#L249-L349)
- `create_preview` 在 app adapter 中解析 workspace 路径并创建卡片：[`crates/app/src/app/agent_runtime/adapter_pump.rs:304-329`](../../crates/app/src/app/agent_runtime/adapter_pump.rs#L304-L329)

重要边界：

- `crates/agent` 保持零 Storage；Idea 工具只定义命令和 DTO，真实存储由 app adapter/Storage 执行。
- 现有 adapter pump 在 GPUI `update_in` 内同步执行命令：[`crates/app/src/app/agent_runtime/adapter_pump.rs:7-38`](../../crates/app/src/app/agent_runtime/adapter_pump.rs#L7-L38)。Idea 文件 IO、哈希和索引不得照搬成前台同步操作。
- 当前 operation receipt 是进程内 cache：[`crates/agent/src/adapter.rs:183-245`](../../crates/agent/src/adapter.rs#L183-L245)。Ideas Vault 需要把 `operation_id` 持久化，避免重试产生重复 Idea。
- NormaSession 的 `turn-N` 是会话内递增值，不是跨重启稳定 ID；来源字段不能只存它。

### 4.7 存储层和全局 workspace identity

- 应用数据目录位于 `~/Library/Application Support/Norma OS/`：[`crates/storage/src/app_store.rs:72-104`](../../crates/storage/src/app_store.rs#L72-L104)
- `AppStore` 当前承载 recent/settings/workspace identity/全局 orchestration：[`crates/storage/src/app_store.rs:47-61`](../../crates/storage/src/app_store.rs#L47-L61)
- 项目级数据库位于 `<workspace>/.multisoul/project.sqlite`：[`crates/storage/src/project_store.rs:18-38`](../../crates/storage/src/project_store.rs#L18-L38)
- 全局 `workspace_id` 已可由 canonical root 稳定解析：[`crates/storage/src/workspace_identity_store.rs:26-56`](../../crates/storage/src/workspace_identity_store.rs#L26-L56)
- 项目数据库已有 voice history，可作为语音来源的可选线索：[`crates/storage/src/migrations.rs:189-203`](../../crates/storage/src/migrations.rs#L189-L203)

结论：统一 Vault 应扩展 AppStore，而不是塞进某个 ProjectStore；Idea 与项目通过 `workspace_id` 建立关系。

### 4.8 现有能力与缺口汇总

| 需要 | 已有 | 缺口 |
|---|---|---|
| 语音进入 Norma | 已有完整 ASR → Norma turn | 没有 Idea 专用工具和意图规则 |
| 创建内容卡片 | 已有 Preview/Browser 工具和布局 | 没有打开 Vault/Idea 的高层命令 |
| 详情/图片查看 | Preview 已支持 Markdown/Text/Image、多图 | 不可编辑，不支持视频/PDF |
| 网页查看 | Browser 已支持 URL 与持久化恢复 | 没有下载到 Vault 的受控桥接 |
| Canvas 恢复 | Snapshot + Preview reload + Browser rebuild | 只恢复展示引用，不是 Idea 数据存储 |
| 全局存储 | AppStore + workspace identity | 没有 Ideas tables、manifest、asset store |
| 检索 | SQLite 可承载索引 | 没有 Idea search API；FTS5 能力需先验证 |
| 幂等 | Norma 有进程内 operation receipt | 没有跨重试/跨重启的 durable operation receipt |

## 5. 产品信息架构

### 5.1 Idea 是业务实体，不是 Canvas 节点

```text
Idea（应用级业务实体）
├── 原始输入与结构化内容
├── 标签、状态、时间和来源
├── workspace 关系
└── attachments

Canvas（项目级展示状态）
├── Preview → 某个 Idea 的 idea.md / 图片
└── Browser → 某个网页素材 / 后续 Library 页面
```

删除 Canvas 卡片不删除 Idea；归档 Idea 也不应自动删除其他 workspace 的卡片。硬删除必须显式执行，并先检查引用。

### 5.2 状态模型

P0 建议只提供五个稳定状态：

| 状态 | 含义 | 默认进入方式 |
|---|---|---|
| `draft` | 已捕获，可能不完整 | 新 Idea 默认 |
| `ready_for_review` | 信息足够，等待判断 | 用户明确说“待评审/可以评审了” |
| `adopted` | 已决定进入实现或路线图 | 用户明确采纳，并可关联目标 |
| `parked` | 暂不做，但保留 | 用户明确搁置 |
| `archived` | 从活跃视图隐藏 | 用户归档 |

“Inbox”不是第六个状态，而是 `status=draft AND triaged_at IS NULL` 的系统视图。这样不会把“是否整理过”和“决策状态”混为一谈。

### 5.3 分类与标签

- `category`：零或一个低频、稳定的一级分类，例如 `product`、`ux`、`technical`、`growth`、`research`。
- `tags`：零到多个自由标签，统一做大小写、空白和 Unicode 归一化。
- Norma 可建议 category/tags，但 UI 必须明确它们可编辑，不把模型推断当事实。
- P0 不强制 taxonomy；无标签 Idea 仍能保存和搜索。

## 6. 录入入口与结构化策略

### 6.1 触发规则

强触发示例：

- “Norma，我有一个想法……”
- “帮我记一下这个点子……”
- “把这个存到想法库……”
- “新建一个想法：……”

弱表达如“我在想也许可以……”默认仍是普通对话，除非用户开启未来的 opt-in 自动捕捉模式。系统不应静默把所有脑暴对话持久化。

### 6.2 保存前后顺序

推荐顺序：

```text
1. Norma 识别明确 capture intent
2. capture_idea(raw_text, suggested_structure, source)
3. IdeaStore 生成稳定 ID，原子写入 manifest + idea.md
4. durable operation receipt 落盘
5. 返回真实 succeeded receipt
6. Norma 才说“已保存”，并给出标题、ID、状态、下一步
```

捕获是低风险、可撤销动作，所以不先弹模态确认。若结构不完整，也先保存 Draft，再问至多一个高价值问题，例如“这个想法主要面向谁？”；用户不回答也不影响已保存内容。

### 6.3 字段提取规则

| 字段 | 来源 | 不确定时处理 |
|---|---|---|
| `title` | 第一核心主张，建议 8–24 字 | 从原文首句生成临时标题，可编辑 |
| `summary` | 一句话价值描述 | 为空也允许保存 |
| `body` | 原始内容的结构化展开 | 不补写用户没说过的事实 |
| `category` | 用户明确值优先，其次模型建议 | 低置信度留空 |
| `tags` | 显式标签 + 高置信度建议 | 最多 5 个首批建议，不强制 |
| `status` | 用户明确状态，否则 `draft` | 不自动标成 adopted |
| `source` | App 真实上下文 | 由 Adapter 填充，不让模型猜 |
| `attachments` | 节点、文件选择器、URL、拖入 | 由 Adapter 验证和复制 |

### 6.4 原文保护

- `source.raw_text` 完整保留首次输入。
- `idea.md` 的“原始记录”区域不可被自动整理覆盖。
- 后续补充以 revision/event 追加；编辑后的当前视图与原始记录并存。
- 模型生成标题或摘要时记录 `generated_by` 和 provider/model 可选线索，不把它伪装成人工输入。

### 6.5 建议的 Norma tools

这些是未来接口规划，不在本次任务实现：

| Tool | 责任 | 是否产生 Canvas 副作用 |
|---|---|---|
| `capture_idea` | 保存 raw + 结构化字段，返回 Idea ID | 否 |
| `search_ideas` | 关键词/标签/状态/workspace 查询 | 否 |
| `get_idea` | 获取单个 Idea 摘要和素材列表 | 否 |
| `update_idea` | 更新正文、标签、状态或追加说明 | 否 |
| `attach_idea_asset` | 从选中 Preview/Browser、路径或 URL 添加素材 | 否；可选刷新已开详情 |
| `open_idea` | 复用 Preview/Browser 在 Canvas 打开详情/素材 | 是 |
| `open_ideas_vault` | 打开检索结果或 P1 Library | 是 |

写入工具与展示工具分开，避免“每记一个想法就自动创建一张卡片”。所有写工具接收 `operation_id` 并由 Store durable 去重。

## 7. 存储方案

### 7.1 方案比较

| 方案 | 优点 | 缺点 | 结论 |
|---|---|---|---|
| `<workspace>/.multisoul/ideas/` | 与当前项目数据接近 | 隐藏、被 git ignore、跨项目分散、远端信任边界复杂 | 不推荐作为统一库 |
| `<workspace>/ideas/` | 用户可见、可入 Git | 污染项目、不同项目分散、可能把私人想法提交到远端 | 仅作为显式导出目标 |
| App Support 全局目录 | 真正统一、默认私有、与 AppStore/workspace identity 契合 | Finder 不直观，备份/迁移需产品入口 | **推荐 canonical** |

### 7.2 推荐目录结构

```text
~/Library/Application Support/Norma OS/
├── app.sqlite
└── ideas-vault/
    └── v1/
        ├── ideas/
        │   └── <idea-uuid>/
        │       ├── manifest.json
        │       ├── idea.md
        │       ├── design.md                 # 可选的扩展设计说明
        │       ├── assets/
        │       │   ├── <asset-id>-prototype.png
        │       │   └── <asset-id>-demo.mov
        │       └── derived/
        │           └── <asset-id>-poster.jpg # 可删除、可重建
        ├── views/
        │   ├── inbox.md                      # P0 可重建视图
        │   └── recent.md                     # P0 可重建视图
        ├── exports/
        └── trash/                            # 软删除保留区
```

目录使用 canonical UUID，避免标题变更导致路径移动。用户可见的短 ID 单独存为 `display_id`，例如 `IDEA-260804-A7K2`。

### 7.3 Source of truth 与索引

- `manifest.json`、`idea.md`、附件是 canonical 数据。
- `app.sqlite` 中的 Ideas tables 是查询索引和 operation receipt，可从 manifest 重建。
- 写入顺序为：临时目录/文件 → fsync（按风险决定）→ atomic rename → index transaction。
- 若索引更新失败，Idea 仍安全存在；后台 reconciliation 重新索引。
- 若 manifest 写入失败，不能返回 succeeded。

这样避免 SQLite 与大文件形成不可恢复的单点，也避免二进制 blob 膨胀。

### 7.4 建议的文件命名规则

- 目录：canonical UUID，不使用标题。
- 附件：`<asset-id>-<sanitized-original-name>`。
- 去除 `/`、NUL、控制字符和保留路径片段；限制文件名长度。
- 同内容附件按 SHA-256 去重，但保留各自 caption/source 关系。
- P0 默认复制普通文件；明确选择“保留外部链接”时才存外部 locator。

### 7.5 与 workspace 恢复的衔接

- Idea 本体随 app 启动可用，不随 workspace 打开/关闭迁移。
- 某 workspace 打开的 Idea Preview/Browser 仍由该 workspace 的 `canvas_snapshots` 保存位置和 URL/path。
- 重开 workspace 后 Preview 重新读取 `idea.md`；Browser 按持久化 URL 重建。
- Idea 不写入 `session_checkpoints` 或 `node_sessions`，也没有 cwd。
- 为避免 Preview path 失效，Idea UUID 目录创建后不移动；状态变化只改 manifest/文档。
- 归档不移动 canonical 目录；硬删除才进入 `trash/`，并保留 tombstone 期。

## 8. 数据模型

### 8.1 Canonical manifest 示例

```json
{
  "schema_version": 1,
  "id": "f079c60d-9a70-4e81-a124-7aa8d66c6ea9",
  "display_id": "IDEA-260804-A7K2",
  "title": "用对话快速记录产品想法",
  "summary": "让用户不离开当前工作流就能捕获并找回零散想法。",
  "status": "draft",
  "category": "product",
  "tags": ["ideas", "voice", "canvas"],
  "created_at": "2026-08-04T08:30:00Z",
  "updated_at": "2026-08-04T08:30:00Z",
  "triaged_at": null,
  "revision": 1,
  "source": {
    "kind": "voice",
    "raw_text": "Norma，帮我记一下……",
    "workspace_id": "0f5c...",
    "workspace_display_name": "norma-ai-os",
    "norma_turn_id": "turn-18",
    "voice_history_id": 320
  },
  "workspace_links": [
    {"workspace_id": "0f5c...", "relation": "captured_in"}
  ],
  "attachments": [
    {
      "id": "asset-6ac1...",
      "kind": "image",
      "storage": "copied",
      "relative_path": "assets/asset-6ac1-prototype.png",
      "original_name": "prototype.png",
      "mime_type": "image/png",
      "bytes": 142033,
      "sha256": "...",
      "caption": "首版交互草图",
      "created_at": "2026-08-04T08:31:00Z"
    }
  ],
  "capture_operation_id": "turn-18:tool-call-2"
}
```

规则：

- `norma_turn_id` 仅作诊断线索，不能作为全局主键。
- `workspace_id` 来自真实 AppStore identity，不由模型填写。
- `voice_history_id` 是可选弱引用；跨库记录删除不影响 Idea。
- `raw_text` 是事实，`title/summary/tags` 可由用户修订。
- URL attachment 不强制有 `relative_path`，但必须有 normalized URL、显示 host 和隐私友好的日志摘要。

### 8.2 App SQLite 索引建议

```sql
CREATE TABLE ideas (
  id                  TEXT PRIMARY KEY NOT NULL,
  display_id          TEXT UNIQUE NOT NULL,
  title               TEXT NOT NULL,
  summary             TEXT NOT NULL,
  status              TEXT NOT NULL,
  category            TEXT,
  manifest_relpath    TEXT UNIQUE NOT NULL,
  search_text         TEXT NOT NULL,
  source_workspace_id TEXT,
  revision            INTEGER NOT NULL,
  created_at_unix     INTEGER NOT NULL,
  updated_at_unix     INTEGER NOT NULL,
  archived_at_unix    INTEGER,
  capture_operation_id TEXT UNIQUE NOT NULL
);

CREATE TABLE idea_tags (
  idea_id TEXT NOT NULL,
  tag_key TEXT NOT NULL,
  tag_display TEXT NOT NULL,
  PRIMARY KEY (idea_id, tag_key)
);

CREATE TABLE idea_workspaces (
  idea_id TEXT NOT NULL,
  workspace_id TEXT NOT NULL,
  relation TEXT NOT NULL,
  PRIMARY KEY (idea_id, workspace_id, relation)
);

CREATE TABLE idea_assets (
  id TEXT PRIMARY KEY NOT NULL,
  idea_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  storage TEXT NOT NULL,
  relative_path TEXT,
  url TEXT,
  sha256 TEXT,
  bytes INTEGER,
  created_at_unix INTEGER NOT NULL
);

CREATE INDEX idx_ideas_status_updated
ON ideas(status, updated_at_unix DESC);

CREATE INDEX idx_idea_tags_key
ON idea_tags(tag_key, idea_id);

CREATE INDEX idx_idea_workspaces_workspace
ON idea_workspaces(workspace_id, idea_id);
```

P0 搜索使用规范化 `search_text` + `LIKE`、标签和状态索引即可；目标数据量通常不大。P1 只有在确认 bundled SQLite 的 FTS5 能力或明确启用相应构建特性后才增加 `ideas_fts`，不能假定当前 `rusqlite = { features = ["bundled"] }` 已满足所有 FTS 能力。

### 8.3 领域边界

- Ideas 纯模型、校验、query DTO 应位于独立零 GPUI 领域层或 storage 邻接模块；不进入 `canvas`。
- `storage` 负责 app-level index、migration 和 background writer。
- `agent` 只拥有 typed tool contract，不依赖 storage。
- `app` 负责把 Norma tool、文件选择、当前选中节点和 Canvas 展示副作用组合起来。
- `canvas` 继续只持有 Preview/Browser 描述节点；P0 不改 `NodeKind`。

## 9. 富内容素材流程

### 9.1 文件选择器

P0 最稳妥入口。用户在 capture receipt 或 Idea 详情上选择“添加附件”，系统打开文件选择器：

1. 选择一个或多个文件。
2. 后台校验普通文件、大小、可读性和路径安全。
3. 默认复制到 Idea `assets/`。
4. 计算 SHA-256、记录大小和 MIME/扩展名。
5. 原子更新 manifest 和索引。
6. 对图片生成可重建缩略图；完成后给非阻断反馈。

当前 Preview 文件选择器已经能选择本地文件和多图，可借鉴其异步入口，但“复制进 Vault”是新增语义，不能只保存外部绝对路径。

### 9.2 从 Preview 卡片加入 Idea

用户先打开或选中一张 Preview，然后说：

> “把这张原型图加到刚才的想法。”

推荐 Tool 参数使用 `source_node_id`，而不是让模型读取或重写绝对路径。App Adapter 在执行时：

1. 重新校验 node 仍存在且是 Preview。
2. 从真实 `PreviewSpec` 取得一个或多个路径。
3. 显式确认“复制”还是“保留链接”；P0 默认复制。
4. 后台导入素材并返回逐项结果。

当前 Norma 的 Canvas context 只包含 node id、kind、title、selected/focused，不包含 Preview path；这正好避免把敏感路径注入 prompt：[`crates/agent/src/current_context.rs:22-39`](../../crates/agent/src/current_context.rs#L22-L39)。

### 9.3 从 Browser 加入 Idea

区分两个动作：

- “把这个网页加到 Idea”：保存 URL reference、标题、host、添加时间；P0 可做。
- “把这个下载下来加到 Idea”：需要 Browser 下载事件、目标文件安全检查、进度、失败和取消；当前没有，放 P1。

同样使用 `source_node_id`，由 Adapter 从真实 BrowserSpec/当前 location 解析 URL。不要让模型复制地址栏文本作为事实来源。

### 9.4 拖入素材

当前仓库没有通用的 Canvas 外部文件 drop → Preview/Idea 导入闭环，因此拖入是 P1 新能力，而不是可直接复用能力。

建议原型：

- 拖到 capture receipt：添加到刚创建的 Idea。
- 拖到 Ideas Library 的某一行：目标行高亮，松手后导入。
- 拖到 Idea 详情素材区：1:1 跟手并显示“复制到 Ideas Vault”。
- 拖到空白 Library：先创建 Draft，再导入素材，标题取文件名并提示补充描述。
- 进入 drop target 时立即反馈；越界后恢复；导入任务可取消。

### 9.5 视频

P0：

- 复制、记录 metadata、生成可选 poster。
- Idea 详情显示文件名、时长（若可安全读取）、大小和“打开/在 Finder 中显示”。
- 使用系统默认播放器或 Quick Look，不承诺 Canvas 内播放。

P2：

- 若要新增 `PreviewType::Video` 或专用媒体显示，必须遵守仓库低拷贝约束：解码后台化，上屏走 420f `surface()` 或等价路径，不能每帧 CPU RGBA/`RenderImage`。

## 10. 查看与检索方案

### 10.1 P0：Norma 搜索 + Preview 打开

用户说：

> “找一下关于语音输入的想法，只看待评审的。”

Norma 调用 `search_ideas(query, status, tags, workspace_id)`，返回最多 5–10 条：

```text
1. IDEA-260804-A7K2 · 对话式想法收集 · draft
2. IDEA-260731-Q2LM · Voice Command Palette · ready_for_review
```

用户说“打开第二个”，Norma 调用 `open_idea`，复用 Preview 打开对应 `idea.md`。图片可打开为第二张 Preview；网页引用用 Browser 打开。

为减少卡片泛滥：

- 同一 Idea 已在当前 Canvas 打开时默认复用并刷新。
- 只有用户明确说“再开一份/创建副本”时才创建第二张。
- 单次搜索不自动把所有结果各自开成卡片。

### 10.2 P0 可选：生成只读结果视图

`open_ideas_vault` 可生成 `views/inbox.md` 或本次 search result Markdown，用 Preview 打开。它适合作为可保存、可回看的结果摘要，但当前 Markdown Preview 不具备点击进入某个 Idea 的确定性导航，因此仍需 Norma 的“打开第 N 个”语音交互。

### 10.3 P1：Browser-backed Ideas Library 原型

当用户验证需要高频鼠标浏览后，推荐先用现有 Browser 节点承载受控本地 Library，而不是立刻新增 NodeKind。

```text
┌ Ideas Vault ───────────────────────────────────────────┐
│ 24 ideas     [Search ideas…                  ] [＋]    │
│ [Inbox 8] [Draft] [Review] [Adopted] [Archived]       │
├───────────────────┬────────────────────────────────────┤
│ IDEA-…  语音输入  │ 用对话快速记录产品想法             │
│ draft · voice     │ Summary…                           │
│ updated 2h        │ Tags: voice  canvas                │
│                   │                                    │
│ IDEA-…  预览卡片  │ Attachments                        │
│ review · canvas   │ [prototype.png] [demo.mov]         │
└───────────────────┴────────────────────────────────────┘
```

原型原则：

- 左侧列表负责 wayfinding，右侧 inspector 展示详情；不在默认态铺满编辑控件。
- 搜索输入立即反馈匹配数量；状态和标签过滤可组合、可清除。
- 单击选中，双击或 Return 打开详情；Space 可 Quick Look 素材。
- 状态更改提供 Undo；硬删除进入 Trash，不弹出无意义的频繁确认。
- 生成页面必须使用严格 CSP、HTML escaping 和受控资源路径。
- P1 若需要 HTML → App action，新增窄 IPC contract，只允许 `openIdea/attachAsset/updateStatus` 等白名单动作，并由 App 重新校验 ID 和权限。

### 10.4 是否新增 Ideas 节点类型

**P0/P1：不建议。** Preview/Browser 已能承担查看和初步管理，新增 NodeKind 会触发 Canvas、Card registry、minimap、label、session exclusion、Agent context、protocol summary 等多处穷尽分支。

**P2 决策门槛：** 同时满足以下条件才考虑 `IdeasLibrary` 非 Live 节点：

- 用户每周多次在 Canvas 内做多选、批量状态变更或拖拽归类。
- Browser-backed Library 因焦点、可访问性、性能或原生交互明显不足。
- 原生节点能把完成核心任务的步骤数至少降低 30%，或显著提高找回率。
- 有完整的 domain model、输入焦点、持久化和 L3 视觉验收方案。

## 11. 建议原型交互流程

### 11.1 流程 A：一句话快速捕获

```text
用户：Norma，记一下：Preview 卡片可以直接挂原型图和演示视频。

系统：
  1. 识别显式 capture intent。
  2. 保存 raw_text。
  3. 建议标题“Preview 卡片挂接原型素材”。
  4. 默认 status=draft，tags=[preview, prototype]。
  5. 真实写入完成后返回 IDEA-260804-A7K2。

Norma：已记下 IDEA-260804-A7K2「Preview 卡片挂接原型素材」，
       现在是草稿。你可以继续补充，或者让我打开它。
```

UI 同时显示非模态 receipt：

```text
┌ Saved to Ideas Vault ──────────────────────────────┐
│ IDEA-260804-A7K2 · Draft                           │
│ Preview 卡片挂接原型素材                           │
│ [Open] [Add attachment] [Undo]                     │
└─────────────────────────────────────────────────────┘
```

### 11.2 流程 B：信息不足也不丢

```text
用户：帮我记个想法，做一个更聪明的启动页。
系统：先保存 Draft 原文。
Norma：已经记下「更聪明的启动页」。你想解决的是项目选择，
       还是首次上手？不补也没关系，它已经在草稿箱里了。
```

只问一个能显著改善检索/决策的问题；不把用户拖进表单采访。

### 11.3 流程 C：把选中的原型图加入刚才的 Idea

```text
用户选中 prototype.png Preview
用户：把这张图加到刚才那个想法。
系统：
  1. 解析“刚才那个”为上一个 succeeded capture receipt。
  2. Adapter 校验选中 node_id 是 Preview。
  3. 后台复制文件、哈希并更新 manifest。
  4. 返回 asset id。
Norma：原型图已加入 IDEA-260804-A7K2。
```

若“刚才那个”无法唯一解析，列出最多三个候选，不猜测。

### 11.4 流程 D：检索并回看

```text
用户：找一下关于 Preview 和视频的想法。
Norma：找到 3 个，最相关的是……
用户：打开第一个。
系统：复用或创建 Preview，打开该 Idea 的 idea.md。
```

### 11.5 流程 E：状态推进

```text
用户：把这个想法标成待评审，关联到当前项目。
系统：update_idea(status=ready_for_review)
     + idea_workspaces(current_workspace, relation=planned_for)
Norma：已更新为待评审，并关联当前项目。 [Undo]
```

### 11.6 失败与 unknown

- manifest 未落盘：明确说“没有保存成功”，保留 transcript 供重试。
- manifest 成功、index 失败：返回“已保存，搜索索引正在修复”，不能制造第二条 Idea。
- 工具回执 unknown：使用 operation id 查询；在确认前不自动重试创建。
- 附件部分失败：逐项返回成功/失败，不回滚已复制的其他附件。
- 源 Preview/Browser 在执行前已关闭：拒绝并提示重新选择，不猜路径/URL。

## 12. Apple 设计原则如何落地

本方案参考 `apple-design`，但不照搬 Web API；未来 GPUI 实现仍以仓库钉死版本和现有组件为准。

| 原则 | 设计落地 | 验收证据 |
|---|---|---|
| Purpose / Simplicity | P0 只做 capture/search/open，不先造新节点和复杂 taxonomy | 原型任务步数与用户测试 |
| Agency | 只在显式意图下保存；状态更改和归档可 Undo | L1/L3 操作验证 |
| Response | 录音结束、保存开始、保存成功/失败都有即时状态 | Computer Use 真实 App 验收 |
| Familiarity | 使用 Library、Inbox、Draft、Search、Quick Look 等 macOS 熟悉概念 | 5 人可用性测试 |
| Spatial consistency | 从 receipt 的 Open 进入对应 Preview；关闭沿原路径返回 Canvas | L3 视觉/焦点验证 |
| Direct manipulation | 拖入素材时 1:1 跟手，drop target 连续高亮 | L3 拖拽验证 |
| Interruptibility | 导入/搜索/展开动效可中断，不锁输入 | 慢放审查 + L3 |
| Accessibility | 完整键盘路径、明确 focus、对比度、VoiceOver 标签、结果数量 | Accessibility audit + L3 |
| Reduced motion | 关闭位移/弹簧，改短 cross-fade；保留状态反馈 | 系统 Reduce Motion 下验收 |

动效策略：

- 保存 receipt 采用短 materialize/cross-fade，不用庆祝性弹跳。
- 拖拽素材才允许轻微 momentum；普通列表切换不使用 bounce。
- 状态变更使用颜色、文字和图标共同表达，不能只依赖颜色。
- 高透明材质在 Reduce Transparency 下切换为高不透明面板。

所有按钮、筛选 chip、列表行必须覆盖适用的 hover、pressed、keyboard focus、selected/toggled 和 disabled 状态。

## 13. P0 / P1 / P2 实施建议

### P0：可靠捕获和可找回闭环

目标：证明用户会持续捕获、之后能找到并打开，而不是证明能做一个漂亮库页面。

范围：

- App-level Idea model、manifest、asset directory、SQLite index/rebuild。
- `capture_idea/search_ideas/get_idea/update_idea/open_idea/attach_idea_asset` typed tools。
- 显式 capture intent 规则和 Master prompt 边界。
- Draft/Review/Adopted/Parked/Archived 状态。
- 文件选择器导入；从已选 Preview 复制图片/文档；Browser URL reference。
- Preview 展示 `idea.md`、图片和可重建 inbox/search Markdown。
- 视频保存 + 系统打开，不做 Canvas 内播放。
- durable operation receipt、后台 IO、原子写、index reconciliation。
- 非模态保存反馈、Undo、错误/unknown 语义。

不做：

- 专用 Ideas NodeKind。
- Browser 下载接管、Canvas 外部拖入、批量编辑。
- FTS/embedding/相似合并。
- 云同步或多人协作。

建议验证窗口：至少 2 周 dogfood，观察每周捕获数、二次打开率、搜索成功率和手动修正标签比例。

### P1：高频管理体验

进入条件：P0 的二次打开率和搜索使用表明用户真的回来管理 Idea。

范围候选：

- Browser-backed Ideas Library：列表、组合筛选、inspector、键盘导航。
- 窄白名单 IPC actions 和严格 CSP。
- Canvas/Library 拖入素材、剪贴板截图导入。
- Browser 受控下载 → Idea asset，含进度/取消/容量确认。
- 相似 Idea 提示和人工 merge。
- 关联 workspace、issue/roadmap、采纳目标。
- 搜索高亮、保存搜索；验证后引入 FTS5。
- 导出/Reveal in Finder/备份提示。

### P2：专用原生体验与扩展

只有数据证明必要时：

- 原生 `IdeasLibrary` 非 Live node 或 app-level Library window。
- 内联编辑、批量状态、空间分组。
- `PreviewType::Video` 或安全的媒体查看路径。
- 本地语义检索/embedding，索引可删除重建。
- 可选 iCloud/文件夹同步、冲突处理、版本历史。
- 从 adopted Idea 生成 PRD/issue/exec plan 的明确工作流。

## 14. 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| 模型过度结构化或误解 | 改写用户真实意图 | 永久保留 raw；低置信度字段留空；可 Undo/编辑 |
| 捕获依赖 LLM 成功 | Provider 失败时灵感丢失 | P0 保留 raw capture fallback；tool 不成功不声称已保存 |
| operation retry 重复创建 | 出现重复 Idea | app-level unique operation id + Store 查询回执 |
| App Support 不可见 | 用户担心锁定/备份 | Reveal in Finder、导出、自包含目录、备份提示 |
| 大视频占满磁盘 | 性能与容量问题 | 导入前显示大小；哈希去重；容量统计；不静默清理 |
| 外部路径移动 | Preview/附件失效 | 默认复制；链接模式明确标识 health |
| 索引与文件不一致 | 搜不到或显示旧状态 | manifest canonical；后台 reconciliation；索引可重建 |
| 多窗口并发写 | revision 覆盖、asset 冲突 | UUID 目录、expected revision、单 writer/文件锁、事务 |
| Idea 内容 prompt injection | 工具误执行 | 搜索结果作为不可信数据；截断/转义；不注入 system rule 区 |
| Browser 本地页面注入 | 执行任意脚本/访问文件 | 严格 CSP、转义、白名单 IPC、受管路径、L3 安全测试 |
| 敏感路径/URL 进日志 | 隐私泄露 | 只记 filename/host + hash；不记正文/query/绝对路径 |
| 软删除引用残留 | Canvas 卡片报错 | tombstone/Trash 保留期；引用检查；清晰错误与恢复入口 |
| 新节点类型扩散 | 大量穷尽式改动和回归 | P0/P1 复用现有卡片；P2 设量化门槛 |

## 15. 验收标准与自动化证据规划

本任务不实现代码，因此下面是未来实施必须满足的验收清单，而不是本次已执行的测试。

### 15.1 P0 功能验收

| 验收标准 | 自动化证据 |
|---|---|
| 说“记一下：X”后只创建一条 Draft，并返回稳定 ID | `agent` tool contract + app adapter unit + e2e voice flow |
| 保存回执前 manifest 已真实存在 | storage integration test |
| 相同 operation id 重试不产生第二条 Idea | storage/app idempotency test |
| 原始 transcript 与结构化标题都存在，且原文未被覆盖 | model/serde unit test |
| 不明确的普通讨论不会静默入库 | agent routing table test |
| 按关键词、标签、状态和 workspace 可组合检索 | storage query tests |
| “打开第一个”创建或复用 Preview，并显示正确 Idea | app unit + `crates/e2e` L1 |
| 关闭并重开 workspace 后 Idea Preview 位置恢复并重载文件 | `crates/e2e` persistence flow |
| Idea 不进入 session checkpoint/node session | runtime/storage unit test |
| 删除 Preview 卡片不删除 Idea | app/storage e2e |
| 文件选择/选中 Preview 可把附件复制进 Idea | app unit + e2e |
| Browser 页面可作为 URL attachment，但不会假装已下载 | adapter contract test |
| 视频附件可保存并通过系统打开，Preview 不误报支持 | app unit + macOS L3 |
| manifest 成功、index 失败时可重建且不重复创建 | failure injection integration test |

### 15.2 交互与视觉验收

- 保存中、成功、失败、撤销均有可感知且不阻断的反馈。
- receipt、列表行、筛选和按钮具备 hover/pressed/focus/selected/disabled 状态。
- 键盘可完成搜索、选择结果、打开、返回和状态修改。
- VoiceOver 能读出 Idea 标题、状态、更新时间、附件数量和操作。
- Reduce Motion 下不使用位移/弹簧，反馈仍完整。
- Reduce Transparency 和高对比度模式下文本与焦点边界清晰。
- 页面/布局/样式的最终验收必须用 `computer-use:computer-use` 操作真实 `.app`；headless 测试和截图仅作补充。

### 15.3 性能与可靠性验收

- Idea IO、SHA-256、thumbnail、index rebuild 和 search 不在 GPUI 前台执行。
- 1,000 条 Idea、5,000 个标签关系下，常见筛选在目标机器上交互无明显卡顿；具体阈值在实施前用基准机器定标。
- 1 GB 视频导入必须显示进度并可取消，不能冻结 Canvas。
- App 异常退出后不会留下半个 canonical Idea；临时文件可安全清理。
- Index 可从 manifests 全量重建，重建不会修改 canonical 内容。
- 日志不包含 Idea 全文、完整本地路径或 URL query。

### 15.4 建议验证命令范围

未来若进入实现，至少需要按变更范围运行：

```bash
cargo test -p agent
cargo test -p storage
cargo check -p app
cargo test -p e2e
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

若新增视频 Canvas 展示，还必须运行：

```bash
python3 scripts/check-video-wallpaper-performance.py
```

并补充针对 Ideas 媒体路径的同等级低拷贝护栏。

## 16. 原型验证计划

### 16.1 可用性任务

用 5–8 名目标用户测试以下任务，不先解释系统结构：

1. 在工作中说出一个零散想法并保存。
2. 给刚才的 Idea 添加一张截图。
3. 一天后找回“语音”相关 Idea。
4. 把某条 Idea 标为待评审并关联当前项目。
5. 删除 Canvas 上的详情卡片，再确认 Idea 仍在库中。

### 16.2 量化指标

- Capture completion rate ≥ 95%。
- 从开始说话到明确保存反馈的感知延迟可接受；记录 P50/P95。
- Search success：用户在 30 秒内找到目标 Idea ≥ 90%。
- 二次打开率：至少 20% 的 Idea 在创建后 7 天内被再次打开，作为“不是只写不看”的早期信号。
- 误捕获率 < 2%；普通对话不得无提示入库。
- 标题/标签人工修正率用于判断模型结构化质量，不以“零修改”为目标。

### 16.3 P1/P2 决策信号

- 如果用户主要通过语音搜索并打开单条 Idea，继续强化 P0，不做专用 Library 节点。
- 如果用户频繁扫描、筛选、批量改状态，推进 P1 Browser-backed Library。
- 如果 Browser 方案在焦点、键盘、拖拽或可访问性上成为主要阻碍，再评估 P2 原生 Ideas 节点/窗口。

## 17. 最终建议

先把 Ideas Vault 做成一个**应用级数据能力 + Norma tools + 现有 Preview/Browser 展示**的闭环，而不是 Canvas 新物种。P0 的产品承诺应非常克制而可信：

> 你明确让我记下的想法，我会保留原话、可靠保存、随时帮你找到，并能在画布里打开它和它的素材。

这条承诺一旦稳定成立，再用真实使用数据决定是否值得建设完整的 Library UI、Browser 下载桥接或专用 Ideas Card。
