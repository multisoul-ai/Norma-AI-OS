# 手机工作空间与桌面新架构兼容性调查

调查日期：2026-10-04。证据源为 `/Users/alan/Documents/mybook/norma-ai-os` 当前工作树；分支 `dev`，HEAD `6d9a1c906a7416b7882da23fb350905236d00510`，本地比 `origin/dev` 多 5 个提交。分析包含当前未提交源码，引用行号属于调查时工作树，不能视为已发布版本。仅做源码路径核验，未启动 App、部署服务或运行旧 `crates/e2e`。

## 结论与优先级

当前手机工作群聊仍承接旧单项目架构；最新桌面已经采用多项目区域、独立会话以及 Mac 上的 Norma 人格与记忆能力。网络恢复只能让旧链路可连接，不能使两端自动指向同一 Norma、Agent 和任务。

| 优先级 | 冲突 | 用户可观察的例子 | 证据章节 |
| --- | --- | --- | --- |
| 高 | 手机与桌面是两条 Norma 推理链 | 在桌面修改人格、记忆或模型后，手机 `@Norma` 仍使用固定云端模型与独立上下文 | N1 |
| 高 | 同一目录有两套 Runtime、画布和 Agent 定义库 | 桌面创建 Alice，手机成员列表看不到她；旧 Alice 可以收到任务，但当前桌面区域没有动静 | D2、D3 |
| 高 | 手机执行目标没有桌面工作空间/项目区域定位 | 同一个 Repo 放在甲、乙两个桌面空间，手机不能指明控制哪一套会话；简单替换 ID 又会破坏授权和回执读取 | D1、D4 |
| 中 | 手机保存的是关联快照，缺少最新目录版本与失效状态 | 桌面重命名、停用或移除对象后，手机仍显示旧成员，直到派工时才出现拒绝或目标变化 | S1 |
| 高 | 设置允许保存执行层不支持的组合 | 手机把两台 Mac 的项目加入一个空间，保存成功，但发送任一群聊消息时被单 Mac 限制拒绝 | S2 |

桌面多项目规格明确把 iOS 页面、同步、身份和协议改造排除在该轮范围之外：[规格:104](/Users/alan/Documents/mybook/norma-ai-os/docs/product-specs/2026-09-29-SPEC-desktop-multi-project-workspaces.md:104)。这是保留旧手机接口的架构缺口，不能把旧协议测试通过理解为新版桌面群聊已兼容。

本调查采用 `research` 技能，以实际源码和已确认方案为证据；独立子 Agent 复核桌面执行、存储与协议路径，主 Agent 分析手机设置和 Norma 调用链。下文建议用于下一轮确认，不构成已批准的新 SPEC 或已完成的修复。

## 桌面执行与协议

### 对象关系：独立 Workspace 本身不是错误

| 对象 | 当前身份与职责 | 证据 |
| --- | --- | --- |
| 手机 `CollaborationWorkspace` | 手机 SQLite 中的讨论、项目及 Agent 引用组合；独立 UUID。`projectID` 明确保留 Mac 协议的旧 `workspace_id`。 | [CollaborationWorkspaceStore.swift:29](/Users/alan/Documents/mybook/norma-ai-os/ios/Sources/NormaCore/CollaborationWorkspaceStore.swift:29)、[同文件:50](/Users/alan/Documents/mybook/norma-ai-os/ios/Sources/NormaCore/CollaborationWorkspaceStore.swift:50) |
| 旧 `WorkspaceIdentity` | 每个 canonical root 的持久化项目 UUID；手机项目目录和权限以它为依据。不是桌面多项目工作空间 ID。 | [workspace_identity_store.rs:36](/Users/alan/Documents/mybook/norma-ai-os/crates/storage/src/workspace_identity_store.rs:36) |
| 新 `DesktopWorkspace` | 桌面多项目画布容器。源码明确它与 mobile/orchestration 的项目身份独立。 | [desktop_workspace_store.rs:15](/Users/alan/Documents/mybook/norma-ai-os/crates/storage/src/desktop_workspace_store.rs:15) |
| `DesktopProjectArea` | 某桌面工作空间内的项目区域；包含 `desktop_workspace_id`、`project_key`、实际执行目录 `root`。同一个项目可以出现于不同桌面空间。 | [desktop_workspace_store.rs:29](/Users/alan/Documents/mybook/norma-ai-os/crates/storage/src/desktop_workspace_store.rs:29)、[同文件:209](/Users/alan/Documents/mybook/norma-ai-os/crates/storage/src/desktop_workspace_store.rs:209) |
| 桌面执行 session | 按 `(desktop_workspace_id, area_id)` 保留独立 `MultisoulApp`/Runtime；CLI route 的 `workspace_id` 为 `desktop:{workspace}:{area}`。 | [desktop.rs:114](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/workspace/desktop.rs:114)、[background_project.rs:63](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/app/background_project.rs:63) |
| 手机 Agent 引用 | `machineID + projectID + definitionID`，引用 Agent 定义；`profile` 是 Codex/Claude 等执行器类型，不是运行中的会话身份。桌面运行实例另有 node/run/session。 | [CollaborationWorkspaceStore.swift:15](/Users/alan/Documents/mybook/norma-ai-os/ios/Sources/NormaCore/CollaborationWorkspaceStore.swift:15)、[coordination.rs:71](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/app/agent_runtime/coordination.rs:71) |

