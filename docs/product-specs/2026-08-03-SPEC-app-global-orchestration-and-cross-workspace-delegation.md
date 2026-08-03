# App 全局编排入口与跨 Workspace Agent 任务委派 SPEC

- 日期：2026-08-03
- 状态：阶段一已实施，待真实 `.app` L3 与 Session 级 Hook 隔离收口
- 文档性质：产品与技术规格、实施边界与自动化证据记录
- 目标版本：下一版 Norma OS 编排能力
- 适用范围：Norma 本地服务、Workspace 路由、Agent Hook、编排安装状态、任务身份、CLI、Skill、持久化与自动化验收
- 扩展规格：[`2026-07-25-SPEC-agent-orchestration-installation-and-task-delegation.md`](2026-07-25-SPEC-agent-orchestration-installation-and-task-delegation.md)

## 0. 实施记录

截至 2026-08-03，阶段一已完成：

- Norma Unix listener、Agent Hook ingress 与编排 health 已提升到 App 全局生命周期。
- Workspace 获得稳定 ID 与运行期 route ID，Agent 编排请求不再依赖活动窗口或跨窗口 node ID 扫描。
- CLI/协议升级到 v3，支持 `doctor`、`workspace list`、显式目标 Workspace、operation ID、结构化版本错误、`outcome_unknown` 与按任务读取最终结果。
- 全局任务账本支持稳定 task ref、assign/cancel 幂等、来源授权、跨 Workspace lineage、循环门禁、后代预算与重启中断对账。
- 受管 Skill 已记录全局服务、显式跨 Workspace 路由、JSON、幂等重放和 untracked 安全规则。
- 双 Workspace E2E 已覆盖后台目标、相同 node ID 碰撞、精确目标 cwd、不抢焦点，以及终态 `status`/`result` 返回同一绑定回复。

仍需在后续收口：

- Hook token 当前按 Workspace route 隔离；还需下沉为 Agent Session generation 级 token，才能完全满足 22.5 的迟到旧 Session 隔离。
- 真实 `.app` L3 尚需覆盖 1/2/10 窗口的唯一 listener、第二进程竞争、退出 join 与 owned socket 清理。
- 跨 Workspace `status`/`result` 已有双 Workspace 端到端闭环；`wait` 复用同一 status 轮询，`cancel` 与目标关闭后的更多组合仍可继续扩充验收。
- `cargo test --workspace` 当前被未改动的 `ego_preview` 既有测试编译错误阻断；本次涉及的 `norma`、`agent`、`storage`、目标 E2E 与 `cargo check -p app` 已通过。

## 1. 背景与问题

当前 Norma OS 把 `NormaServer` 放在 `MultisoulApp` 中，因此每创建一个窗口都会：

1. 绑定同一个 Unix socket 路径。
2. 启动一个独立的 `norma-server` listener 线程。
3. 启动一个窗口级请求 drain loop。

新窗口绑定前会删除旧 socket 路径。旧 listener 仍持有已经被 unlink 的 Unix socket inode，窗口关闭顺序又可能删除当前有效路径，最终形成以下半失效状态：

```text
norma-os 进程仍然存在
├── 多个旧 norma-server listener 线程仍在 accept
├── 多个旧 Agent Hook listener 线程仍在 accept
├── 文件系统中的 socket 不存在或没有可达 listener
└── norma agent list 返回 app_unavailable
```

现有编排协议同时缺少 Workspace 路由身份。请求只携带 Provider 授权和 `source_node_id`，无法表达：

- 来源 Agent 属于哪个 Workspace。
- 用户希望查询哪个 Workspace 的 Agent。
- 任务应当被派发到哪个 Workspace。
- `status`、`wait`、`result` 和 `cancel` 应当查询哪个 Workspace 的任务协调器。

不同 Workspace 的 node ID 和 Agent 名称都可能重复。因此，依赖“当前活动窗口”“最新绑定 socket 的窗口”或跨窗口扫描 node ID，都无法提供确定、安全的跨 Workspace 编排。

此外，当前实现还存在以下可靠性问题：

- `AgentHookServer` 同样按窗口启动且没有 shutdown/join，关闭窗口后可能继续监听并丢弃事件。
- Provider 编排安装状态按窗口异步加载，窗口刚启动时会先把所有 Agent 投影为 `untracked`。
- `wait` 先检查 `tracking=untracked`，导致已经进入 `done/failed/interrupted` 的任务仍可能被拒绝。
- 任务执行状态主要保存在窗口内存 coordinator 中，窗口关闭、错误路由或 App 重启后无法继续查询。
- `assign` 超时后没有幂等凭据，客户端无法判断任务是否已经产生副作用。
- Skill、CLI 与运行中 App 缺少明确的协议版本握手和诊断命令。

## 2. 与既有规格的关系

本 SPEC 是现有 Agent 编排规格的增量扩展。除本节明确列出的覆盖项外，既有规格中的安装、Provider 授权、Agent 队列、用户明确授权、禁止自动 `--force-untracked`、任务树预算和视觉规范继续有效。

本 SPEC 覆盖既有规格中的以下约束：

1. “跨项目 Agent 寻址或任务派发”为 Out of Scope。
2. 旧 `task_id` 在 App 重启后只能返回 `runtime_expired`。
3. 本地 Norma 服务由当前窗口承担。
4. Provider 编排组件状态由每个窗口独立维护。
5. `tracking=untracked` 时无条件拒绝 `wait`，即使任务已经有明确终态。

新的产品语义是：

- 同一 App 进程只有一个 Norma CLI 入口和一个 Agent Hook 入口。
- Workspace 通过显式身份注册到 App 全局路由表。
- 用户可以明确指定一个已经打开的目标 Workspace，并向其中的 Agent 派发任务。
- 执行队列仍不跨 App 重启恢复，但任务状态与路由记录可持久查询。

## 3. 第一性原则与核心判断

### 3.1 生命周期归属

Unix socket、CLI 请求入口、Provider 安装健康状态和全局任务路由属于 App 进程，而不是任何一个窗口。

窗口只拥有：

- Workspace Runtime。
- Canvas 和 Agent 卡片。
- 该 Workspace 的本地任务调度执行面。
- 向 App 全局路由注册和注销的能力。

因此窗口不得创建、替换或删除 App 全局 socket。

### 3.2 身份不能依赖显示名称或 node ID

- Workspace 显示名称可以重复。
- canonical path 可能移动，且不应直接作为授权令牌。
- node ID 只在单个 Canvas document 内唯一。
- Agent Definition ID 只在单个 Workspace 内唯一。

