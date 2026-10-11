# 工作空间智能体卡片派工与进度汇报：开发任务拆分

日期：2026-10-10。实施更新：2026-10-11。状态：实施中；T01／T02／T03／T05 已合入，T06 账本核心与 T09 纯聚合合同部分完成，T04／T07 实施中；正式派工与进度查询闭环仍未完成。

实施合同以已发布的 [GitHub Issue #96](https://github.com/multisoul-ai/Norma-AI-OS/issues/96)及其[本地规格副本](../product-specs/2026-10-10-workspace-agent-task-dispatch.md)为准，Issue 标记 `ready-for-agent`。源码事实、两张运行流程图与完整验收矩阵见[派工方案](../design-docs/2026-10-09-workspace-agent-task-dispatch.md)；术语见[CONTEXT.md](../../CONTEXT.md)。本计划记录本地任务依赖，不作工期承诺。

## 首版边界

- Dock Norma 协调整个当前工作空间；展开项目仅提供上下文。
- 一个空间内角色名称与卡片一对一；主名称与中文语音别名共享唯一呼叫键。
- 改名使旧称呼失效，原角色仍存在时禁止其他角色复用，历次失效占用在原角色彻底删除成功后才释放。
- 历史重名按固定顺序自动修复；任务、历史引用与重试使用稳定身份。
- 明确点名不存在或失效时保留任务并补全目标；仅指定项目时唯一候选自动选中，多候选选择，零候选补全。
- 候选覆盖全部画布、后台与未加载区域；未加载目标异步准备，不启动全部 PTY 来获取名单。
- 沿用现有 CLI 队列；Dock 逐项反馈，点击才导航；安全失败手动重试，未知结果先核对。
- 主动汇报整个当前工作空间的进度：总体、项目、角色／任务、成果和待处理事项；含后台、未加载快照及直接卡片工作，未知或过时明确标注，只读查询不派工、不启动 PTY。默认全部当前未结束任务／已知异常与最近 24 小时成果，可明确其他时间范围。

本轮不增加按能力选人、新建角色派工、任务依赖或智能体跨项目互派。完整两端群聊页面与空间删除后的历史保留期限另行设计。

## 任务与依赖

当前状态与验证证据见下节。代码落点表示职责范围，具体文件拆分以施工时源码与单文件 500 行约束为准。

| 编号 | 任务与代码落点 | 前置任务 | 完成条件与先写的失败场景 |
| --- | --- | --- | --- |
| T01 | 持久卡片身份与目标协议；`norma`／`agent` 的入口无关 DTO、`runtime` 纯校验、`storage` 的恢复映射 | 无 | 相同 node_id、同目录多画布仍定位不同卡片；删除后重建获得新身份，旧请求拒绝；恢复原卡片保持持久身份，运行代次不混用 |
| T02 | 工作空间角色与呼叫键登记；`agent` 名称规范化、`storage` 唯一约束及创建／改名／删除事务 | T01 | 主名称与别名交叉冲突被拒绝，大小写统一；失效占用也阻止其他角色复用；连续改名、重启与停用不释放；删除失败不释放，成功才释放所属全部键；并发同名只有一项成功 |
| T03 | 完整空间登记与旧数据自动修复；`storage` migration、`app` 后台恢复与卡片标题投影 | T02 | 包含未加载区域与无定义卡片；固定保留一个重名角色，其他自动获得合法未占用称呼；共享旧键不重复变成失效占用；恢复中断可重试且不重复改名；其他空间不被共享定义变更影响 |
| T04 | 工作空间目标解析；`runtime` 纯路由、`agent` 工具计划校验、`app` 完整目录与候选投影 | T01、T02、T03 | 点名与语音别名只查有效键；旧称呼零接收零投递；明确项目的零／一／多候选行为准确，后台与未加载候选计入；目录不完整、项目歧义或任务指代不清时不猜目标 |
| T05 | 卡片实例级协调与 CLI 适配；`agent` coordinator／delivery，`app` adapter、pending、Hook 与 task_result | T01 | 调度、回复、队列和结果均关联具体实例，定义 ID 不再决定第一张卡片；无定义卡片有明确提交与跟踪能力；A 未结束不阻止合法提交 B，B 不被提前标为运行或完成；审批与用户输入占用不被绕过 |
| T06 | 通用操作账本与跨区域投递；`storage` operation／assignment／event，`app` 后台加载、授权与副作用入口 | T04、T05 | 重发同操作不产生新输入，同键不同参数冲突；预检后原子接收多目标，再独立投递；持久化等待或加载期间目标失效则拒绝；正文／Enter 与写回崩溃标为待核对，不自动重放 |
| T07 | 工作空间 Norma 与桌面 Dock 接入；`agent` bounded workspace context／typed tools，`app` command_dock 与手机共享适配 | T04、T06 | 总览与展开项目均能给另一项目指定卡片派工；发送时冻结来源空间，切换视图不改归属；讨论不派工；首个跨项目流程真实执行并显示接收回执，所有非目标卡片零输入 |
| T08 | 回执、导航、手动安全重试与恢复闭环；`app` Dock 状态投影／结果查询，`storage` 结果快照与尝试关联 | T06、T07 | 每项进度由证据更新，终态有摘要和结果入口；后台结果不抢焦点；点击才导航且不打开同名新卡片；失败重试复核原身份，成功项不重发；未知先核对；区域移除后已知历史仍可查看 |
| T09 | 全空间进度汇报；`norma` 只读查询 DTO、`agent` query typed tool、`app` 共享事实聚合与 Dock 回复、`storage` 持久任务／结果／活动快照读取 | T03、T05、T07、T08 | 展开 A 查询仍覆盖 B 后台与 C 未加载／停用角色；直接卡片工作与未关联活动可见，稳定任务引用去重、状态／步骤分列；全部当前未结束任务不按创建时间过滤，24 小时成果窗口与指定时间可核对；部分读取失败、缺失计划和旧快照不猜完成；手机先授权再聚合；零 assignment 创建、零卡片输入、零 PTY 启动，无自动审批／重试／订阅，查询与点击导航均不串空间 |

## 实施状态与证据

| 编号 | 状态 | 已有证据／下一步 |
| --- | --- | --- |
| T01 | 完成，已合入 `228e751f`（实施提交 `f247ac4f`） | 持久卡片身份、冻结目标协议与 Runtime 纯校验已进入生产恢复路径；下述聚焦和受影响 crate 验证通过 |
| T02 | 完成，已合入 `3e10d43d`（实施提交 `a84e20ec`） | 工作空间角色／有效与失效呼叫键事务及严格只读快照已验证；App 生命周期接入属于 T03 |
| T03 | 实现与自动验证完成，已合入 `e899a580`（实施提交 `751ff5c5`） | 完整登记／自动修复／两阶段创建／标题编辑／持久删除凭据已接入；真实 App 视觉与流程验收留在整体集成阶段 |
| T04 | 实施中，未完成 | 纯解析合同与目标 manifest 已开始；生产目录接入仍依赖 T03 |
| T05 | 完成，已合入 `2ef7854d`（实施提交 `24cc5fa8`） | 实例协调、独占输入传输与 exact Hook 证据关联已验证；共享派工入口由 T06 接通 |
| T06 | 部分完成：账本核心已合入 `8acb39e5`（实施提交 `631478b1`） | 来源／持久原子接收／Armed 屏障／可靠 journal／只读与 exact 证据已验证；正式 App facade、frame writer、实时权限 gate 与 PTY 反例待完成 |
| T07 | 实施中，未完成 | 整组规划与来源入口开始；生产接线依赖 T04／T06，Dock 真实 App 闭环尚未验收 |
| T08 | 未完成 | 等待工作空间入口；回执、重试与恢复尚未验收 |
| T09 | 部分完成：纯聚合合同已合入（实施提交 `4a823afc`、`1b598e9b`） | 统计／证据规则与有界分页已验证；只读事实接入、typed tool、Dock 呈现／导航与真实 App 验收仍待完成 |

T01 的验收对应 [`workspace_card_target.rs`](../../crates/norma/tests/workspace_card_target.rs)、[`workspace_card_identity.rs`](../../crates/runtime/tests/workspace_card_identity.rs)与[`card_instance_identity.rs`](../../crates/storage/tests/card_instance_identity.rs)：同卡恢复／改名保持身份，删除后相同 node_id 重建拒绝旧目标；同目录多区域独立，两个后台回填事务只发布一份身份；Runtime、route、PTY、run／session 变化拒绝旧指纹，记录型 PTY 验证零输入。普通 OpenCode PTY 也有可冻结的 terminal session。

实施者已完成 `cargo test --quiet -p canvas -p norma -p runtime -p storage`、`cargo check --workspace`、fmt、存储／性能边界及行数／no-allow／指南护栏，域层依赖树无 GPUI。合入无冲突、无其他任务改动；集成分支另运行三个新增集成测试验证公共接口。T01 的存储恢复调用在后台执行；严格只读汇报仍须 T09 新建 readonly reader，不能调用 `load_snapshot_with_card_identity()` 或默认确保 schema 的 reader。

真实 Provider 完成证据、派工 UI 与全空间汇报尚未验证，不能由 T01 测试推定后续任务完成。

T02 提供 [`WorkspaceAgentRoleStore`](../../crates/storage/src/workspace_agent_role_store.rs) 的创建、改名、启停、彻底删除后的释放、解析及完整快照接口。主名称与内置中文别名共用 `(workspace_id, normalized_key)` 唯一键，有效／失效状态共同占用；立即写事务确保并发同名仅一项成功。改名失败保留原称呼与 revision，历次旧键在停用／重开后仍失效占用，自身也不得复活；精确原卡确认删除成功才释放。`snapshot()`／`resolve()` 使用只读连接，缺文件或 schema 明确返回缺口，零创建／迁移；`schema_present` 不表示全空间登记完成。

验收对应 [`workspace_agent_roles.rs`](../../crates/storage/tests/workspace_agent_roles.rs)、[`workspace_agent_role_readonly.rs`](../../crates/storage/tests/workspace_agent_role_readonly.rs)与[`agent_definition_name_normalization.rs`](../../crates/storage/tests/agent_definition_name_normalization.rs)，共 9 条公开接口场景，覆盖跨空间独立、主名／别名交叉冲突、连续改名、错误目标／存储失败不释放、并发抢名、非法名称及目录只读。名称统一 trim 的项目定义兼容回归也通过。实施者完成完整 `cargo test --quiet -p agent -p storage`、正常提交 hook 的 workspace check／fmt 及存储／性能／行数护栏；集成合入无冲突、无无关改动，另运行上述三组聚焦测试。

T02 自身不自动登记旧卡片或更改共享定义的具名身份；生产接线和变更记录的证据见 T03，不能只由 T02 模块测试推定 App 路径完成。

T03 的 `register_workspace()` 扫描全部持久区域，后台持久回填 UUID 后以单个 AppDB 事务固定排序登记和修复，最后精确补当前 Canvas 的角色标题。已有登记优先，未加载、无定义与停用卡片参与，重复恢复不再次改名；共享旧键不会重复变成失效占用。两阶段创建先登记名称与 disabled 意图，再精确追加原 node／UUID，最后启用；未知发布仍保留占用及 publication barrier。永久删卡凭据和新 Canvas 在同个事务保存，确认删除后才释放名称；缺文件／缺节点但没有删除凭据仍保留占用。

卡片标题的角色编辑按原 UUID 改名／启停并显示修复记录，原位投影标题不替换 Runtime 或冻结指纹，冲突保留输入；共享配置定义改名不再同步空间角色。桌面和手机创建均异步登记、精确复核原区域／session／Runtime，再安装原身份并后台启动 PTY。迟到回调、同 CanvasDB 重绑或读取未知不能解除发布保护，旧脏 Document 不得覆盖后台已发布卡片。

公开验收对应 [`workspace_role_reconciliation.rs`](../../crates/storage/tests/workspace_role_reconciliation.rs) 3 条、[`workspace_role_creation.rs`](../../crates/storage/tests/workspace_role_creation.rs) 3 条、[`workspace_role_directory.rs`](../../crates/storage/tests/workspace_role_directory.rs) 4 条、[`reserved_card_identity.rs`](../../crates/canvas/tests/reserved_card_identity.rs) 1 条、[`registered_agent_launch.rs`](../../crates/runtime/tests/registered_agent_launch.rs) 1 条。实施者已完成 agent／canvas／runtime／storage 完整测试，App role publication 3、desktop creation 3、mobile creation 4 与 App／workspace check、正常提交 hook 和护栏；最终重读真实 Canvas 和名称占用，不只检查 publication 计数。集成合入无冲突，保留 T05 bootstrap／frame／Accepted Hook 状态，另跑上述 12 条公开测试与 App check。

T03 引入的角色编辑遵循 Apple 直接操作、即时错误反馈与键盘可访问性，使用现有 Input／Button／Dialog；主题、hover／pressed／focus／disabled 和真实创建／改名／删除仍须 Computer Use 验收。旧同步 definition-first Create／Activate 等兼容入口目前明确拒绝未登记启动，只是过渡保护；T07 必须接共享注册服务恢复显式创建，不能以永久拒绝宣称完整规格完成。

T05 实施及证据见[卡片实例级派工验收记录](2026-10-11-card-instance-dispatch-evidence.md)。`agent::card_instance` 保存完整冻结目标，`MultisoulApp` 提供纯预检、受授权 gate 保护的接收、逐项回执／事件、提交前取消和精确终态核对。正文／Enter 的传输互斥，不等待前任务完成；B 只有自身可信 start 才运行，迟到 A 不结束 B。普通无定义卡片可传输，无可靠跟踪则保持 Untracked。重复旧 Hook 在 Runtime 状态更新前拒绝，App 按事件接收时的私有卡片／执行身份核对同批事件。停止／替换项目废弃输入队列，历史不自动重放。

领域公开接口验证对应 [`card_instance_coordination.rs`](../../crates/agent/tests/card_instance_coordination.rs)、[`card_instance_untracked.rs`](../../crates/agent/tests/card_instance_untracked.rs)、[`workspace_card_input.rs`](../../crates/runtime/tests/workspace_card_input.rs)与[`workspace_card_hook_identity.rs`](../../crates/runtime/tests/workspace_card_hook_identity.rs)。App 的 10 条 `card_instance` 和 11 条 `agent_hook` 聚焦场景覆盖非目标零输入、草稿／审批／旧队列保护、部分写入未知、撤权／会话替换、重复操作、原 turn 中断、同批旧新 Hook 与终态后 SessionStart。实施者完成完整 agent／runtime 测试、App／workspace check、正常提交 hook 及护栏，并经独立对抗审查关闭 4 项发现；集成合入无冲突，另运行上述聚焦接口验证。

T06 必须在持久整组接收前调用预检并检查组内冲突，接收使用 authority gate，在后台持久化事件；冷启动准备后才捕获指纹，不刷新已接收目标。Task／Reply 含义由账本保存，T05 输入 seam 不自行判定。T08 必须用可信原 run／session／turn 的 RPC 结果核对终态；未绑定的 Unknown 不按正文猜归属，不安全输入隔离不会因摘要或结果回执自动释放。严格核对后释放隔离的恢复接口仍需后续设计与验收。

以上测试仅证明实例寻址、输入与事件规则；真实 Provider 完成、正式跨区域入口、Dock／回执 UI 和导航尚待 T06–T08 验收，不将录制 PTY 当作真实任务完成。

T06 核心提供共享 `WorkspaceOperationSource`、`WorkspaceOperationStore` 与后台 `BackgroundWorkspaceOperationJournal`。来源空间、主体、消息和操作保留原文；原请求 fingerprint 由私有入口生成，模型不能赋予来源／mode／force。整组单事务接收且同操作只有一个投递 winner，同键改空间／参数／attempt 则冲突；Task／Reply／Retry 保留 canonical 逻辑任务与精确父 turn，不复用其他任务引用。历史名称／项目／区域／执行身份不随当前视图或改名刷新。

`arm_input()` 在后台复核角色、项目与手机 Control grant 后持久 commit Armed，只有首次成功才可装入输入队列；重复 Armed 不重放。事件整批原子保存、去重且不回退状态，正文和 Enter 分列；未绑定 Unknown 不接受伪 turn 终态，原 exact 终态仅允许一次同 turn 的结果补充。重启只注释中断／待核对，不恢复队列。Journal 写失败保留原 batch 并关闭 watched authority，恢复不自动复活旧权限；sender 失败返回原事件给调用者，不在前台降级 SQLite。

原请求／计划 lookup、回执、事件和手机认证使用严格只读连接，缺 DB／schema 零创建／迁移。历史事实 API 本身不认证；公共查询须先按当前授权过滤后返回。等待原因改变产生新证据，未改变不重复刷事件；已经可能写输入的取消不能伪造零输入成功。

公开验收对应 [`workspace_operation.rs`](../../crates/norma/tests/workspace_operation.rs) 1 条、[`card_instance_waiting.rs`](../../crates/agent/tests/card_instance_waiting.rs) 3 条，以及 Storage 的 [`workspace_operations.rs`](../../crates/storage/tests/workspace_operations.rs) 9 条、[`workspace_operation_evidence.rs`](../../crates/storage/tests/workspace_operation_evidence.rs) 4 条、[`workspace_operation_lineage.rs`](../../crates/storage/tests/workspace_operation_lineage.rs) 5 条、[`workspace_operation_journal.rs`](../../crates/storage/tests/workspace_operation_journal.rs) 1 条、[`workspace_operation_results.rs`](../../crates/storage/tests/workspace_operation_results.rs) 1 条、[`mobile_access_readonly.rs`](../../crates/storage/tests/mobile_access_readonly.rs) 1 条。实施者完整 norma／agent／storage 测试 484 passed、1 ignored 与正常提交 hook 均通过；新增错 turn、组内引用、原请求 hash、BodyWritten 后取消和改空间重发反例已实际修复。集成保留 T03 导出／migration 与 T05／T09 模块，并另跑上述 25 条新增公开核心测试。

这是 T06 第一阶段，未接生产 frame writer 或共享投递服务。第二阶段仍须用不可变原 manifest／首次 code resolution 绑定整组目标，在每个 await 后复核范围，先查幂等再只准备必要区域、捕获冷卡真实指纹、持久 Armed 后 guarded 接收，并可靠保留事件。Query／Discussion 不走加载全区域的旧桥；实时停用／删除／撤权、公开 PTY／DB 反例、独立审查和真实 App／Provider 验收仍待完成，T06 不标为全部完成。

T09 纯核心提供 [`norma::workspace_progress`](../../crates/norma/src/workspace_progress.rs) 的 `aggregate_workspace_progress()`。其 15 条公开接口场景分别验证稳定任务／显式 turn 关联去重、角色与未关联活动分列、最新尝试阶段和旧异常保留、可信终态与迟到快照／同名新卡隔离、24 小时边界及指定窗口、不截断总计的分页、授权区域过滤、目录／证据缺口和无可靠卡片身份的历史记录。没有任务／卡片身份的旧称呼记录独立待核对，不绑定同名现存角色；缺失观察时间保留未知。

对应测试为 [`workspace_progress.rs`](../../crates/norma/tests/workspace_progress.rs)、[`workspace_progress_activity.rs`](../../crates/norma/tests/workspace_progress_activity.rs)、[`workspace_progress_evidence.rs`](../../crates/norma/tests/workspace_progress_evidence.rs)、[`workspace_progress_history.rs`](../../crates/norma/tests/workspace_progress_history.rs)、[`workspace_progress_scope.rs`](../../crates/norma/tests/workspace_progress_scope.rs)与[`workspace_progress_time.rs`](../../crates/norma/tests/workspace_progress_time.rs)。实施者已完成完整 `cargo test -p norma` 与正常提交 hook；合入时保留 `workspace_dispatch`／`workspace_progress` 两个模块并再次运行上述六组聚焦测试及目标协议测试。

该核心没有存储、Runtime、PTY 或派工副作用，但不能替代生产查询服务的零副作用验收。readonly AppDB／区域 DB reader、持久活动及结果事实、查询前后权限／目录代次复核、Mac 时区自然语言窗口、入口与实际结果导航仍待接入；T09 不标为完成。

## 实施里程碑

1. **名称与身份可持久恢复（T01–T03）**：先守住跨项目唯一性、旧称呼失效占用、自动修复与删除释放。现有名称兼容路径也不能绕过登记。
2. **指定现有卡片的跨项目闭环（T04–T07）**：先验证“展开 A，在 Dock 指定 B 的卡片，B 收到独有任务，A 无输入且视图不跳转”。结构化目标和自然语言工具最终调用同一副作用入口。
3. **首版完整反馈与失败恢复（T08）**：覆盖多目标、后台／未加载、改名、删除、重启和不确定提交。收到消息、任务接收、提交、运行与终态分别验证。
4. **主动全空间进度汇报（T09）**：复用共享 query 与活动／任务事实，汇报空间总览、逐项目进展、成果与用户待处理事项；注明范围、更新时间、未知和旧快照。验证“展开 A 查询，B 后台与 C 未加载仍有明确结果，所有卡片零新增输入”。

T05 的实例调度接口可在 T01 后开发，但正式入口必须等完整名称登记与目标校验接通后开放。不要启用测试特性中的旧 dispatcher 代替上述闭环。

T09 的聚合合同可先复用现有活动汇总、可信任务步骤与摘要测试开发；正式 Dock 入口依赖完整目录、实例级事实与结果快照接通，不另建派工执行器。最近 24 小时成果窗口为本轮补充的默认建议，周期汇报、推送、跨空间总览与新的项目计划管理另行设计。

## 验证与工程约束

每项先写能失败的聚焦测试，再实现；单测／集成测试按受影响 crate 维护，并按仓库规范写完整场景注释和具体断言。验收矩阵中的正例同时验证非目标卡片零输入、无重复提交等反例。

- `agent`、`runtime`、`storage`、`norma` 等受影响 crate 运行相应 `cargo test -p <crate>`；`app` 运行 `cargo check -p app` 及适用测试；跨 crate 行为按仓库要求验证。
- SQLite、目录扫描、文件 IO 和加载放后台；纯路由零副作用，GPUI 前台不阻塞，热路径不新增查库或会话扫描。
- 编写 GPUI 前加载对应技能；选择器遵循 Material 菜单规范，其余交互按 Apple 设计原则落实即时反馈、层级和可访问性。
- 真实 `.app` 用 Computer Use 验证 Dock、选择器、跨项目提交、全空间进度查询及结果入口、回执导航、浅深主题、键盘和焦点；headless 点击不能替代此项。
- 可信完成证据使用支持的真实 Provider／Hook 验证；Echo PTY 只证明输入送达。旧 `crates/e2e` 已禁用，不新增、维护或运行。
- 代码 review 按仓库要求由独立 subagent 做对抗式审查，实施者核验发现；提交不绕过 hook。源码行数、存储与依赖边界检查按变更范围运行。

## 后续事项

完整两端聊天历史呈现、空间归档／删除后的历史保留期限、自定义语音别名编辑范围、按能力选人、任务依赖与智能体互派继续作为后续范围，不影响本次明确卡片派工的规则。实施时发现真正影响首版行为的新矛盾，应先记录具体触发场景，再讨论该项，避免重新打开全部已确认决定。