手机讨论空间与桌面画布空间可以有意独立。实质断点是：手机的项目/Agent 引用没有映射到新的桌面 ProjectArea 执行实例及其存储。

### D1｜确定：手机目录仍是根目录项目集合，不能表达桌面工作空间成员或选定区域

手机 snapshot 从设备 grant 的项目 ID 读取 `workspace_identities`，不是 `desktop_workspaces`/`desktop_project_areas`。全项目授权也在每次读取时展开这一旧身份表。[snapshot.rs:27](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/mobile/snapshot.rs:27)、[mobile_access_store.rs:252](/Users/alan/Documents/mybook/norma-ai-os/crates/storage/src/mobile_access_store.rs:252)。桌面加载区域时会先为 `area.root` 创建该身份，但只把内存中的 ID 改成区域 route；原表中的项目 UUID 保持不变。[background_project.rs:57](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/app/background_project.rs:57)。

**触发例：**桌面「移动端体验」和「发布质量」各加入同一个 Repo，分别有独立画布与 Agent 会话。手机“添加项目”只得到 Repo 根目录对应的一个 UUID，无法表达“控制移动端体验中的 Repo”。如果一个桌面区域尚未成功加载，`DesktopWorkspaceStore::add_project` 只写区域目录，未创建手机项目身份，因此目录注册与手机可见时刻也不一致。[desktop_workspace_store.rs:219](/Users/alan/Documents/mybook/norma-ai-os/crates/storage/src/desktop_workspace_store.rs:219)。

**影响：**桌面分组、区域数量、活跃 session 不等于手机导出的项目集合；同一 Repo 多个执行实例没有明确选取规则。目录仍能读到已有 legacy 项目，不能据此声称所有手机项目列表失效。实际手机显示取决于已有身份记录，需真实 App 验证。

### D2｜确定：手机请求项目详情会加载另一套旧 Runtime，Agent 定义也来自另一数据库

手机单项目 snapshot（也是“添加 Agent”的来源）要求 `include_cards=true`，调用旧 `workspace::sessions::ensure(database, project_uuid)`；此函数只查旧 sessions registry，缺失后执行 `PreparedProject::load` 并新建 Runtime。[controller/sources.rs:37](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/mobile/controller/sources.rs:37)、[sessions.rs:107](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/workspace/sessions.rs:107)、[sessions.rs:129](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/workspace/sessions.rs:129)。新桌面 area session 则由桌面 host 持有，显式不注册到旧 registry。[norma_route.rs:102](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/workspace/norma_route.rs:102)。

两条加载路径的存储也不同：

| 内容 | 手机目前加载的 legacy 项目 | 新桌面 ProjectArea |
| --- | --- | --- |
| 画布、会话、执行任务 | `DatabasePaths.project_db_path(canonical_root)` | app 数据目录 `desktop-canvases/SHA256(workspace:area).sqlite` |
| Agent 定义 | 上述 legacy 项目 DB 的 `agent_definition_store` | app 数据目录 `desktop-project-definitions/SHA256(project_key).sqlite` |

证据：[background_project.rs:39](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/app/background_project.rs:39)、[background_project.rs:64](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/app/background_project.rs:64)、[project_store.rs:42](/Users/alan/Documents/mybook/norma-ai-os/crates/storage/src/project_store.rs:42)、[project_store.rs:57](/Users/alan/Documents/mybook/norma-ai-os/crates/storage/src/project_store.rs:57)、[project_store.rs:119](/Users/alan/Documents/mybook/norma-ai-os/crates/storage/src/project_store.rs:119)。