跨 Workspace 身份必须使用组合主体：

```text
AgentPrincipal
├── workspace_id
├── definition_id
└── provider
```

运行期窗口路由还必须额外包含 `workspace_route_id`，防止关闭后的旧句柄继续接收请求。

### 3.3 明确路由优先于活动窗口猜测

Agent 发起的编排请求必须包含来源 Workspace。跨 Workspace 请求必须显式包含目标 Workspace。

- 不允许用当前活动窗口推断 Agent 请求的来源。
- 不允许扫描所有窗口寻找相同 node ID。
- 不允许在目标 Workspace 未打开时擅自打开项目。
- 不允许因为路由失败而降级到另一个 Workspace。

### 3.4 执行状态与持久状态分离

目标 Workspace 的内存 coordinator 负责实际排队和提交；App 全局任务注册表负责：

- 全局 `task_ref`。
- 来源与目标 Workspace 路由。
- 父子任务关系和跨 Workspace 循环检测。
- 当前状态摘要。
- App 重启后的终态查询与未完成任务中断对账。

App 重启不恢复正在运行的 Provider 任务，但不得让已经持久化的任务身份消失或错误指向新任务。

### 3.5 终态证据优先于派发时的 tracking 快照

`tracking` 描述派发和运行阶段是否具备可信 Hook，不应覆盖后来已经取得的明确终态。

```text
Done / Failed / Cancelled / Interrupted
    → 直接作为终态返回

仍在运行 + tracking=managed
    → 可以继续 wait

仍在运行 + tracking=untracked
    → 返回 task_untracked
```

## 4. 目标与成功定义

### 4.1 产品目标

1. App 进程内始终只有一个可达的 Norma CLI socket。
2. 打开、关闭或切换任意窗口都不会破坏 CLI 可用性。
3. 用户可以明确要求 Workspace A 的 Agent 把任务派给 Workspace B 的 Agent。
4. 请求、Hook 和任务状态始终路由到正确 Workspace。
5. `status`、`wait`、`result` 和 `cancel` 不依赖当前活动窗口。
6. 关闭来源窗口不应中断已经进入目标 Workspace 的任务。
7. 关闭目标窗口或退出 App 时，未完成任务进入真实的 `interrupted` 状态。
8. Provider、CLI、Skill 和 Hook 的版本与健康状态由 App 全局维护。
9. 任何副作用未知状态都不得被自动重试为一个新任务。

### 4.2 成功标志

功能成功时必须同时满足：

- 打开 10 个 Workspace 窗口后仍只有一个 `norma-server` listener 和一个 App 全局 Hook listener。
- 以任意顺序关闭窗口，只要 App 仍运行，CLI socket 就保持可连接。
- Workspace A 和 B 存在相同 node ID、Agent 名称和 Provider 时，请求仍能准确命中指定目标。
- 从 A 指派到 B 后，任务在 B 的 cwd 和 B 的 Agent 队列中执行。
- B 不是活动窗口时仍能接收任务，且系统不自动切换焦点。
- 关闭 A 后，B 中已经提交的任务继续执行。
- 关闭 B 后，B 中未完成任务被标记为 `interrupted`，A 的查询可以得到该终态。
- App 重启后，旧任务引用可查询持久终态或对账后的 `interrupted`，不会命中新运行实例任务。
- 已经具有 `done/failed/cancelled/interrupted` 状态的 untracked 任务能正常结束 `wait`。
- Skill、CLI 和 App 协议不兼容时，在副作用发生前返回结构化错误。

## 5. 范围

### 5.1 In Scope

- App 全局单例 `NormaEndpointHost`。
- App 全局 Workspace 路由注册表。
- App 全局 Agent Hook ingress，或具备等价单例路由能力的 Hook 模块。
- App 全局 Provider 编排健康状态。
- 持久化 `WorkspaceId` 和运行期 `WorkspaceRouteId`。
- 当前 Workspace 和跨 Workspace Agent 查询。
- 向已打开目标 Workspace 的 Agent 派发一个任务。
- 全局任务引用、任务路由、父子关系与状态摘要。
- 同 Workspace 与跨 Workspace 的 `status`、`wait`、`result` 和 `cancel`。
- 任务终态优先的 wait 判定。
- 请求幂等凭据和副作用未知语义。
- App/CLI/Skill/Hook 协议版本握手和诊断。
- socket、listener、client handler 的 shutdown/join。
- 结构化日志、健康诊断和 L3 真实 App 验收。
- 更新 `norma-orchestration` Skill 的跨 Workspace 与错误处理规则。

### 5.2 Out of Scope

- 自动打开尚未打开的目标 Workspace。
- 远程机器、局域网或云端跨设备编排。
- Agent 自主选择目标 Workspace 或目标 Agent。
- 未经用户明确要求的跨 Workspace 委派。
- 自动复制源 Workspace 的文件、Git 状态、对话或记忆到目标 Workspace。
- 让目标 Agent 默认读取来源 Workspace 文件。
- 跨 Workspace 共享终端、PTY、Canvas 节点或 Provider Session。
- App 重启后恢复正在运行的任务队列或 Provider 进程。
- 一次 CLI 调用向多个 Workspace 或多个 Agent 批量派发。
- 返回终端全文、文件内容或工作产物；可信 Hook 提供的有界 Agent 最终回复属于任务结果协议。
- 自动修复用户不归 Norma 管理的 Hook、Skill 或配置冲突。
- 在本 SPEC 中重新设计已有 Orchestration Settings 页面。

## 6. 用户与使用场景

### 6.1 用户角色

- Norma OS 用户：明确指定来源、目标 Workspace 和目标 Agent。
- 来源 Agent：在用户明确指示下调用 `norma-orchestration` Skill。
- 目标 Agent：在自己的 Workspace、cwd 和任务队列中执行任务。
- 维护者：通过全局任务路由、Hook 日志和版本诊断排查问题。

### 6.2 关键用户故事

1. 作为用户，我想让主项目中的 Codex 把官网检查任务派给官网 Workspace 中的 Codex，而不切换窗口。
2. 作为用户，我想先列出当前已打开 Workspace，再准确选择目标项目，而不是按窗口焦点猜测。
3. 作为用户，我想在两个 Workspace 都有名为 Alan 的 Agent 时，明确选择其中一个。
4. 作为用户，我想关闭来源窗口后让目标任务继续运行。
5. 作为用户，我想在目标窗口关闭后看到任务被中断，而不是一直等待。
6. 作为用户，我想在 Hook 暂时不可用时得到真实的 `untracked`，并且系统不擅自覆盖 Agent 输入。
7. 作为用户，我想在 App 重启后继续查询旧任务的最终审计状态。
8. 作为用户，我想让跨 Workspace 任务默认只接触目标 Workspace，避免隐式泄露源项目内容。

