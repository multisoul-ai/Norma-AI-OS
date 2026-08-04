# Ideas Vault 与 Norma OS 现有架构契合性研究

> **决策更新（2026-08-04）：** 本文的一手源码证据仍可参考，但“应用级 Vault / 每条 Idea 的 Canvas 投影”等早期推论已被重新审查，不再作为推荐方案。当前决策见 [`Ideas Vault：基于现有 Canvas 的原生化重设计`](../product-specs/2026-08-04-SPEC-ideas-vault-canvas-native-redesign.md)。

> 调研日期：2026-08-04
> 范围：只调查 Ideas Vault 与当前 Canvas、Runtime、Workspace 持久化、Terminal/Agent session 恢复、Preview/Browser/Agent/Terminal 创建链路及 Norma typed tools 的契合点；不包含产品实现。
> 证据口径：仓库内一手文档与生产源码。重要结论标为“已存在能力”“缺口”或“设计推论”。

## 1. 结论摘要

Ideas Vault 与 Norma OS **在展示壳层、工作区持久化和 Agent 工具编排上高度契合，在领域数据模型上尚未存在**。

- **已存在能力**：Canvas 已经把节点的可持久化描述与运行时状态分离；`Preview`、`Browser` 是没有 `LiveNode` session 的描述型卡片，`Terminal`、`Agent` 才有 PTY/session。Ideas Vault 的“打开一条想法到画布”可以沿用描述型卡片的恢复模式，而不需要伪装成终端 session。[`crates/canvas/src/node.rs:24-75`](../../crates/canvas/src/node.rs#L24) [`crates/runtime/src/session_restore.rs:39-77`](../../crates/runtime/src/session_restore.rs#L39)
- **已存在能力**：Canvas snapshot 会保存所有节点的 `kind`、bounds、z-index 和 viewport，并由项目 SQLite 中单一 JSON snapshot 恢复；工作区打开先恢复静态 Canvas，再分批恢复 Terminal/Agent live session。这为 Idea 卡片“先出现外壳、再异步加载内容”提供了直接先例。[`crates/canvas/src/document.rs:7-19`](../../crates/canvas/src/document.rs#L7) [`crates/storage/src/canvas_store.rs:16-42`](../../crates/storage/src/canvas_store.rs#L16) [`crates/app/src/workspace/flow.rs:347-361`](../../crates/app/src/workspace/flow.rs#L347) [`crates/app/src/workspace/flow.rs:428-453`](../../crates/app/src/workspace/flow.rs#L428)
- **已存在能力**：Norma 已有 Rig typed tool → `NormaToolRuntime` → `AgentCardAdapter` → GPUI App → Runtime/CardCommand 的副作用链，并已支持创建 Terminal、Browser、Preview、Agent。Ideas Vault 的 capture/search/open/archive 工具可以接入同一 seam，无需另建第二套 Agent 循环。[`crates/agent/src/rig_tools.rs:72-108`](../../crates/agent/src/rig_tools.rs#L72) [`crates/agent/src/adapter.rs:175-245`](../../crates/agent/src/adapter.rs#L175) [`crates/app/src/app/agent_runtime/adapter_pump.rs:279-328`](../../crates/app/src/app/agent_runtime/adapter_pump.rs#L279)
- **缺口**：当前没有 `Idea`/`IdeaRef`/`Vault` 领域类型、store、SQLite 表、卡片类型或 typed tool；节点类型固定为 Rect/Term/Agent/Preview/Browser，项目 store 也只暴露 Canvas、layout、session、voice、Agent 定义/任务/会话尾部等 store。[`crates/canvas/src/node.rs:59-75`](../../crates/canvas/src/node.rs#L59) [`crates/storage/src/project_store.rs:50-80`](../../crates/storage/src/project_store.rs#L50) [`crates/agent/src/rig_tools.rs:169-180`](../../crates/agent/src/rig_tools.rs#L169)
- **缺口**：`Preview` 只是只读文件投影。Canvas 只保存文件路径和类型，真实内容在 App 层后台读取；它没有稳定 Idea ID、标题之外的元数据、标签、状态、来源、检索或编辑语义。因此 Preview 可以承载低成本原型的视觉呈现，但不能单独充当 Vault 的权威数据模型。[`crates/canvas/src/node.rs:38-57`](../../crates/canvas/src/node.rs#L38) [`crates/app/src/preview/mod.rs:77-126`](../../crates/app/src/preview/mod.rs#L77) [`crates/app/src/preview/mod.rs:200-255`](../../crates/app/src/preview/mod.rs#L200)
- **缺口**：Norma 当前只看到卡片的 `node_id`、position、kind、title、selected、focused，不会看到 Preview 正文；动态环境最多纳入 64 张卡片。把 Vault 中每条 Idea 都实例化成 Canvas 节点，既不能让 Norma 可靠检索正文，也会把长期集合误建模成当前画布工作集。[`crates/agent/src/current_context.rs:22-41`](../../crates/agent/src/current_context.rs#L22) [`crates/app/src/app/agent_runtime/coordination.rs:19-67`](../../crates/app/src/app/agent_runtime/coordination.rs#L19) [`crates/agent/src/environment.rs:7-9`](../../crates/agent/src/environment.rs#L7)
- **设计推论**：最符合当前架构的边界是“**Vault 是权威集合，Canvas 是按需打开的工作投影**”。全部 Idea 应存在独立 store；Canvas snapshot 只保存已打开 Idea 的稳定引用和布局。检索应通过显式 typed tools/store query，而不是把全部 Idea 内容塞进 Norma 的每回合环境。

## 2. 调查前提与边界

仓库中没有 Ideas Vault 的既有规格或实现。本文因此只采用一个最小、可检验的能力定义：

1. 持久 capture 一条 Idea；
2. list/search/get 已保存 Idea；
3. 更新至少一种生命周期状态，例如 active/archived；
4. 可选地把某条 Idea 打开为 Canvas 卡片；
5. Norma 能通过确定性工具执行上述动作。

“是否跨项目全局共享”“是否直接编辑正文”“是否以 Markdown 文件为权威来源”都不是现有源码能够回答的事实，本文将它们保留为架构决策，而不冒充已确认需求。

仓库自身把 Norma OS 定义为“Live Node on Infinite Canvas”，并要求纯逻辑沉到零 GPUI 域层、UI 与副作用上浮到 App；存储和文件 I/O 不能阻塞 GPUI 前台。[`AGENTS.md:7-26`](../../AGENTS.md#L7) [`AGENTS.md:28-34`](../../AGENTS.md#L28) [`AGENTS.md:48-51`](../../AGENTS.md#L48) `README` 同样把 Canvas/Terminal/Runtime/Storage 与 GPUI App 分层列为当前基本结构。[`README.md:1-15`](../../README.md#L1)

## 3. 现有能力证据

### 3.1 Canvas 已经是“轻描述 + 外部运行态”的投影层

**已存在能力**：`CanvasNode` 只持有 `id`、`kind`、bounds、z-index 和选中状态。`TermSpec` 是空描述；注释明确真实 PTY/session 位于 Runtime。`AgentSpec` 也只持有 profile、alias、definition ID，真实进程/session 不在 Canvas。[`crates/canvas/src/node.rs:24-36`](../../crates/canvas/src/node.rs#L24) [`crates/canvas/src/node.rs:115-122`](../../crates/canvas/src/node.rs#L115)

**已存在能力**：`PreviewSpec` 保存一个主路径、可选附加路径和显式渲染类型，不保存文件正文；`BrowserSpec` 保存 URL、标题和 surface，不保存 HTML/cookie/cache。两者证明 Canvas 节点可以只是外部资源的可持久化引用。[`crates/canvas/src/node.rs:38-57`](../../crates/canvas/src/node.rs#L38) [`crates/canvas/src/node/browser.rs:4-28`](../../crates/canvas/src/node/browser.rs#L4) [`crates/canvas/src/node.rs:328-423`](../../crates/canvas/src/node.rs#L328)

**已存在能力**：`DocumentSnapshot` 逐项保存节点 `kind`、bounds、z-index 与 viewport；恢复时重算 `next_id`/`max_z`，不恢复选择状态。这一模型天然适合在 snapshot 中放一个稳定 `idea_id`，而不是复制 Idea 全文。[`crates/canvas/src/document.rs:7-19`](../../crates/canvas/src/document.rs#L7) [`crates/canvas/src/document.rs:45-85`](../../crates/canvas/src/document.rs#L45)

**设计推论**：若做第一等 Idea 卡片，Canvas 侧应是类似 `IdeaRefSpec { idea_id }` 的轻引用。Idea 正文、标签和生命周期不应进入 `CanvasNodeKind`，否则会把领域数据复制进单一 Canvas JSON snapshot，并使同一 Idea 多个视图之间产生双写问题。

### 3.2 Runtime 已明确区分“live session”与“静态卡片”

**已存在能力**：`LiveNode` 当前抽象的是可 drain/input/resize/scroll/view/kill 的运行节点，`NodeView` 只有 Terminal grid。Runtime 的 live registry 只服务真正运行的 Terminal/Agent，而不是所有 Canvas 类型。[`crates/runtime/src/live.rs:28-55`](../../crates/runtime/src/live.rs#L28) [`crates/runtime/src/live.rs:84-105`](../../crates/runtime/src/live.rs#L84) [`crates/runtime/src/runtime.rs:32-48`](../../crates/runtime/src/runtime.rs#L32)

**已存在能力**：Terminal 创建会构造 workspace-root cwd 的 PTY command、加入 `Term` 描述节点并注册 live/event；Agent 创建先加入描述节点，再按 profile 构造 terminal-backed live node。Browser 和 Preview 只加入 Document，不创建 live entry。[`crates/runtime/src/runtime.rs:214-235`](../../crates/runtime/src/runtime.rs#L214) [`crates/runtime/src/runtime.rs:238-278`](../../crates/runtime/src/runtime.rs#L238) [`crates/runtime/src/runtime.rs:281-295`](../../crates/runtime/src/runtime.rs#L281) [`crates/runtime/src/preview.rs:20-61`](../../crates/runtime/src/preview.rs#L20)

**已存在能力**：session checkpoint 和 node-session 持久化明确排除 Rect/Preview/Browser，只处理 Term/Agent。Terminal node-session 保存子进程 cwd；Agent 保存精确 session ID。[`crates/runtime/src/session_restore.rs:39-77`](../../crates/runtime/src/session_restore.rs#L39) [`crates/runtime/src/session_restore.rs:310-336`](../../crates/runtime/src/session_restore.rs#L310) [`crates/runtime/src/session_snapshot.rs:19-59`](../../crates/runtime/src/session_snapshot.rs#L19)

**已存在能力**：恢复 Terminal 时优先使用 node-session 中保存的 cwd，没有才回退 workspace root；恢复 Agent 时按绑定到该 node 的 session ID 构造 provider `--resume` 命令，并继续把 workspace root 作为 cwd。checkpoint 只补 shell、grid size 和 profile 等元数据，不承担 Idea/Preview 内容恢复。[`crates/runtime/src/session_restore.rs:175-243`](../../crates/runtime/src/session_restore.rs#L175) [`crates/runtime/src/session_snapshot.rs:236-288`](../../crates/runtime/src/session_snapshot.rs#L236)

**设计推论**：普通 Idea 卡片不需要新增 `LiveNode`、session checkpoint、cwd 或进程恢复。它应沿用 Preview/Browser 的静态描述型路径；只有当产品明确要求每条 Idea 自带持续运行进程时，才有理由进入 live registry。

### 3.3 Workspace 打开与恢复顺序适合异步 hydrate Idea 卡片

**已存在能力**：打开 workspace 会先校验并 canonicalize 目录，初始化项目 store，flush 前一个 workspace，然后用新 root 创建 Runtime 并加载 Canvas snapshot。[`crates/app/src/workspace/state.rs:32-58`](../../crates/app/src/workspace/state.rs#L32) [`crates/app/src/workspace/flow.rs:326-361`](../../crates/app/src/workspace/flow.rs#L326)

**已存在能力**：随后加载 layout、session checkpoints 和 node sessions，构造待恢复列表，交换 Runtime，并立即进入 Canvas；Terminal/Agent live session 再按 16ms 节拍逐个恢复。源码注释明确“先以静态 document snapshot 呈现”，避免 session I/O 延迟首帧。[`crates/app/src/workspace/flow.rs:372-429`](../../crates/app/src/workspace/flow.rs#L372) [`crates/app/src/workspace/flow.rs:175-281`](../../crates/app/src/workspace/flow.rs#L175) [`crates/app/src/workspace/flow.rs:431-453`](../../crates/app/src/workspace/flow.rs#L431)

**已存在能力**：工作区切换后 App 清空原生 Browser view/Preview 缓存，再为所有 Preview 节点触发后台文件读取；Browser 原生 view 则在 Canvas 渲染时根据保存的 URL 重建/同步。[`crates/app/src/workspace/flow.rs:121-125`](../../crates/app/src/workspace/flow.rs#L121) [`crates/app/src/preview/actions.rs:191-225`](../../crates/app/src/preview/actions.rs#L191) [`crates/app/src/canvas/view.rs:367-386`](../../crates/app/src/canvas/view.rs#L367)

**设计推论**：Idea 卡片恢复应复制这个顺序：先从 Canvas snapshot 得到 `idea_id` 和布局并绘制 loading shell，再在后台批量读取 IdeaStore，回到 GPUI 只更新卡片状态。不要在 `enter_workspace_for_path_with_source` 的前台阶段逐条读 Idea 正文。

### 3.4 项目持久化已有合适承载点，但没有 Idea store

**已存在能力**：项目级数据库默认位于 `<workspace>/.multisoul/project.sqlite`；`ProjectStore` 是 Canvas、layout、session、node-session、voice history、Agent definition/dispatch/conversation tail 等项目级 store 的聚合入口。[`crates/storage/src/project_store.rs:13-25`](../../crates/storage/src/project_store.rs#L13) [`crates/storage/src/project_store.rs:50-80`](../../crates/storage/src/project_store.rs#L50)

**已存在能力**：全局 `AppStore` 位于用户 Application Support 下的 `Norma OS/app.sqlite`，目前暴露 recent/settings/workspace identity/orchestration route 等跨项目能力。[`crates/storage/src/app_store.rs:4-19`](../../crates/storage/src/app_store.rs#L4) [`crates/storage/src/app_store.rs:47-61`](../../crates/storage/src/app_store.rs#L47) [`crates/storage/src/app_store.rs:72-104`](../../crates/storage/src/app_store.rs#L72)

**已存在能力**：Canvas 保存使用 360ms debounce；只有脏状态才写 snapshot/layout/checkpoint/node sessions。该调度器提供持久化节流先例，但它目前没有通用领域对象写入队列。[`crates/app/src/workspace/persistence.rs:8-24`](../../crates/app/src/workspace/persistence.rs#L8) [`crates/app/src/workspace/persistence.rs:55-71`](../../crates/app/src/workspace/persistence.rs#L55) [`crates/app/src/workspace/persistence.rs:94-115`](../../crates/app/src/workspace/persistence.rs#L94)

**缺口**：项目 migration 目前创建 Canvas、session、node-session、layout、voice、Agent 等表，没有 Idea/Vault 表。[`crates/storage/src/migrations.rs:153-203`](../../crates/storage/src/migrations.rs#L153) [`crates/storage/src/migrations.rs:205-268`](../../crates/storage/src/migrations.rs#L205)

**设计推论**：

- 如果 Ideas Vault 是“当前项目的想法库”，`ProjectStore` 是一致的承载点。
- 如果 Ideas Vault 是“跨项目个人收件箱”，权威记录必须放 `AppStore`，并显式记录来源 workspace identity；不能悄悄放进当前项目的 `project.sqlite`。
- 这项 scope 决策必须先于 schema，因为它决定记录所有权、删除/归档语义、迁移与跨项目检索入口。

### 3.5 四类节点已有完整创建数据流

#### Terminal

**已存在能力**：手工新建 Terminal 的 App action 计算 bounds/grid，调用 `Runtime::spawn_terminal`，激活节点、启动 motion/layout，并同时标记 Canvas 与 session 脏。[`crates/app/src/app/mod.rs:256-283`](../../crates/app/src/app/mod.rs#L256)

#### Agent

**已存在能力**：Agent 卡片从持久定义得到 `AgentSpec`，按 definition ID/alias 调 Runtime spawn，成功后激活、布局并标记 Canvas/session 脏。[`crates/app/src/app/agent_actions.rs:120-170`](../../crates/app/src/app/agent_actions.rs#L120) Agent 的创建工具还会创建/复用项目级 Agent definition，再可选激活卡片，说明“领域实体”和“Canvas 投影”已经有一个可参考的分离案例。[`crates/app/src/app/agent_runtime/create_agent_command.rs:7-67`](../../crates/app/src/app/agent_runtime/create_agent_command.rs#L7)

#### Browser

**已存在能力**：Browser 创建只向 Runtime/Document 写 URL 描述并布局；网页标题和地址变化由原生 view 事件回写 `BrowserSpec` 后标记 Canvas 脏。[`crates/runtime/src/runtime.rs:281-295`](../../crates/runtime/src/runtime.rs#L281) [`crates/app/src/app/browser.rs:221-249`](../../crates/app/src/app/browser.rs#L221) [`crates/app/src/app/browser.rs:408-437`](../../crates/app/src/app/browser.rs#L408)

#### Preview

**已存在能力**：Preview 创建把路径放进 `PreviewSpec`，选中/布局/标记 Canvas 脏，然后在后台读取 Markdown/Text/Image；恢复 workspace 时会重新读取，不把正文塞进 Canvas snapshot。[`crates/app/src/preview/actions.rs:131-185`](../../crates/app/src/preview/actions.rs#L131) [`crates/app/src/preview/actions.rs:206-239`](../../crates/app/src/preview/actions.rs#L206)

**设计推论**：Ideas Vault 的最低成本视觉原型可以复用 Preview；完整实现则更像 Agent definition 模式——Idea 是独立持久实体，Canvas 上只是其可打开/关闭的投影。

### 3.6 Norma typed tool seam 可以承接 Vault 操作

**已存在能力**：`NormaRigTool` 从统一 tool definition 表读取描述/schema，通过 `RigToolContext` 取得稳定 call ID，交给 `NormaToolRuntime.execute`；单回合最多 20 次工具调用，连续 3 次失败会耗尽失败预算。[`crates/agent/src/rig_tools.rs:8-68`](../../crates/agent/src/rig_tools.rs#L8) [`crates/agent/src/rig_tools.rs:72-125`](../../crates/agent/src/rig_tools.rs#L72)

**已存在能力**：`NormaToolRuntime` 负责 cancellation、参数解析、当前进程内按 operation ID 复用副作用回执，再调用抽象 `AgentCardAdapter`。这是新增 `capture_idea`、`search_ideas`、`open_idea` 等工具的正确领域 seam。[`crates/agent/src/adapter.rs:175-245`](../../crates/agent/src/adapter.rs#L175)

**已存在能力**：当前定义和注册了 12 个 typed tools，其中包括 create Terminal/Browser/Preview/Agent；App adapter 把工具命令转为既有 CardCommand 或专用 Preview 创建流程，并返回 `node_id` 回执。[`crates/agent/src/rig_tools.rs:169-180`](../../crates/agent/src/rig_tools.rs#L169) [`crates/agent/src/master.rs:453-481`](../../crates/agent/src/master.rs#L453) [`crates/app/src/app/agent_runtime/adapter_pump.rs:279-328`](../../crates/app/src/app/agent_runtime/adapter_pump.rs#L279)

**缺口**：现有工具只会“创建只读 Preview”，不会创建文件或持久 Idea。`create_preview` schema 只有 path/type，App adapter 也只解析 workspace 相对路径并打开它。[`crates/agent/src/adapter.rs:326-349`](../../crates/agent/src/adapter.rs#L326) [`crates/agent/src/adapter/commands.rs:179-191`](../../crates/agent/src/adapter/commands.rs#L179) [`crates/app/src/app/agent_runtime/adapter_pump.rs:304-328`](../../crates/app/src/app/agent_runtime/adapter_pump.rs#L304)

**缺口**：Rig wrapper 通过硬编码 definition index 取 schema；新增工具时必须同步更新 definition 顺序、wrapper、`build_agent` 注册、`AdapterCommand`、parser 和 App 执行分支，否则名称与 schema 可能错配。[`crates/agent/src/rig_tools.rs:72-95`](../../crates/agent/src/rig_tools.rs#L72) [`crates/agent/src/rig_tools.rs:127-180`](../../crates/agent/src/rig_tools.rs#L127) [`crates/agent/src/adapter.rs:31-82`](../../crates/agent/src/adapter.rs#L31)

**设计推论**：新增 Vault 写入工具时不能直接在 App adapter 的 `this.update_in` 闭包内同步写 SQLite/文件。当前 adapter command 正在 GPUI update 中执行，而仓库规则禁止前台文件/数据库 I/O；需要后台 store command/worker，完成后再回到 App 创建投影并返回确定性回执。[`crates/app/src/app/agent_runtime/adapter_pump.rs:7-38`](../../crates/app/src/app/agent_runtime/adapter_pump.rs#L7) [`AGENTS.md:50-51`](../../AGENTS.md#L50)

## 4. 关键缺口

### 4.1 没有权威 Idea 领域模型

**缺口**：当前 `NodeKind`/`NodeType` 只有五类，Card registry 和 Norma Canvas context 也对这五类做静态穷举分发。新增第一等 Idea 卡片不是“只加一个 enum variant”；它至少会跨 Canvas serde、App Card registry、Norma context mapping 和各处穷举 restore/cleanup 分支。[`crates/canvas/src/node.rs:59-88`](../../crates/canvas/src/node.rs#L59) [`crates/app/src/cards/mod.rs:65-79`](../../crates/app/src/cards/mod.rs#L65) [`crates/app/src/app/agent_runtime/coordination.rs:229-236`](../../crates/app/src/app/agent_runtime/coordination.rs#L229) [`crates/runtime/src/session_restore.rs:252-307`](../../crates/runtime/src/session_restore.rs#L252)

**缺口**：没有 stable Idea ID、正文、摘要、标签、状态、来源、created/updated 时间、归档或去重语义；Canvas node ID 只是当前 Document 的空间对象 ID，不应代替长期领域 ID。`Document` 的 ID 是单文档内递增分配，snapshot 恢复后继续递增。[`crates/canvas/src/document.rs:21-27`](../../crates/canvas/src/document.rs#L21) [`crates/canvas/src/document.rs:61-100`](../../crates/canvas/src/document.rs#L61)

### 4.2 Preview 可做原型，但不是完整 Vault

**缺口**：Preview 是只读加载状态；其内容枚举只有 Markdown/Text/Image，错误模型也只围绕文件存在性、类型和读取失败，没有编辑或领域生命周期。[`crates/app/src/preview/mod.rs:77-126`](../../crates/app/src/preview/mod.rs#L77) [`crates/app/src/preview/mod.rs:200-255`](../../crates/app/src/preview/mod.rs#L200)

**缺口**：Norma 的 `create_preview` 会先把相对路径解析为 `workspace_root.join(path)`，Runtime 随后把这个解析后的路径字符串写入 `PreviewSpec`。因此当前 Agent 创建的 workspace 相对 Preview 最终通常持久化为绝对路径；项目目录移动或另一台机器 clone 后，路径可移植性没有保证。[`crates/app/src/preview/mod.rs:156-174`](../../crates/app/src/preview/mod.rs#L156) [`crates/app/src/app/agent_runtime/adapter_pump.rs:304-323`](../../crates/app/src/app/agent_runtime/adapter_pump.rs#L304) [`crates/runtime/src/preview.rs:46-60`](../../crates/runtime/src/preview.rs#L46)

**设计推论**：若 MVP 采用 workspace Markdown 文件，应为 Idea 保存 workspace-relative 逻辑路径或稳定 Idea ID，并在加载时解析；不能把现有 Preview 的绝对路径行为直接当成可迁移 Vault 设计。

### 4.3 当前 Agent 环境不是知识检索层

**缺口**：`CanvasCardContext` 不含 Preview/Idea 正文、标签或摘要；App 只从 `node.kind.display_title()` 生成环境数据。即使一条 Idea 已打开为 Preview，Norma 也只知道文件名。[`crates/agent/src/current_context.rs:22-38`](../../crates/agent/src/current_context.rs#L22) [`crates/app/src/app/agent_runtime/coordination.rs:48-67`](../../crates/app/src/app/agent_runtime/coordination.rs#L48)

**缺口**：Norma environment 最多展示 64 张 Canvas 卡片，且排序优先焦点/Agent/选择。这是合理的当前工作集保护，但不适合“所有历史 Idea”枚举。[`crates/agent/src/environment.rs:7-9`](../../crates/agent/src/environment.rs#L7) [`crates/agent/src/environment.rs:45-59`](../../crates/agent/src/environment.rs#L45) [`crates/agent/src/environment.rs:148-157`](../../crates/agent/src/environment.rs#L148)

**设计推论**：Vault 必须提供显式的 list/search/get 工具，结果做数量和正文预算控制；每回合环境只需要可选注入“当前打开/选中的 Idea 摘要”，不应注入整个 Vault。

### 4.4 Canvas 不应等同于 Vault 全量集合

**缺口**：`Document` 以 `Vec<CanvasNode>` 保存所有节点，node lookup、remove 等操作是线性扫描；Canvas snapshot 又整体序列化为数据库中的单行 JSON。[`crates/canvas/src/document.rs:21-27`](../../crates/canvas/src/document.rs#L21) [`crates/canvas/src/document.rs:103-147`](../../crates/canvas/src/document.rs#L103) [`crates/storage/src/canvas_store.rs:16-42`](../../crates/storage/src/canvas_store.rs#L16)

**设计推论**：若 Vault 未来包含数百/数千条 Idea，不应自动为每条记录创建 CanvasNode。Canvas 只保存用户当前打开的一小部分投影；Vault 列表、过滤、全文搜索和归档由独立 store/query 负责。

### 4.5 可编辑 IdeaCard 还缺非 live 的文本焦点通路

**缺口**：没有 live entry 的节点通过 `Runtime::node_caps` 得到 `Caps::NONE`；Canvas 双击是否进入 node focus 又取决于 `node_caps(id).focusable`，非 focusable 卡片只会被选中。当前 runtime 键盘路由只在 `NodeFocused` 模式下把 Key 送给 `LiveNode`。[`crates/runtime/src/runtime.rs:349-378`](../../crates/runtime/src/runtime.rs#L349) [`crates/app/src/canvas/events.rs:119-139`](../../crates/app/src/canvas/events.rs#L119) [`crates/runtime/src/route.rs:60-98`](../../crates/runtime/src/route.rs#L60)

**已存在能力**：Browser 地址编辑展示了另一条 App 层输入模式：创建独立 GPUI input entity、订阅 submit/cancel 并直接取得 window focus，不依赖 `LiveNode::input`。[`crates/app/src/app/browser.rs:322-358`](../../crates/app/src/app/browser.rs#L322)

**设计推论**：只读 IdeaCard 可以直接沿用静态卡片模式；若首期要求内联编辑，应采用 App 层文本输入实体和明确的焦点桥接，或先扩展通用非 live card 输入契约，不能假定现有 Terminal focus 路径会自动适用。

### 4.6 架构文档有已知漂移，扩展时应以源码为准

**缺口**：`ARCHITECTURE.md` 的 Canvas 数据模型仍只列 Rect/Term/Agent，遗漏源码已经存在的 Preview/Browser；Agent 章节写“十个 typed tools”，而生产 `rig_tools.rs` 实际注册 12 个。[`ARCHITECTURE.md:61-66`](../../ARCHITECTURE.md#L61) [`ARCHITECTURE.md:207-217`](../../ARCHITECTURE.md#L207) [`crates/canvas/src/node.rs:59-75`](../../crates/canvas/src/node.rs#L59) [`crates/agent/src/rig_tools.rs:169-180`](../../crates/agent/src/rig_tools.rs#L169)

**设计推论**：Ideas Vault 施工时必须同步更新架构文档的穷举列表，并用源码/测试守住 NodeKind、Card registry、Agent context 与 typed tool 注册的一致性，不能继续基于旧列表扩展。

## 5. 推荐架构边界

以下是基于证据的设计推论，不是仓库已实现事实。

### 5.1 权威数据与画布投影分离

```text
IdeaStore（权威集合）
  ├── Idea ID / title / body-or-content-ref
  ├── status / tags / source / timestamps
  └── list / search / get / update / archive
                 │
                 │ open one idea
                 ▼
Canvas Document（当前工作投影）
  └── IdeaRefSpec { idea_id } + bounds + z-index
                 │
                 ▼
App Idea state/cache（后台 hydrate）→ IdeaCard renderer
```

该边界复用三个现有模式：

1. Agent definition 与 AgentCard 投影分离；
2. Preview/Browser 的外部资源引用与 App 状态分离；
3. workspace 打开时静态 snapshot 首帧与后台 live/content hydrate 分离。

### 5.2 两条可行路径

| 路径 | 复用点 | 必补能力 | 适用范围 |
|---|---|---|---|
| A. workspace Markdown + Preview | 直接复用 `PreviewSpec`、Markdown reader、Preview card、Canvas snapshot | 确定性文件创建/更新、稳定相对路径、list/search 索引、标签/状态约定、写入后台化 | 验证“快速捕获并在画布查看”是否有价值的低成本 MVP |
| B. 第一等 IdeaStore + IdeaRef card | 复用 Project/App Store、Canvas snapshot、Card registry、后台 hydrate、typed tool adapter | 新领域 DTO/store/schema、Idea card、capture/search/get/update/archive 工具、迁移与 e2e | 需要可靠检索、编辑、状态流、跨项目或长期规模的正式 Vault |

**推荐**：如果范围只是概念验证，先走 A，但明确它不是完整 Vault；如果验收已经包含搜索、归档、编辑、跨项目或大量 Idea，直接走 B，避免先把 Preview 文件路径固化成领域 ID。

### 5.3 建议的数据流

```text
用户文本/语音
  → Norma `capture_idea`
  → crates/agent 解析并校验 typed args
  → App adapter 把持久化命令送到后台
  → IdeaStore 提交成功，返回稳定 idea_id
  → 可选：GPUI 前台创建/聚焦 IdeaRef CanvasNode
  → Canvas snapshot 只保存 idea_id 与布局
  → 工具回执返回 idea_id / node_id
```

```text
用户“找出关于 X 的想法”
  → Norma `search_ideas(query, filters, limit)`
  → IdeaStore 查询
  → 返回受预算限制的稳定 ID + title + summary + status
  → 用户需要时再 `open_idea(idea_id)` 创建 Canvas 投影
```

这条链保持 `crates/agent` 零 Storage、App 执行副作用、Storage 持有 SQLite 的现有依赖方向。[`ARCHITECTURE.md:30-46`](../../ARCHITECTURE.md#L30) [`ARCHITECTURE.md:61-66`](../../ARCHITECTURE.md#L61)

## 6. 必须先决定的产品架构问题

1. **作用域**：Idea 属于项目，还是属于用户并可跨项目搜索？前者进入 `ProjectStore`，后者进入 `AppStore`。
2. **正文权威来源**：SQLite body、workspace Markdown，还是 SQLite metadata + 文件正文？这决定可移植性、Git 协作和冲突语义。
3. **Canvas 语义**：打开一条 Idea 是只读 Preview，还是可编辑 IdeaCard？若只是 Preview，不应承诺编辑和状态管理。
4. **检索语义**：首期是结构化 title/tag/status 查询，还是全文/语义检索？当前仓库没有索引或 embedding 基础设施。
5. **捕获回执**：capture 成功是否必须自动打开卡片？建议把“持久化成功”和“打开投影”分成两个事实，回执分别返回 `idea_id` 与可选 `node_id`。
6. **删除与归档**：删除 Canvas 卡片目前只移除投影；Ideas Vault 必须区分 close card、archive idea、delete idea，不能复用一个删除动作混淆领域数据与视图。[`crates/runtime/src/runtime.rs:417-437`](../../crates/runtime/src/runtime.rs#L417) [`crates/agent/src/master_prompt.rs:36-38`](../../crates/agent/src/master_prompt.rs#L36)

## 7. 最终判断

**已存在能力**足以支持 Ideas Vault 的集成骨架：Workspace 范围、项目/全局 SQLite、轻量 Canvas snapshot、静态卡片异步 hydrate、统一 Card registry，以及 Norma typed tool/Adapter 副作用链都可以直接复用。

**真正缺少的不是另一套 Canvas 或 Agent Runtime，而是 Ideas Vault 自己的权威领域层**：稳定身份、存储所有权、内容与元数据、查询/检索、更新/归档语义，以及能把一条记录按需投影到 Canvas 的 adapter。

因此最重要的架构约束是：

> 不要把“Vault 中存在一条 Idea”等同于“Canvas 上永久存在一张卡片”，也不要把只读 Preview path 当成长期 Idea identity。

在这个边界下，Ideas Vault 与 Norma OS 当前架构是顺向扩展；越过这个边界，把全部 Idea 塞进 Canvas snapshot 或 Norma environment，则会直接撞上单 JSON snapshot、线性 Document、64 卡环境上限、绝对 Preview 路径和缺少正文检索等现有边界。