**具体触发例（静态执行链已核实，未在 App 重现）：**

1. 桌面空间甲加入 Repo，并在甲/Repo 中创建 Alice；定义落在 `desktop-project-definitions`，运行节点落在甲的 `desktop-canvases`。
2. 手机工作空间加入 Repo，添加 Agent 时请求 `snapshot(project_uuid)`。[WorkspaceSettingsView.swift:222](/Users/alan/Documents/mybook/norma-ai-os/ios/App/WorkspaceSettingsView.swift:222)。
3. Mac 自动加载 Repo 的 legacy 项目 DB，创建/复用 `project_uuid` 的旧 session，读取其 Agent 定义；不会复用甲/Repo session。
4. 若旧 DB 没有 Alice，手机成员选项里没有桌面 Alice；若旧 DB 曾有另一个 Alice，手机添加的是旧定义和旧画布中的 Alice。

**影响：**可能表现为“桌面明明有 Agent，手机无法添加”，或手机能派工但桌面当前区域没有动静。同一工作目录可能出现两个 Runtime/PTY，持有不同上下文及执行记录。

桌面“项目级 Agent 定义保留”策略确实存在：`project_key` 对 Git 项目为 common dir，定义可跨桌面区域保留/共享。[project.rs:7](/Users/alan/Documents/mybook/norma-ai-os/crates/workspace/src/project.rs:7)、[project.rs:31](/Users/alan/Documents/mybook/norma-ai-os/crates/workspace/src/project.rs:31)。现有 DWS-08 测试只验证两个桌面区域读同一 `desktop-project-definitions` 及删除区域后保留，未验证与 mobile legacy DB 互通。[background_project.rs:261](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/app/background_project.rs:261)。本调查仅阅读该测试，未运行。

### D3｜确定：手机派工按项目 route 定位，并非“只命中桌面当前 Runtime”；但目前命中旧画布

手机 ThreadTask 提交把 `task.project_id` 写成命令的 `workspace_id`，使用当前 snapshot 获取的 `route_id`。[thread_task.rs:73](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/mobile/thread_task.rs:73)、[thread_task.rs:101](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/mobile/thread_task.rs:101)。桌面 endpoint 精确解析 `(workspace_id, route_id)`，再次检查目标 app 身份，从该 app 的 AgentDefinitions 按 definition ID 取得目标，再走现有 adapter 派工。[mobile_command.rs:36](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/norma/endpoint/mobile_command.rs:36)、[mobile_command.rs:52](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/norma/endpoint/mobile_command.rs:52)、[mobile_command.rs:80](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/norma/endpoint/mobile_command.rs:80)。

**触发例：**手机 `@Alice` 指向 Repo 的 legacy UUID/route，桌面用户从甲切到乙，不会使该请求自动变成乙；它仍执行 legacy session。桌面自身的群发入口则按当前 DesktopWorkspace 的 areas/sessions 定位节点，走 `submit_compact_agent_text`，没有沿用手机 ThreadTask 的领域入口。[desktop/dispatch.rs:21](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/workspace/desktop/dispatch.rs:21)、[desktop/dispatch.rs:75](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/workspace/desktop/dispatch.rs:75)。

**影响：**route index 的多项目能力仍有效；问题是两端选取的对象不一致。桌面 session 切换保留（detach window 不改 route、不 kill Runtime）本身合理，[area.rs:84](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/workspace/desktop/area.rs:84)、[background_project.rs:150](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/app/background_project.rs:150)，不能把“保留 session”误判为手机无法派工的根因。跨真实桌面区域的手机执行和回执尚未验收。

### D4｜确定：回执与权限也没有区域映射，只修改手机 projectID 不能解决

执行完成的手机轮询通过 `workspace_identity_store.get(task.project_id)` 找 root，然后读 root 的 legacy ProjectStore 中的 `agent_dispatch_task_store`。[thread_task_execution.rs:20](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/mobile/thread_task_execution.rs:20)。新桌面任务实际保存在该 session 的 area ProjectStore，派工 adapter 的持久化检查同样使用 session 自己的 ProjectStore。[mobile_realtime.rs:68](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/norma/endpoint/mobile_realtime.rs:68)。