## 7. 领域模型

### 7.1 WorkspaceId

用户本机上的稳定 Workspace 身份，由 App 全局 SQLite 保存，并以 canonical root 作为本机映射键。

```text
WorkspaceIdentity
- workspace_id: UUID
- canonical_root: PathBuf
- display_name: String
- created_at_unix
- updated_at_unix
```

约束：

- 不把 Workspace 目录中的可复制文件作为唯一身份源。
- 同一 canonical root 在本机得到同一个 `workspace_id`。
- Workspace 移动后默认作为新的本机位置重新登记；历史任务保留旧路径快照。
- 路径只用于展示、诊断和打开状态匹配，不充当授权凭据。

### 7.2 WorkspaceRouteId

Workspace 窗口在当前 App 运行实例中的临时注册代次。

```text
WorkspaceRoute
- workspace_id
- workspace_route_id: UUID
- window_handle
- app_entity
- state: registered | closing
```

窗口关闭时必须先把 route 标记为 `closing`，再注销句柄。已解析到旧 route 的请求不得转移到新窗口或其他 Workspace。

### 7.3 AgentPrincipal

跨 Workspace 授权与循环检测使用稳定主体：

```text
AgentPrincipal
- workspace_id
- definition_id
- provider
```

`node_id` 是当前活跃卡片实例，只用于向 Runtime 提交输入，不作为跨 Workspace 稳定身份。

### 7.4 GlobalTaskRoute

```text
GlobalTaskRoute
- task_ref: opaque UUID-based reference
- operation_id: idempotency UUID
- source_workspace_id
- source_definition_id
- source_provider
- source_node_id_at_dispatch
- target_workspace_id
- target_definition_id
- target_node_id_at_dispatch
- target_runtime_instance_id
- target_runtime_task_id
- parent_task_ref NULL
- root_task_ref
- state
- tracking
- error_code NULL
- created_at_unix
- updated_at_unix
- terminal_at_unix NULL
```

全局记录不默认保存完整任务正文、终端输出或 Agent 回复。完整任务审计继续保存在目标 Workspace 的项目数据库中。

### 7.5 OrchestrationHealth

```text
OrchestrationHealth
- phase: checking | ready | degraded
- app_version
- protocol_version
- cli_version
- skill_version_by_provider
- hook_version_by_provider
- hook_installation_state_by_provider
- hook_ingress_state
- authorization_state_by_provider
- last_checked_at
- last_error NULL
```

`checking` 不得被投影为 `untracked`。在检查完成前，编排命令返回可重试的 `orchestration_checking`。

## 8. 总体架构

```text
┌──────────────────────────────────────────────────────────────┐
│                     Norma OS App 进程                        │
│                                                              │
│  ┌────────────────────┐      ┌────────────────────────────┐  │
│  │ NormaEndpointHost  │      │ AgentHookIngress           │  │
│  │ 唯一 Unix socket   │      │ 唯一 localhost listener    │  │
│  └─────────┬──────────┘      └─────────────┬──────────────┘  │
│            │                               │                 │
│            └──────────────┬────────────────┘                 │
│                           ▼                                  │
│              ┌───────────────────────────┐                   │
│              │ AppOrchestrationRuntime   │                   │
│              │ - WorkspaceRouter         │                   │
│              │ - HealthRegistry          │                   │
│              │ - GlobalTaskRegistry      │                   │
│              │ - IdempotencyRegistry     │                   │
│              └─────────────┬─────────────┘                   │
└────────────────────────────┼─────────────────────────────────┘
                             │
              ┌──────────────┴──────────────┐
              ▼                             ▼
   ┌──────────────────────┐      ┌──────────────────────┐
   │ Workspace A          │      │ Workspace B          │
   │ route_id=A-17        │      │ route_id=B-04        │
   │ MultisoulApp A       │      │ MultisoulApp B       │
   │ local coordinator A  │      │ local coordinator B  │
   └──────────────────────┘      └──────────────────────┘
```

### 8.1 NormaEndpointHost

这是隐藏 socket、listener、client handler、请求队列和退出清理的 App 全局深模块。

最小接口：

```text
install(app_context) -> Result<EndpointHandle>
health() -> EndpointHealth
shutdown() -> Result<ShutdownReport>
```

要求：

- 只能在 `Application::run` 初始化阶段安装一次。
- `start` 返回显式 `Result`，不得用 `Option` 静默降级。
- listener 线程具有 shutdown 信号和 `JoinHandle`。
- client handler 数量有上限，不能无界创建线程。
- 请求队列必须有界并提供明确的过载错误。
- 不依赖任何一个窗口 Entity 存活。
- App 退出时先停止接收，再完成/拒绝队列请求，最后删除自己拥有的 socket。

### 8.2 WorkspaceRouter

负责把稳定 `workspace_id` 解析为当前有效 `WorkspaceRoute`。

最小接口：

```text
register(workspace_id, window_handle, app_entity) -> RegistrationGuard
resolve(workspace_id) -> Result<WorkspaceTarget, RouteError>
mark_closing(workspace_route_id)
unregister(workspace_route_id)
list_open() -> Vec<WorkspaceSummary>
```

要求：

- 注册同一 Workspace 时必须复用/激活现有窗口或拒绝重复窗口。
- route 解析完成后冻结 `workspace_route_id`；执行前再次校验 route 未关闭。
- 不得在请求处理中按 node ID 扫描所有窗口。
- Agent 请求不使用活动窗口 fallback。
- 人工非编排命令是否使用活动窗口保持现有语义，不由本 SPEC 扩张。

### 8.3 AgentHookIngress

App 全局 Hook 入口负责验证 Hook token，并通过 token 中的服务端注册信息定位 Workspace、Agent 和 Session generation。

```text
HookSessionRegistration
- hook_token_hash
- workspace_id
- workspace_route_id
- node_id
- definition_id
- provider
- session_generation
- expires_at
```

要求：

