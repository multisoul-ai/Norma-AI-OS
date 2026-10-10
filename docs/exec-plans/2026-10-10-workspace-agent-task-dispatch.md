# 工作空间智能体卡片派工与进度汇报：开发任务拆分

日期：2026-10-10。实施更新：2026-10-11。状态：实施中；T01 已合入，T09 纯聚合合同部分完成，正式进度查询仍待接入；其余任务未完成。

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
| T02 | 可开始 | 基于已合入的 `WorkspaceCardRef` 实现角色与呼叫键登记 |
| T03 | 未完成 | 等待 T02；后台登记必须先持久回填旧快照身份 |
| T04 | 未完成 | 等待完整名称登记与目录修复 |
| T05 | 可开始 | 基于已合入的 `WorkspaceCardTarget` 实现实例协调与输入传输 |
| T06 | 未完成 | 等待 T04／T05 的生产接口 |
| T07 | 未完成 | 等待通用操作与跨区域投递；Dock 真实 App 闭环尚未验收 |
| T08 | 未完成 | 等待工作空间入口；回执、重试与恢复尚未验收 |
| T09 | 部分完成：纯聚合合同已合入（实施提交 `4a823afc`、`1b598e9b`） | 统计／证据规则与有界分页已验证；只读事实接入、typed tool、Dock 呈现／导航与真实 App 验收仍待完成 |

T01 的验收对应 [`workspace_card_target.rs`](../../crates/norma/tests/workspace_card_target.rs)、[`workspace_card_identity.rs`](../../crates/runtime/tests/workspace_card_identity.rs)与[`card_instance_identity.rs`](../../crates/storage/tests/card_instance_identity.rs)：同卡恢复／改名保持身份，删除后相同 node_id 重建拒绝旧目标；同目录多区域独立，两个后台回填事务只发布一份身份；Runtime、route、PTY、run／session 变化拒绝旧指纹，记录型 PTY 验证零输入。普通 OpenCode PTY 也有可冻结的 terminal session。

实施者已完成 `cargo test --quiet -p canvas -p norma -p runtime -p storage`、`cargo check --workspace`、fmt、存储／性能边界及行数／no-allow／指南护栏，域层依赖树无 GPUI。合入无冲突、无其他任务改动；集成分支另运行三个新增集成测试验证公共接口。T01 的存储恢复调用在后台执行；严格只读汇报仍须 T09 新建 readonly reader，不能调用 `load_snapshot_with_card_identity()` 或默认确保 schema 的 reader。

真实 Provider 完成证据、派工 UI 与全空间汇报尚未验证，不能由 T01 测试推定后续任务完成。

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