**触发例：**若只把执行路由改成甲/Repo session，却不调整回执存储定位，手机执行查询仍读旧 DB，找不到桌面已完成的任务。若直接让手机使用 `desktop:甲:区域` 字符串，它也不在现有 grant 项目目录，并被 `ThreadTask.validate` 的项目 ID 字符校验拒绝（不允许冒号）。[mobile_access_store.rs:254](/Users/alan/Documents/mybook/norma-ai-os/crates/storage/src/mobile_access_store.rs:254)、[thread_task.rs:62](/Users/alan/Documents/mybook/norma-ai-os/crates/norma/src/mobile/thread_task.rs:62)。

**影响：**修复需要统一 catalog → grant → 执行 instance → durable task store → terminal/follow-up/receipt 的定位，不能只替换一个 ID 或从多个同根 Runtime 中任取一个。当前正常手机拿到的 legacy route 并不因这一冒号限制失败；这是新映射方案必须处理的边界。

### D5｜确定：一次性设备授权和后台生命周期仍可运行，语义需与新的“移除区域”区分

设备全项目授权按 `workspace_identities` 动态展开，可以覆盖其后注册的根目录项目；没有发现它自动扩展到 DesktopProjectArea route。[mobile_access_store.rs:252](/Users/alan/Documents/mybook/norma-ai-os/crates/storage/src/mobile_access_store.rs:252)。普通手机命令每次鉴权、校验 grant revision，未因为新桌面 session 模型绕过这些检查。[thread_task.rs:25](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/mobile/thread_task.rs:25)。

手机服务按 app DB 进程持有，关窗后可保留 legacy session 与桌面 host；主动退出分别关闭两套 sessions，项目停止执行 `kill_all`。[controller/sources.rs:3](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/mobile/controller/sources.rs:3)、[lib.rs:141](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/lib.rs:141)、[desktop/sessions.rs:44](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/workspace/desktop/sessions.rs:44)、[lifecycle.rs:4](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/app/lifecycle.rs:4)。这些生命周期设计在源码上仍兼容，不应声称关闭窗口必然断开手机。

**触发例：**删除桌面甲/Repo 区域只停止并删除该 area session/canvas，不删除旧 `WorkspaceIdentity`，也不会停止同根的手机 legacy session。[management.rs:174](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/workspace/desktop/management.rs:174)、[desktop_workspace_store.rs:250](/Users/alan/Documents/mybook/norma-ai-os/crates/storage/src/desktop_workspace_store.rs:250)。所以手机中 Repo 仍可列出、仍可能派工。

**影响与待确认：**“从桌面空间移除项目”和“撤销手机对项目的控制”本可不同，不能仅按对象命名判为权限漏洞；产品需要明确移除的是组织关系、执行实例还是设备授权。关窗继续派工、主动退出取消迟到加载、删除目标后 terminal/follow-up 的行为，仍须用受影响集成测试和真实 App 验证；本调查未宣称这些运行验收通过。

## 手机 Norma 与设置

### N1｜确定：手机群聊尚未迁入已确认的 Mac 主控架构

10 月 3 日的总体方案已确认以 Mac 为主体：核心运行、人格、分域记忆与能力管理在 Mac，手机连接同一个 Norma，云端仍可提供模型推理。该文档同时明确旧手机云端会话/实时语音尚未迁移：[总体方案:17](/Users/alan/Documents/mybook/norma-ai-os/docs/design-docs/2026-10-02-norma-master-memory-and-growth.md:17)、[迁移边界:59](/Users/alan/Documents/mybook/norma-ai-os/docs/design-docs/2026-10-02-norma-master-memory-and-growth.md:59)。

实际调用链：

```text
桌面输入 → Mac NormaSession → 人格 / 分域记忆 / 工具 → 配置的模型

手机 @Norma → CloudChatModel → Node /v1/threads/plan
                             → 固定豆包模型、固定提示词
                             → 回复与任务候选 → 手机保存 → Mac 执行
```

手机入口经 [WorkspaceLibraryView.swift:147](/Users/alan/Documents/mybook/norma-ai-os/ios/App/WorkspaceLibraryView.swift:147) 调用 [CloudChatModel.swift:115](/Users/alan/Documents/mybook/norma-ai-os/ios/App/CloudChatModel.swift:115)。服务端在 [mobile-text.mjs:112](/Users/alan/Documents/mybook/norma-ai-os/services/norma-cloud/mobile-text.mjs:112) 固定调用 `doubao-seed-2.0-mini`，只接收本轮消息、手机生成的线程上下文和成员清单；没有接入桌面 Persona/MemoryAdapter，也没有读取桌面模型配置。