- Agent 环境不需要信任其自行声明的目标 Workspace；服务端根据 token 注册信息解析路由。
- Workspace 或 Agent Session 关闭时撤销对应 Hook token。
- 已关闭 receiver 不得继续返回 HTTP 202。
- route 已过期时返回明确的 410/等价状态并记录诊断。
- Hook 转发失败继续采用不阻断 Provider 的 fail-open 原则，但必须写入独立、可轮转的本地诊断日志。
- App 日志记录收到、拒绝、路由失败和队列关闭事件，但不得记录 token 或完整任务正文。

### 8.4 OrchestrationHealthRegistry

Provider 安装扫描和自动 reconcile 是用户/App 级行为，只执行一次，不再由每个窗口并发执行。

窗口和 Settings 页面订阅同一个全局状态。

状态规则：

- `checking`：尚未完成检测，不允许把 Agent 标记为 untracked。
- `ready`：CLI、Skill、Hook、授权和 Hook ingress 均满足要求。
- `degraded`：至少一个组件缺失、冲突、版本不兼容或入口不可用。

### 8.5 GlobalTaskRegistry

负责全局 task identity、跨 Workspace lineage、状态摘要和重启对账。

本地 `AgentTaskCoordinator` 继续负责目标 Workspace 内的 FIFO 排队，不建立第二套执行队列。

全局注册表必须在副作用发生前创建 `accepted` 路由记录。跨 App DB 与项目 DB 的写入采用可对账的分步事务：

1. 在全局表创建 `accepted` 记录并占用 `operation_id`。
2. 在目标项目 DB 创建详细任务记录。
3. 提交到目标 coordinator。
4. 更新全局和项目状态。
5. 任一步失败都把全局记录更新为 `failed`，不得留下无身份副作用。

## 9. 生命周期

### 9.1 App 启动

```text
初始化日志和基础运行环境
  → 安装 NormaEndpointHost
  → 安装 AgentHookIngress
  → 加载 WorkspaceIdentityStore
  → 启动全局 OrchestrationHealth 检查
  → 加载并对账未终态 GlobalTaskRoute
  → 打开 License Gate / Launchpad / Workspace
```

要求：

- socket 和 Hook listener 在首个业务窗口之前建立。
- 文件扫描、哈希、SQLite 和 reconcile 在后台执行。
- 健康检查未完成时 CLI 可以连接，但编排命令返回 `orchestration_checking`。
- 启动发现上次运行遗留的非终态任务时，将其对账为 `interrupted`；不恢复队列。

### 9.2 窗口打开

1. 解析 canonical root。
2. 获取或创建本机 `WorkspaceId`。
3. 创建新的 `WorkspaceRouteId`。
4. 窗口完成 Workspace Runtime 初始化后注册 route。
5. route 注册成功前不得接受发往该 Workspace 的任务。

窗口不得启动新的 Norma socket、全局 Hook listener 或 Provider 安装 reconcile。

### 9.3 窗口关闭

1. route 进入 `closing`。
2. 拒绝尚未开始执行的新请求。
3. 目标 Workspace 中未完成任务按现有 Runtime 关闭规则中断。
4. 同步全局与项目任务状态为 `interrupted`。
5. 撤销该窗口下所有 Hook Session token。
6. 注销 route。
7. kill/wait 该窗口的 PTY 子进程。

关闭来源窗口不影响已经进入其他目标 Workspace 的任务。

### 9.4 App 退出

1. 全局运行状态进入 `shutting_down`。
2. 停止接收新的 assign/present/card 副作用请求。
3. 对已接收但尚未提交的请求返回确定性取消结果。
4. 关闭所有 Workspace Runtime 并对账任务终态。
5. shutdown/join AgentHookIngress。
6. shutdown/join NormaEndpointHost。
7. 仅删除由当前 `SocketLease` 拥有的 socket inode。
8. 刷新日志并退出进程。

## 10. 业务流程

### 10.1 同 Workspace 指派

未指定 `--workspace` 时，目标默认为来源 Agent 所在 Workspace。

```text
来源 Agent 调用 agent list
  → 服务端从来源身份确定 Workspace A
  → 只返回 A 的 Agent
  → task assign 目标 Agent
  → 在 A 中完成身份校验、排队和提交
```

不得使用活动窗口覆盖来源 Workspace。

### 10.2 跨 Workspace 指派

```text
用户明确说“把任务交给 Workspace B 的 Agent X”
  → 来源 Agent 调用 workspace list
  → 精确解析 B 的 workspace_id
  → 调用 agent list --workspace B
  → 精确解析 B 中的 Agent X
  → 调用 task assign --workspace B AgentX --task ...
  → App 在 Workspace A 校验来源 Agent
  → 全局 lineage 校验自派发、循环和预算
  → App 在 Workspace B 校验目标 Agent
  → 创建全局 task_ref 与持久路由
  → 必要时在 B 自动激活目标 Agent 卡片
  → 进入 B 的现有 coordinator
  → 返回 task_ref、目标 Workspace 和真实 tracking/status
```

约束：

- 目标 Workspace 必须已经打开并完成 route 注册。
- 不自动激活或切换目标窗口焦点。
- 目标 Agent 使用 B 的 cwd、文件和 Session。
- 不自动把 A 的对话尾部、文件路径或项目上下文注入 B。
- 同名 Workspace 必须通过 `workspace_id` 消歧。
- 同名 Agent 必须在目标 Workspace 名册中精确匹配。

### 10.3 状态查询、等待和结果读取

`status`、`wait` 和 `result` 先查询全局任务注册表，再在目标 route 仍存活时向目标 coordinator 获取更实时状态。终态 `status`/`wait` 直接携带结果；`result` 是按全局 `task_ref` 的单次查询。非终态 `result` 返回可重试的 `task_result_not_ready`，不得改查最新对话。

```text
读取 GlobalTaskRoute
  → 校验调用方来源主体有权查询
  → 如果已有终态：直接返回
  → 如果目标 route 存活：读取目标 coordinator
  → 如果目标 route 已关闭：对账为 interrupted
  → 如果仍运行且 managed：继续 wait
  → 如果仍运行且 untracked：返回 task_untracked
```

终态判定必须先于 tracking 判定。

### 10.4 取消

- `accepted/queued`：从目标 coordinator 移除，进入 `cancelled`。
- `submitted`：向目标 Runtime 发送中断，进入 `cancel_requested`。
- 收到中断 Hook 或 Runtime 退出证据后进入 `interrupted`。
- 目标 Workspace 已关闭：返回已经对账的 `interrupted`。
- 已终态：返回确定性 `task_not_cancellable`，不重复发送中断。
- 不级联取消子任务，沿用既有规格。

### 10.5 来源窗口关闭

- 已提交到目标 Workspace 的任务继续执行。
- 来源 Agent 的当前 CLI wait 进程随来源 PTY 关闭而结束。
- 同一来源 `AgentPrincipal` 在重新激活后可以用原 `task_ref` 查询。
- 其他 AgentPrincipal 不能仅凭 task_ref 查询任务。

### 10.6 目标窗口关闭

- 未提交任务取消。
- 已运行任务被中断。
- Hook token 失效。
- 全局任务记录进入 `cancelled` 或 `interrupted`。
- 来源方下次查询立即得到终态，不等待到 timeout。

## 11. CLI 与协议

### 11.1 CLI 命令

新增：

```text
norma --version
norma doctor [--format json|text]
norma workspace list [--format json|text]
norma agent list [--workspace <workspace-id>] [--format json|text]
norma agent task assign <agent> \
  [--workspace <workspace-id>] \
  --task <text> \
  [--operation-id <uuid>] \
  [--force-untracked]
```

保持：

```text
norma agent task status <task-ref>
norma agent task wait <task-ref> [--timeout <seconds>]
norma agent task result <task-ref>
norma agent task cancel <task-ref>
```

### 11.2 workspace list 输出

```text
WorkspaceSummary
- workspace_id
- display_name
- canonical_root
- active
- route_state
- agent_count
```

只返回当前 App 已打开并注册的 Workspace。该命令属于编排命令，必须验证来源 Agent 和 Provider 授权。

### 11.3 WireRequest 扩展

```text
WireRequest
- protocol_version
- client_version
- request_id
- operation_id NULL
- authorization
- source_workspace_id
- source_workspace_route_id
- source_node_id
- target_workspace_id NULL
- command
```

服务端必须以自己的 WorkspaceRouter 和来源 token/授权重新验证字段，不信任客户端自行声明的 node、route 或 target。

### 11.4 task assign 输出

```text
AgentTaskReceipt
- task_id: opaque global task_ref（保留现有 JSON 字段名）
- operation_id
- source_workspace_id
- target_workspace_id
- target_definition_id
- target_name
- target_node_id
- status
- tracking
- parent_task_id: parent global task_ref
- root_task_id: root global task_ref
```

成功只表示任务被真实接收、排队或提交，不表示完成。

### 11.5 幂等与副作用未知

- 每个副作用命令必须带 `operation_id`。
- 同一 `operation_id` 和相同规范化参数重复提交，返回第一次结果。
- 同一 `operation_id` 配合不同参数，返回 `idempotency_conflict`。
- CLI 在一次调用中生成并保留 operation ID。
- 如果连接在获得响应前断开，错误输出必须包含 operation ID 和 `outcome_unknown=true`。
- Skill 不得生成一个新 operation ID 自动重试；只能复用原 ID查询或重放。

### 11.6 版本握手

- CLI、Skill 和 Hook 都具有可读取的实际版本。
- WireRequest 携带协议版本和客户端版本。
- Server 返回 server version 和 protocol version。
- 主版本不兼容时，在任何副作用前返回 `protocol_version_mismatch`。
- Skill 的 `managed-version` 不能作为 CLI 实际版本的替代证据。
- `norma doctor` 必须显示 App、CLI、Skill、Hook、socket、Hook ingress 和 Provider health，且不暴露 token。

### 11.7 向后兼容

- 非编排 card 命令保持现有协议兼容策略。
- 旧编排 CLI 缺少 source Workspace 身份时，不在多 Workspace 环境中猜测路由。
- 恰好只有一个 Workspace 也不得为 Agent 编排静默伪造来源；返回 `client_upgrade_required`，提示重启 Agent Session。
- 升级后已运行的旧 Agent Session 不热注入新环境变量，必须重新激活或重启。
- 旧格式 task ref 继续按旧项目记录查询；无法确定 Workspace 时返回明确的 `legacy_task_route_unavailable`。

## 12. Skill 行为规范

更新后的 `norma-orchestration` Skill 必须：

1. 仅在用户明确要求委派给明确 Agent 时执行。
2. 仅在 Norma 创建、具有来源 Workspace 身份的 AgentCard Session 中使用。
3. 使用 `--format json`，不解析自然语言输出。
4. 跨 Workspace 前先调用 `workspace list`。
5. 通过精确 `workspace_id` 调用目标 Workspace 的 `agent list`。
6. 一次只向一个 Agent 派发一个任务。
7. 安全传递 Agent 名称和任务正文，不把用户文本直接拼接为未转义 shell 代码。
8. receipt 为 managed 时才自动进入 wait。
9. receipt 为 untracked 时停止自动等待，说明无法可靠跟踪。
10. 已经具有明确终态的 untracked 任务可以正常报告终态。
11. 不自动使用 `--force-untracked`。
12. `outcome_unknown` 时停止工具链，不使用新 operation ID 重试。
13. `orchestration_checking` 可以在有界退避后只读重试。
14. `workspace_not_open`、`ambiguous_workspace` 和版本不兼容时停止，不自动打开窗口或修改安装。

## 13. Tracking 与 Hook 语义

### 13.1 tracking 来源

Agent 的 tracking 由 App 全局 Provider health、Hook ingress 可用性和当前 Agent Session Hook 注册共同决定，不再读取窗口私有的默认 Missing 状态。

建议状态：

```text
checking   全局健康检查尚未完成
managed    Hook 安装和当前 ingress 路由有效
untracked  Hook 缺失、冲突、版本不兼容或当前 Session 无有效注册
```

如果为了保持协议兼容不新增 `checking` 到 receipt，则在 checking 阶段直接拒绝派发并返回 `orchestration_checking`。

### 13.2 wait 判定

```text
if status is terminal:
    return complete
else if tracking is untracked:
    return task_untracked
else:
    continue waiting
```

终态包括：

- `done`
- `failed`
- `cancelled`
- `interrupted`

### 13.3 Hook 事件路由

Hook token 服务端注册信息必须同时匹配：

- WorkspaceId
- WorkspaceRouteId
- node ID
- Agent Definition ID
- Provider
- Session generation

任一不匹配都不能把事件投递给其他 Workspace 或新 Session。

## 14. Socket 与进程安全

### 14.1 SocketLease

socket 绑定必须通过进程级 lease 串行化：