桌面则通过 [values.rs:402](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/settings/voice/state/values.rs:402) 读取所选 Provider/model，通过 [memory.rs:42](/Users/alan/Documents/mybook/norma-ai-os/crates/app/src/app/memory.rs:42) 读取人格与记忆，再由 [memory_session.rs:66](/Users/alan/Documents/mybook/norma-ai-os/crates/agent/src/master/memory_session.rs:66) 注入每轮上下文。这些是已经存在的桌面代码，但不代表跨端会话管理已经实现。

**例子：**用户在桌面设置 Norma 为“简洁风格”，保存项目约定并改用某个模型；手机工作线程中的 Norma 不读取这些设置。给手机独立服务补 Key 能让旧请求成功，不能让手机与桌面成为同一主智能体。

旧语音设置另有独立 Provider、网关与 Mac 签名授权，并仍显示“电脑暂时离线时仍可聊天”：[CallSettingsView.swift:23](/Users/alan/Documents/mybook/norma-ai-os/ios/App/CommandCenter/CallSettingsView.swift:23)、[VoiceCallAuthorization.swift:11](/Users/alan/Documents/mybook/norma-ai-os/ios/App/CommandCenter/VoiceCallAuthorization.swift:11)。这也是迁移对象；按既有用户安排，语音实现继续放在群聊闭环之后。

### S1｜确定：项目与成员设置保存本地引用快照，没有表达桌面执行实例

手机项目引用只有 `machineID/projectID/name/machineName`，Agent 引用只有 `machineID/projectID/definitionID/name/profile`：[CollaborationWorkspaceStore.swift:4](/Users/alan/Documents/mybook/norma-ai-os/ios/Sources/NormaCore/CollaborationWorkspaceStore.swift:4)。它不保存桌面空间、区域、实际执行目录或具体运行会话。

设置页明确“关联保存在这部 iPhone”：[WorkspaceSettingsView.swift:65](/Users/alan/Documents/mybook/norma-ai-os/ios/App/WorkspaceSettingsView.swift:65)。工作空间刷新从本地 store 读取：[WorkspaceLibraryView.swift:40](/Users/alan/Documents/mybook/norma-ai-os/ios/App/WorkspaceLibraryView.swift:40)。成员候选只在选择器加载时取 snapshot，并将名称/profile 复制进本地引用：[WorkspaceSettingsView.swift:211](/Users/alan/Documents/mybook/norma-ai-os/ios/App/WorkspaceSettingsView.swift:211)。派工时会校验当前 grant、route 和持久回执，因此不能据此推断已有越权；缺的是设置页对失效关联、桌面最新名称/状态和执行实例的及时说明。

**建议区分：**项目表示目录/仓库身份；Agent 定义表示可创建和呼叫的角色；Agent 实例表示某个区域内已经运行的卡片与会话。群聊成员需要明确执行绑定。添加角色用于新任务、连接现有实例用于续聊，两者不能只用一个 `definitionID` 隐式代表。

### S2｜确定：设置允许多 Mac，群聊发送强制单 Mac

项目选择器按全部 `computers.machines` 展示项目，允许把不同电脑的项目加入同一个空间：[WorkspaceSettingsView.swift:164](/Users/alan/Documents/mybook/norma-ai-os/ios/App/WorkspaceSettingsView.swift:164)。store 校验检查引用有效性与唯一性，没有单 Mac 限制：[CollaborationWorkspaceStore.swift:97](/Users/alan/Documents/mybook/norma-ai-os/ios/Sources/NormaCore/CollaborationWorkspaceStore.swift:97)。

发送前则要求关联项目的 Mac 集合恰好只有一个：[ThreadOutboxStore.swift:37](/Users/alan/Documents/mybook/norma-ai-os/ios/Sources/NormaCore/ThreadOutboxStore.swift:37)。所以配置可保存、普通消息和具名派工却无法送达。这是可以直接从现有路径确认的设置/执行合同冲突。

建议首期仍按已确定的单 Mac 范围执行，在添加项目时就约束或解释不支持的组合，不为消除此问题擅自扩展多 Mac 执行。

### S3｜确定：配置目录与发送消息使用不同连接路径

添加项目/Agent 通过 `MobileClient.snapshot` → `PinnedConnection.open` 直连 Mac：[MobileClient.swift:47](/Users/alan/Documents/mybook/norma-ai-os/ios/Sources/NormaCore/MobileClient.swift:47)。线程同步则先加载云端设备准入，再强制使用协作中继：[WorkspaceThreadDelivery.swift:28](/Users/alan/Documents/mybook/norma-ai-os/ios/App/WorkspaceThreadDelivery.swift:28)、[同文件:87](/Users/alan/Documents/mybook/norma-ai-os/ios/App/WorkspaceThreadDelivery.swift:87)。