1. 获取 lock/lease。
2. 检查路径类型。
3. 如果现有 socket 可完成健康握手，视为活跃实例，绝不删除。
4. 如果连接失败，确认它是 stale Unix socket 且 lease 仍属于当前启动流程后再删除。
5. 如果路径是普通文件或符号链接，拒绝覆盖。
6. bind 后记录 device/inode。
7. shutdown 时只删除仍由当前 lease 拥有的 inode。

第二个 App 进程不得 unlink 第一个活跃 App 的 socket。产品层可以选择激活现有 App 或以无 CLI 的降级模式启动，但必须明确记录，不能静默抢占。

### 14.2 Listener 退出

- Norma socket listener 和 Hook listener 都必须有 shutdown 信号。
- shutdown 必须能唤醒阻塞中的 accept。
- App teardown 必须 join listener。
- client handler 必须设置读写超时和并发上限。
- 关闭 receiver 后不得继续返回成功响应。
- 监听线程数量必须通过 L3 测试和诊断命令可观测。

## 15. 状态、错误与边界情况

### 15.1 稳定错误码

保留既有错误码，并新增：

```text
orchestration_checking
client_upgrade_required
protocol_version_mismatch
workspace_required
workspace_not_found
workspace_not_open
workspace_route_expired
ambiguous_workspace
workspace_identity_conflict
cross_workspace_forbidden
task_not_cancellable
idempotency_conflict
outcome_unknown
server_overloaded
app_shutting_down
legacy_task_route_unavailable
```

### 15.2 关键失败行为

| 场景 | 行为 |
|---|---|
| App 进程不存在 | `app_unavailable`，不产生副作用 |
| socket 文件存在但 listener 不可达 | `app_unavailable`，doctor 报 stale socket |
| 全局安装状态检查中 | `orchestration_checking`，可重试 |
| 来源 Workspace route 已关闭 | `workspace_route_expired`，不 fallback |
| 目标 Workspace 未打开 | `workspace_not_open`，不自动打开 |
| Workspace 名称重复 | `ambiguous_workspace`，要求使用 ID |
| 来源 node ID 在其他 Workspace 碰撞 | 仍按来源 Workspace 校验，不跨窗口扫描 |
| 目标 Agent 未知 | `unknown_agent`，不创建未知 Agent |
| 目标 Agent 已禁用 | `agent_disabled`，不自动启用 |
| 目标 Agent untracked 且空闲 | 允许首次提交，返回 untracked |
| 目标 Agent untracked 且已有未确认任务 | `untracked_requires_force` |
| 已终态但 tracking=untracked | 返回真实终态 |
| assign 响应丢失 | 返回 operation ID 和 outcome unknown，不盲重试 |
| 来源窗口关闭 | 目标任务继续，来源 wait 结束 |
| 目标窗口关闭 | 未完成任务取消或中断 |
| App 重启 | 非终态对账为 interrupted，旧 task ref 可查询 |

## 16. 安全与权限

### 16.1 来源认证

服务端必须同时验证：

- Provider authorization token 有效。
- 来源 Workspace route 有效。
- 来源 AgentPrincipal 存在并启用。
- 当前 node ID 对应同一 definition/provider。
- 请求来自该 Agent Session 注入的上下文。

Provider token 仍然按 Provider 隔离，不能因为跨 Workspace 而跨 Provider 使用。

### 16.2 跨 Workspace 授权

- 只有用户明确说出目标 Workspace 与目标 Agent 时，Skill 才能执行跨 Workspace assign。
- Agent 不得为了并行化或自行拆分任务选择其他 Workspace。
- Server 记录 source/target principal 和 operation ID，以便审计。
- 首版不增加每个 Workspace 的 ACL；安全边界仍是同一 macOS 用户和同一 Norma OS App。
- 未来如支持多用户或远程执行，必须重新设计授权，不能复用本地信任模型。

### 16.3 数据隔离

- 目标 Agent 的 cwd 固定为目标 Workspace。
- 不自动注入来源 Workspace 对话、Canvas、文件列表或绝对路径。
- 用户明确要求目标读取来源文件时，任务正文必须包含明确路径和授权意图。
- 默认日志不记录完整任务正文、Provider token、Hook token、Authorization header 或 Agent 回复。

## 17. UI/UX 需求

本功能以后台路由和 CLI 为主，不要求新增跨 Workspace 管理页面。

用户可观察反馈：

- 来源窗口的派发回执显示“目标 Agent · 目标 Workspace”。
- 目标窗口使用既有任务接收反馈和粒子效果。
- 后台目标窗口不自动抢焦点或切换为活动窗口。
- `workspace_not_open` 的文案明确告诉用户先打开目标项目。
- `untracked` 明确区分“任务已发送”和“系统无法可靠等待完成”。
- Settings → Orchestration 使用 App 全局 health，不因切换窗口显示不同状态。
- `norma doctor` 为维护者提供文本和 JSON 两种诊断，不要求用户阅读日志才能识别版本漂移。

如果实现修改 Settings 页面或新增可见状态，必须遵循 Apple 设计语言、现有交互状态规范，并用 Computer Use 验收真实 App。

## 18. 持久化与迁移

### 18.1 App 全局数据库

建议新增：

```text
workspace_identities
- workspace_id PRIMARY KEY
- canonical_root UNIQUE
- display_name
- created_at_unix
- updated_at_unix

orchestration_task_routes
- task_ref PRIMARY KEY
- operation_id UNIQUE
- source_workspace_id
- source_definition_id
- source_provider
- source_node_id_at_dispatch
- target_workspace_id
- target_definition_id
- target_node_id_at_dispatch
- target_runtime_instance_id
- target_runtime_task_id
- parent_task_ref NULL
- root_task_ref
- state
- tracking
- error_code NULL
- created_at_unix
- updated_at_unix
- terminal_at_unix NULL
```

为 source/target workspace、state、root task 建立必要索引。

### 18.2 目标项目数据库

现有 `agent_dispatch_tasks` 继续保存详细审计，并补充：

- source_workspace_id
- target_workspace_id
- global_task_ref
- operation_id

所有更新必须使用 `global_task_ref` 或 `(runtime_instance_id, runtime_task_id)`，不能只用可能跨运行实例重复的 `runtime_task_id`。

### 18.3 迁移策略

- 旧项目首次打开时获取本机 WorkspaceId，不修改项目文件身份语义。
- 旧任务记录不强制生成全局路由。
- 可确定属于当前项目的旧 task ref 继续从项目 DB 查询。
- 无法确定来源/目标 Workspace 的旧任务返回 `legacy_task_route_unavailable`。
- 新表 migration 必须幂等。
- 测试数据库继续与生产数据库隔离。

## 19. 非功能性需求

### 19.1 性能

- 打开窗口不得启动新的 Norma/Hook listener。
- `workspace list` 和 `agent list` 只读取内存路由与名册快照，不同步扫描项目目录。
- 请求入口使用有界队列和事件唤醒，不使用每窗口 12ms 轮询。
- 普通本地 list/status 在稳定运行时应在 250ms 内返回 p95。
- assign 接收回执在不需要激活目标 Agent 时应在 500ms 内返回 p95。
- Hook 投递不得在 GPUI 前台执行网络或阻塞 IO。
- App 退出的 listener shutdown 必须有界，不能因 accept 永久挂起。

### 19.2 容量与背压

- listener client handler、请求队列和 Hook 队列都必须有显式容量。
- 容量数值在实现测试中根据最大并发 Workspace、Agent 和请求突发推导，不写无依据魔数。
- 过载返回 `server_overloaded`，不得无界创建线程或积压任务正文。

### 19.3 可靠性

- socket bind、全局任务注册和状态转换必须可重复、可对账。
- 窗口关闭和 App 退出不得遗留 listener。
- route 失效不得 fallback 到其他 Workspace。
- 状态查询不依赖窗口焦点。
- 超时不改变任务状态。
- 重复 Hook、重复 operation ID 和迟到旧 Session Hook 必须幂等。

### 19.4 可运维性

结构化日志至少包含：

- request_id
- operation_id
- task_ref
- source_workspace_id
- target_workspace_id
- source_definition_id
- target_definition_id
- provider
- route_id
- command
- state
- tracking
- error_code
- elapsed_ms

启动和退出额外记录：

- socket path 和 lease 状态
- listener thread start/stop
- Hook ingress address
- 注册 Workspace 数量
- 未完成任务对账数量
- App/CLI/Skill/Hook/协议版本

## 20. 技术落点

建议模块与职责：

```text
crates/norma/
├── protocol            请求上下文、Workspace/Task 协议、版本握手
├── client              CLI 上下文、operation ID、连接和错误映射
├── cli                 workspace/doctor/跨 Workspace 参数
├── orchestration       Skill 模板、安装与全局组件检查
└── agent_hook          Hook 转发、版本与诊断日志

crates/storage/
├── workspace_identity_store
├── orchestration_task_route_store
└── agent_dispatch_store 扩展 global_task_ref 与 Workspace 字段

crates/app/
├── norma/endpoint      App 全局 socket、listener、shutdown
├── norma/router        Workspace 注册与请求路由
├── norma/dispatch      GPUI 前台执行 adapter
├── norma/task_registry 全局任务路由与对账
├── settings/orchestration 全局 health adapter
└── agent_hook_ingress  App 全局 Hook listener 与 Session token 路由
```

依赖边界：

- `norma` crate 不依赖 GPUI、App、Runtime 或 Storage。
- `storage` 不依赖 App、Runtime 或 UI。
- 窗口和 GPUI Entity 路由只存在于 `app`。
- 任务队列规则继续由 `agent` crate 维护。
- 跨 Workspace lineage 的纯规则优先下沉到 `agent`，App 只做路由和副作用适配。

## 21. 实施阶段

### Phase 0：聚焦失败测试

- 多窗口反向关闭导致 socket 不可连接。
- 多窗口产生多个 Norma listener。
- AgentHookServer 关闭后仍接受并返回 202。
- 两个 Workspace node ID 碰撞导致错误来源校验。
- 已终态 untracked 任务被 wait 拒绝。
- 不同 runtime 中相同 runtime task ID 更新错误记录。

### Phase 1：App 全局 NormaEndpointHost

- 提取进程级 socket 模块。
- 实现 SocketLease、安全 stale 检查、shutdown 和 join。
- 在 `run_norma_os` 开窗前安装。
- 删除 `MultisoulApp.norma_server` 和窗口级 drain loop。

### Phase 2：Workspace 身份与路由

- 增加 WorkspaceIdentityStore。
- 增加运行期 WorkspaceRouter。
- 窗口注册/closing/注销。
- WireRequest 增加来源 Workspace 上下文。

### Phase 3：全局 Hook ingress 与 health

- 合并窗口级 Hook listener。
- 通过 Session token 路由事件。
- 全局化 Provider reconcile 和 Settings health。
- 修复 Hook 失败诊断与关闭后的错误响应。

### Phase 4：跨 Workspace CLI 与派发

- `workspace list`。
- `agent list --workspace`。
- `task assign --workspace`。
- 跨 Workspace AgentPrincipal、lineage、循环和预算校验。

### Phase 5：全局任务注册与可靠等待

- 全局 task ref 与 operation ID。
- App/项目 DB 分步持久化和对账。
- 状态/等待/取消路由。
- 终态优先于 tracking。
- App 重启把非终态任务对账为 interrupted。

### Phase 6：版本、Skill 与诊断

- `norma --version` 和 `norma doctor`。
- 协议版本握手。
- 更新受管 Skill。
- 旧 Agent Session 升级提示。

### Phase 7：E2E 与真实 App 验收

- Headless 多窗口/多 Workspace 流程。
- 真 Unix socket、真 CLI、真 Hook listener L3。
- Computer Use 验收受影响的 Settings 和用户反馈。

## 22. 验收标准

### 22.1 App 全局入口

- [ ] App 打开 1、2、10 个窗口时都只有一个 Norma Unix listener。
- [ ] App 打开 1、2、10 个窗口时都只有一个全局 Agent Hook listener。
- [ ] 关闭任意非最后窗口后 `norma doctor` 和 `agent list` 仍可连接。
- [ ] 窗口关闭不会删除 App 全局 socket。
- [ ] App 退出后 listener 线程全部结束，owned socket 被删除。
- [ ] 第二个进程不能删除第一个活跃进程的 socket。
- [ ] 普通文件或符号链接占用 socket path 时不会被覆盖。

### 22.2 Workspace 路由

- [ ] `workspace list` 只返回已打开、已注册 Workspace。
- [ ] 同名 Workspace 返回不同 workspace ID。
- [ ] 目标 Workspace 未打开时返回 `workspace_not_open`。
- [ ] Agent 请求缺少来源 Workspace 时不使用活动窗口猜测。
- [ ] route 在请求接收后关闭时返回 route expired/interrupted，不转移到其他窗口。
- [ ] 两个 Workspace 使用相同 node ID 和 Provider 时仍准确校验来源。