因此“扫描成功”“项目可选”“群聊可发送”不是同一个就绪条件。中继的设备准入可以保留，但应明确它是连接与转发能力，不能让用户把旧独立云端 Norma 授权理解为 Mac 主智能体的开关。统一目录、鉴权与消息的连接状态表达，并在规格里明确是否支持局域网直连；本调查不假设直连发送已经实现。

## 2026-10-05 已确认：移除独立工作线程模块

用户同意工作空间直接进入群聊，并明确可以去除工作线程。

- 目标入口为「工作空间列表 → 工作空间群聊」，移除工作线程列表、创建与命名步骤。
- 新群聊消息直接归属工作空间；任务、追问、回执与成果保留来源消息和具体执行对象的关联，不把整段群聊当成一个执行任务。
- 想法库继续独立；将想法带入工作时引用到选定工作空间的群聊，不创建工作线程。
- 已有线程对话与任务历史必须保留原始归属与来源；迁移展示与数据映射需在实现规格中定义，不能直接删除或无来源地拼接成一条时间线。
- 不新增替代线程的独立话题模块；两端是否共用同一套工作空间及项目/Agent 配置、历史权威与冲突策略仍待确认。

这次确认是产品结构决定，不表示手机页面、数据库、同步协议或桌面会话映射已完成改造。

## 建议的对齐方式（其余关系待确认）

1. **Norma 由 Mac 统一承载。** 手机发消息给同一 Mac 主智能体，继承相同人格、记忆范围和能力策略；云端负责模型与必要中继。手机保留本地消息、草稿和想法，历史正文权威及跨端冲突策略另行确认，不自动搬运旧私聊。
2. **保留两类 Workspace 的不同身份，补执行绑定。** 手机 Workspace 继续组织讨论；其项目关联明确指向 Mac 上某个桌面空间的项目区域。不得把手机 UUID、仓库身份和桌面区域 ID 混成一个字段，也不得悄悄选择桌面当前区域。
3. **把角色定义和正在运行的 Agent 分开。** 复用新桌面的项目定义库；绑定现有 Agent 时记录区域与实例身份，创建新任务时在明确区域内创建/复用实例。界面显示所属空间、项目和真实执行目录，避免同名 Repo/Alice 被误认。
4. **版本化扩展协议，贯通所有读写路径。** 在 catalog、权限、执行、任务账本、成果、追问、终端与回执中共用同一个目标定位；保留旧协议语义和已冻结任务，不把旧任务自动重定向到新区域。对失效目标显示明确状态。
5. **设置只允许当前可执行的组合。** 首期一台 Mac、多个项目；项目及 Agent 状态由 Mac 核验。已有一次性手机授权继续生效，不为每个聊天成员另造一套用户授权流程。

目标关系示意，不是已经实现的新协议：

```text
手机工作空间 → 群聊 / 本地记录（消息直接归属工作空间）
      │
      ├── Norma 消息 → 已配对 Mac 上的同一 Norma
      │                └── 人格 / 分域记忆 / 模型 / 执行策略
      │
      └── 执行绑定 → Mac → 桌面工作空间 → 项目区域 → Agent 实例
                                               └── 实际目录 / 任务账本 / 成果
```

## 修复前必须锁定的验收场景

| 场景 | 必须观察到 |
| --- | --- |
| 桌面甲/Repo 新建 Alice，手机添加成员并派工 | 手机看到正确定义/实例；同一桌面卡片执行，回执来自同一任务账本 |
| 甲、乙都包含同一个 Repo 和同名 Alice | 手机可明确选定目标；桌面切换不改变已发送请求的目标，也不向另一实例输入 |
| 桌面修改人格、模型及个人/项目记忆 | 手机 Norma 回合使用相同设置；个人、项目与工作树范围按既有策略隔离 |
| 移除区域、停用成员、撤销手机、Mac 重启 | 手机及时显示目标状态；原任务可核对，不自动换目标或重复派工 |
| 设置尝试加入第二台 Mac 或不可执行成员 | 保存前明确反馈；不会创建界面显示正常却永久无法发送的空间 |

当前只有源码证据，未运行这些场景。后续实现需要聚焦失败测试、受影响集成测试，以及真实 Mac/iPhone 流程验收；当前文档不能作为群聊已可用的验收记录。