### 22.3 跨 Workspace 指派

- [ ] A 的 Agent 可以把任务派给已打开 B 的已启用 Agent。
- [ ] 目标 Agent 未激活时在 B 自动创建唯一卡片。
- [ ] 任务进入 B 的现有 coordinator 和 cwd。
- [ ] B 在后台时不被自动激活或抢焦点。
- [ ] A 和 B 中同名 Agent 不会混淆。
- [ ] 用户未明确指定跨 Workspace 时 Skill 不自行选择 B。
- [ ] 来源与目标 Workspace、Agent 和 operation ID 写入审计。

### 22.4 生命周期与任务状态

- [ ] 关闭 A 不会中断已经进入 B 的任务。
- [ ] 关闭 B 会把 B 的未完成任务对账为 cancelled/interrupted。
- [ ] App 重启后旧非终态任务可查询为 interrupted。
- [ ] 旧 task ref 不会命中新 runtime 中编号相同的任务。
- [ ] done/failed/cancelled/interrupted + untracked 能正常结束 wait。
- [ ] 非终态 + untracked 返回 task_untracked。
- [ ] wait timeout 不取消任务。
- [ ] cancel 精确作用于 task ref 对应的目标 Workspace 任务。

### 22.5 Hook 与 health

- [ ] 每个 Hook token 只能路由到注册的 Workspace、Agent 和 Session generation。
- [ ] 关闭 route 后的 Hook 请求不会返回虚假 202。
- [ ] 迟到的旧 Session Hook 不更新新 Session。
- [ ] Provider 状态 checking 时不显示为 untracked。
- [ ] 所有窗口读取同一份 Orchestration health。
- [ ] Hook 转发失败可以从本地诊断日志或 doctor 识别。

### 22.6 幂等与版本

- [ ] 同一 operation ID 重试只产生一个任务。
- [ ] 同一 operation ID 配合不同参数返回冲突。
- [ ] 响应丢失时返回 outcome unknown 和原 operation ID。
- [ ] Skill 不使用新 operation ID 盲目重试。
- [ ] App、CLI、Skill 和 Hook 版本可由 doctor 读取。
- [ ] 协议不兼容在副作用前失败。

### 22.7 安全与隔离

- [ ] Provider token 不能跨 Provider 使用。
- [ ] 请求不能通过伪造 node ID 路由到其他 Workspace。
- [ ] 目标 Agent 默认不能获得来源 Workspace 的上下文或文件列表。
- [ ] 日志和 JSON 不包含 token、Authorization header 或默认完整任务正文。
- [ ] 未授权 AgentPrincipal 不能仅凭 task ref 查询任务。

## 23. 自动化证据计划

| 验收范围 | 自动化证据 |
|---|---|
| SocketLease、stale/live socket、shutdown | App 单元测试 + 真 Unix socket L3 |
| Workspace identity 与 route 状态机 | 纯单元测试 |
| 来源/目标路由和 node ID 碰撞 | App 单元测试 + e2e |
| 跨 Workspace assign/list/status/wait/result/cancel | `crates/e2e` 双 Workspace 用例 |
| Hook token 路由与迟到事件 | App/Norma 单元测试 + Hook L3 |
| 终态优先于 tracking | Norma CLI Runtime 单元测试 |
| task ref、operation ID、持久化对账 | Agent/Storage 单元测试 |
| App 重启与 listener 清理 | 独立真实 `.app` L3 runner |
| Skill/CLI/App 版本兼容 | Norma installer/CLI 测试 |
| Settings 全局 health | e2e + Computer Use（如 UI 有变更） |

实现完成后按改动范围至少运行：

```text
cargo test -p norma
cargo test -p agent
cargo test -p storage
cargo test -p app
cargo test -p e2e
cargo check -p app
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
```

跨 crate 协议与状态稳定后运行 `cargo test --workspace`。真实 socket、真实 `.app` 退出和线程数量必须由 L3 runner 验证，不能只用 headless GPUI 代替。

所有新增单元测试必须遵守项目规定的完整场景、数据构造、执行过程、预期结果、正负断言和数值推导注释规范。

## 24. 风险与权衡

### 24.1 已确认权衡

1. 只支持已打开 Workspace

   避免后台自动打开项目、恢复大量 Session 或产生用户不可见副作用。

2. 不恢复执行队列

   App 重启后把非终态任务对账为 interrupted；优先保证身份和状态真实，而不是伪装继续运行。

3. 目标 Workspace 隔离

   不自动传递来源上下文，牺牲一部分便利性以换取文件和项目边界清晰。

4. 全局注册、局部执行

   全局模块负责身份、路由和审计；现有 Workspace coordinator 继续负责 FIFO 执行，避免重写全部任务队列。

5. 同一 macOS 用户信任边界

   首版不引入多租户 ACL，但所有跨 Workspace 行为仍需用户明确授权和完整审计。

### 24.2 已知风险

- App 全局表与目标项目 DB 不能使用单个 SQLite 事务，需要分步写入和启动对账。
- Workspace 移动后本机 identity 映射需要明确迁移或按新位置登记。
- Provider Hook 格式可能变化，安装 Ready 不等于运行期每个事件都一定成功。
- 跨 Workspace 父子任务会让循环检测和后代预算从局部提升到全局。
- 全局 Hook ingress 的 token 路由表必须正确清理，否则会形成新的内存泄漏。
- 旧 Agent Session 缺少新 Workspace 环境，升级后必须重启才能使用。
- task ref 可持久查询会扩大状态数据保留范围，需要后续定义清理周期。

## 25. 默认决策与后续可调参数

本草案采用以下默认决策，评审时可以调整，但实现前必须确定：

- 目标 Workspace 必须已打开。
- `--workspace` 使用稳定 WorkspaceId，不使用显示名称作为最终寻址键。
- 来源窗口关闭后，目标任务继续。
- 目标窗口关闭后，目标任务中断。
- App 重启后不恢复执行，但旧任务状态可查询。
- 全局任务表不保存完整任务正文。
- 跨 Workspace 不自动共享文件和对话上下文。
- 不新增批量派发。
- 不自动使用 `--force-untracked`。
- 不允许活动窗口 fallback 处理 Agent 编排请求。

后续需要通过实现测量确定的参数：

- Norma 请求队列容量。
- Hook 请求队列容量。
- listener client 并发上限。
- operation idempotency 记录保留周期。
- terminal task route 保留周期。
- shutdown join 超时和失败后的诊断策略。
