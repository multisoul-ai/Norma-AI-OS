# Goal：幕僚模式 M1

状态：M1 与用户新增复验修正已完成；工作空间汇总、等待/流式/真实计划、原型页面对照均通过本轮真实 App 中英文 4/4 验收。PR #235 不自动合并。基线：`dev@e555367e2`。分支：`feat/staff-m1-20261112`。

## 目标与边界

完成「创建任务 → 与 Norma 对话 → 提议 → 用户采用 → 新 Worker 卡片执行 → 结果回到任务 → 验收」闭环，并提供与画布并列的幕僚视图。

依据：`2026-11-12-norma-staff-tasks.md` 第 9 节、`../prototypes/norma-staff-mode/`、`../design-docs/2026-11-12-norma-staff-architecture.html`。交接文件从 PR #190 的未提交工作区复制，代码仅以 dev 为基础，未修改来源工作区。

M1 包含 Task/Worker、原子快照、任务对话、采用后分派、hook 结果关联、自动 phase 与手动保护、三栏 UI、双语和真实 App 验收。M2 的回收/唤起/重启对账/热会话管理，M3 的 Goal 迁移/未归属卡片/自动派活上限（默认 8，可配置）均后置。

## 已定决策

- `Interrupted` 保持静默，任务显示需要你；不改 Router/triage 的分诊规则。
- 新增 `Lane::Task(task_id)`；AgentFinished 携带宿主验证的 node_id 与本轮 dispatch_id。以任务的 Worker 绑定查归属，拒绝重复与旧 dispatch。
- Task.messages 是会话事实来源；事件持久化成功后才投递。`from_result=false` 不视为取得结果，失败/中断留下明确终态消息，不自动重试派活。
- Norma 从 M1 自动推进 phase；用户修改后自动推进不覆盖；已完成任务收到新消息自动重开并记录事件。
- 验证现有适配器的一任务一会话能力；无法证明可靠隔离时采用已授权降级：复用主会话，逐任务重放 messages，任务轮次禁止播报。
- 所有实际分派必须对应用户采用的提议；同一采用重试不能重复创建卡片。

## 实施顺序与验收映射

| 步骤 | 交付 | 验收边界与证据 |
| --- | --- | --- |
| 1 | norma 协议 Task/Worker 与 brain TaskStore | 公共类型/Store 接口：状态优先级、手动 phase、完成重开、原子保存恢复、损坏文件保护、跨任务/跨工作空间隔离 |
| 2 | 服务路由、任务消息与会话 | BrainService：先保存再投递、重复/迟到结果拒绝、静默不唤醒、任务不播报、任务摘要；适配器探测或降级证据 |
| 3 | CLI、提议采用、Worker 启动与 hook | norma CLI + 宿主集成：任务身份独立于指令文本，dispatch 环境透传、用户采用授权、新卡片与结果精确关联、无结果/失败/中断 |
| 4 | 幕僚三栏界面 | App：画布/幕僚切换、分组任务列表、新建与输入、提议采用、阶段修改、全屏/Esc、复用右抽屉与文件标签 |
| 5 | staff desktop-e2e 模块 | `bash scripts/desktop-e2e.sh staff --visual`：闭环、三栏/切换/全屏/文件、中英、当前可选主题，逐图给出模型视觉判定 |
| 6 | 收尾 | 受影响 crate 测试、app check/release、必要 lint、独立对抗式审查、正常 hook 提交、创建面向 dev 的 PR |

各逻辑切片先运行聚焦失败测试，再写最小实现；测试遵守场景/数据/过程/预期与具体正负断言规范。测试 seam 已由交接文档的域层单测、真实 App 验收要求确定。

## 设计落地

遵循 apple-design 的清晰层级与用户控制：左侧任务、中间对话、右侧既有内容抽屉保持稳定位置；切换与按下立即反馈；可退出全屏，保留任务输入与选择；优先现有 gpui-component 控件。阶段选择采用紧凑菜单，区分当前阶段和键盘焦点。状态不仅靠颜色，中英文及当前可选主题检查可读性与 hover/focus/disabled。dev 的 ThemeId::ALL 已下线浅色主题，旧浅色配置会迁移到 InkBlack；本次使用 InkBlack 与 SapphireNight，不能宣称浅色真实 App 验收。不新增装饰性动效或依赖。

## 风险与验证策略

- 旧 CLI 的 `assign --task` 已表示任务文本：保留兼容入口，为 staff 身份提供明确无歧义的新解析分支，并覆盖解析测试。
- GPUI 前台不进行快照读写与 ACP 等待；brain 持有唯一写入口，App 订阅快照与异步响应。
- 持久化失败不得发布成功快照/唤醒任务；文件损坏保留原件，不用空状态覆盖。
- 按具体改动运行 norma/brain 及其他受影响 crate 的测试，不跑全工作区套件；真实 E2E 使用隔离数据及工作树自己的二进制，不结束用户 App。
- 现有主目录有用户未提交改动，保持原状；新 worktree 独立开发。PR #190 不提交、不合并。

## 执行记录

- 已读取交接任务文档、原型与架构页；确认 dev 有 scoped_router，已创建独立 worktree。
- 适配器探测：Codex ACP 2.2.2 与 OpenCode 可返回不同 session id，Claude 适配器未安装；未证明完整 prompt 隔离，使用已授权的共享主会话＋Task.messages 重放。证据：`artifacts/staff-m1/acp-probe.json`。
- 独立审查已发现并修复：启动早到 Ready 被二次注册清空（聚焦测试先红后绿）、不同服务代写入覆盖、等待审批未进入需要你、Worker 定位未退出全屏、缓存错误未随语言切换；停用期间结算现由独立后台串行保存，保存成功后才通知模型服务，入队后立即停用的 App 回归已通过。
- 2026-11-12 首次 `staff-workflow --visual` 被驱动判 ENV：macOS 屏幕锁定，未发送键鼠；已请求用户解锁。不能据此宣称视觉验收通过。
- `rtk cargo test -p brain -p norma -p runtime -p ui`：brain、norma 全部通过；runtime 旧 OpenCode 进程测试在并发构建负载下超时，缺少 calls.txt。未修改该既有用例，随后按串行复验。
- `rtk cargo test -p runtime -p ui -- --test-threads=1`：422 通过，11 忽略；含 Worker 身份和提前 Ready 回归。
- `rtk cargo test -p app --lib disabled_brain_preserves_worker_completion_for_background_persistence`：1 通过，1785 过滤；覆盖存活服务入队后立即停用、终态落盘且模型不唤醒。
- 最新 `rtk cargo check -p app`、debug build、SwiftPM staff 场景编译通过。i18n 2258 双语键、行数/allow/资源生命周期/存储边界/性能/指南行数/SOCKS 静态护栏通过；`rtk cargo lint-resources` 通过。
- 独立对抗式审查完成两轮以上，已核验并处理发现；最后复核未发现 M1 核心阻塞。验收发现的输入校验、主题反馈和全屏避让修复也经过独立复核。
- 最终 app check、debug/release 构建、资源 clippy 与相关静态护栏通过；PR：https://github.com/multisoul-ai/Norma-AI-OS/pull/235。
- 解锁后保留的失败证据：`2026-11-12T15-05-06Z` / `15-08-18Z` 场景遗漏默认收起的右抽屉前置操作，已显式展开。`15-11-28Z` 首条任务 Enter 触发 GPUI 单行换行断言；复用命令 Dock 的 validation 修复，`15-20-28Z` 已成功落盘用户消息。后者又暴露 ACP 夹具错误读取 CLI envelope，已按真实 `result.Brain.output` 解包。
- `2026-11-12T15-25-36Z` 中文和 `15-32-50Z` 英文的空态、提议、采用前未派活、Task report 闸门均通过；两主题采用按钮 hover 变化过弱，模型判定失败。已增强共同组件主题 token，六主题反馈回归先红后绿；`rtk cargo test -p ui` 最终 23 通过。场景新增按下态看图检查。
- 后续失败证据保留：`15-54-40Z` 视觉响应点格式错误（驱动 ENV）；`16-01-22Z` 场景遗漏展开默认折叠的 All files，已补显式前置操作；`16-08-31Z` 全屏 Tasks 标题与红黄绿按钮重叠，已复用顶栏既有安全边距修复。
- 最终 `bash scripts/desktop-e2e.sh staff --visual` 通过 2/2：中文 InkBlack 的 `2026-11-12T16-29-57Z`、英文 SapphireNight 的 `2026-11-12T16-51-41Z`，各 6 个步骤、11 项模型看图检查全部通过。证据位于 `artifacts/desktop-workspace-e2e/`，包含 report.json、原 PNG、匹配 SHA 的逐图观察、Task 快照和 CLI/ACP/Worker 日志。
- 两套场景均确认采用前不执行、Task report 被拒、采用后创建新卡且精确 dispatch 仅送达一次、结果回原任务并推进待验收、右侧读取真实产物、自动完成和新消息重开、手动阶段保护、全屏安全区域/Escape/画布切回且不创建第三张卡。测试仅替换外部模型与 Agent 进程，实际 App、CLI、权限闸门、PTY、hook 和 TaskStore 均走生产路径；不将此证据宣称为真实模型服务验收。

## 交付边界

- 当前 Agent 逐一读取 visual request 与原 PNG 后写观察结果；未预填通过，失败证据保留。
- PR 交付经正常 hook 提交与推送；合并仍须最新提交 CI 全绿，本任务不自动合并。
- M2/M3 仍按既定拆分后置；一任务一会话未获完整验证，M1 使用共享主会话＋Task.messages 重放。

## 2026-11-13 工作空间与执行可见性修正（已完成）

| 用户验收项 | 实现边界 | 新验证 |
| --- | --- | --- |
| 总览也可进入幕僚 | 入口仅依赖已打开工作空间；任务按项目聚合，选择/新任务目标独立于画布展开 | App `staff_workspace_aggregates_late_projects_and_routes_selected_owner`；真实 `staff-workspace-streaming` 从总览点击入口 |
| 项目筛选与切换 | 所有项目/单项目列表；按任务归属发送、定位、跟随文件；切工作空间清空旧选择 | 双项目 E2E 核对 TaskStore project_id、历史和未发送草稿 |
| 等待与流式 | TaskActivity 独立瞬态事件；ACP 原始分片在合并审计前发布；保存完整消息后交接 | `brain --test staff_streaming` 先红后绿；App 轮次/落盘/服务退役回归；E2E 持有外部 ACP 分片 |
| 真实 todo 过程 | 使用 ACP plan 更新替换状态，不编造步骤、不展示内部思考 | E2E 检查1/2→2/2、完成前后无重复气泡、错误停止等待 |
| 原型对照 | 按原型最终覆盖样式铺满顶栏以下、56px标题/34px页脚、灰阶阅读区、紧凑字号与控件 | 中英文模型看图验收，截图、请求、SHA响应和隔离夹具回执全部保留 |

动效采用现有 gpui-component Spinner 表示真实等待，减少动态效果设置下用静态状态图标；输入和切换立即更新，不增加装饰动效或依赖。旧 E2E 通过记录不计入本轮结果。

- 独立对抗式审查发现并修复：总览入口错误依赖当前项目、迟到历史导致 UI 猜测的消息基线吞掉新流、加载中提示被永久保留。基线改由后台当前 Task 快照提供；服务就绪重载，停止或释放时结束瞬态等待。最终增量只读审查未发现新的确定问题。
- 工作空间聚合加载中的项目，发送/采用/定位和右侧文件工具统一跟随当前任务归属；所有项目与单项目筛选不改变画布展开，项目草稿独立保存，切换工作空间清空旧任务选择。
- `rtk cargo test -p brain -p ui --config 'profile.test.opt-level=0'`：168 通过、2 忽略；`rtk cargo test -p app --lib app::staff:: --config 'profile.test.opt-level=0'`：8 通过。最新 app check、debug/release build、资源 clippy、Swift 场景编译与 fmt/i18n（2288 双语键）/行数/allow/生命周期/存储/性能/指南护栏均通过。
- 初次工作空间流式场景 `2026-11-13T07-07-53Z` 通过旧功能判据，但看图发现右抽屉透出紫色壁纸、顶栏右端留白；修复 Staff 抽屉底色与顶栏宽度，并新增 drawer-frame 判据后重新运行完整模块。原始证据保留。
- 最终 `bash scripts/desktop-e2e.sh staff --visual` **4/4 PASS**：共 24 个步骤、72 次视觉检查、168 个定位/检查请求；原 PNG、请求 ID 和响应 SHA 全部一致，所有逐图观察均通过。以下目录均位于 `artifacts/desktop-workspace-e2e/`：

| 场景 | 证据目录 | 步骤 / 视觉检查 / 请求 |
| --- | --- | --- |
| 工作空间与流式，中文 InkBlack | `2026-11-13T07-38-53Z/` | 4 / 12 / 34 |
| 工作空间与流式，英文 SapphireNight | `2026-11-13T08-04-26Z/` | 4 / 12 / 34 |
| 完整派活闭环，中文 InkBlack | `2026-11-13T08-40-33Z/` | 8 / 24 / 50 |
| 完整派活闭环，英文 SapphireNight | `2026-11-13T09-18-10Z/` | 8 / 24 / 50 |

- 新场景确认：所有项目直接进入幕僚、首片前等待、完成前可见部分正文、真实 plan 从 1/2 到 2/2、终态只留一份正文且停止动画、跨项目历史与草稿隔离、文件树与正文跟随项目、失败停止等待且保留既有回复、切工作空间无旧任务泄漏。闭环重验同时覆盖原型结构、搜索/筛选、宽度上限、900/760/600px 布局、真实文件、完成/重开/手动阶段、全屏/Escape/画布。
- 外部 ACP 模型与 Agent 进程使用受控夹具，真实 App、CLI、PTY、hook 与 TaskStore 使用生产路径。此结果不代表真实外部模型服务验收或像素级一致；M2/M3 和共享会话降级边界保持不变。

## 2026-11-13 用户复验修正（早期记录）

- 根因：为用户启动的 release 裸二进制与已运行正式 App 使用同一个本地 socket，workspace route 注册失败；旧 `staff_ready` 又把缺少 route 误判为未打开工作空间。停止本次预览实例并改用开发入口，不结束用户正式 App；已打开项目连接失败改为准确双语提示。
- 回归先红：App 测试通过真实 `register_loaded_project_route` 制造入口不可用；原代码返回 `island.staff_no_workspace`，新期望为 `staff.connection_unavailable`，并断言项目仍打开、未排入无身份消息。
- 原型差距：旧场景只查三栏可见，未验证侧栏密度、Worker 卡条、对话层级、多行输入和分段切换。按原型恢复 330px 侧栏、56px 标题、34px 页脚、搜索/筛选/折叠组、真实 Worker 卡片、右对齐用户气泡、Norma 开放正文、圆角提议与多行输入。仍只实现 M1 数据与操作，不显示 M2/M3 的空按钮。
- 新增验收：Enter 保留换行、Cmd+Enter 发送完整两行；搜索无匹配/Agent/标题、Running/All 筛选保持当前任务；将原型结构写入双语视觉判据。后续逐图读图，不能复用旧样式通过结论。

- 编辑提议追加到现有草稿而不覆盖；输入复用组件的多行模式，Cmd+Enter 在输入动作捕获阶段发送，避免原始 keydown 被组件先消费。采用按钮保留主题前景色，hover/pressed 与禁用态均读图确认。
- 右栏宽度上限为窗口减任务栏与 320px 对话保底；小于 1180px 隐藏右栏，小于 760px 再隐藏左任务栏，恢复窗口时保留抽屉开关和已存宽度，不改画布宽度偏好。
- 新增 4 条 App 回归均先红后绿：连接失败保留已打开项目、宽窗抽屉上限、760/600px 响应布局、编辑提议保留草稿。`rtk cargo test -p app --lib app::staff::layout_tests -- --nocapture` 3 通过；连接失败聚焦测试 1 通过。`rtk cargo test -p ui` 23 通过；最新 app check、debug/release build、`cargo lint-resources` 及 Swift 场景编译通过。i18n 2276 双语键、行数/allow/资源生命周期护栏通过。
- 本轮独立对抗式审查指出并修复最大抽屉宽度挤没对话、760/600px 断点与草稿验收假阳性；最后只读复核确认原问题关闭，限定范围未发现新的确定反例。
- 本轮失败证据保留：`2026-11-13T02-24-59Z` 暴露 Cmd+Enter 原始 keydown 无效，改为组件动作捕获；`02-50-07Z` 视觉响应超时属 ENV，途中发现的采用按钮低对比度已修复；`03-31-04Z` 运行被打断、无最终 report，不计入通过。
- 最新 `bash scripts/desktop-e2e.sh staff --visual` **2/2 通过**：中文 InkBlack `2026-11-13T04-15-20Z`（1354.78 秒），英文 SapphireNight `2026-11-13T04-37-55Z`（3022.44 秒）。每套 8 个步骤、24 次视觉检查（26 条判据）全部通过；每套 50 个视觉定位/检查请求均有匹配原 PNG SHA 的模型观察。
- 新证据补齐 Enter 换行/Cmd+Enter 完整发送、用户气泡与提议层级、真实 Worker 卡条、采用前编辑追加、派发后禁用、非空两行草稿经过搜索/筛选仍保留、右栏拖至上限、900/760/600px 断点及恢复。原有权限闸门、精确且仅一次派发、真实产物、自动 phase/手动保护/完成重开、全屏/Escape/画布保留也均通过。证据目录为 `artifacts/desktop-workspace-e2e/` 下上述时间戳，原型结构和关键状态已逐图复核；仍不宣称像素级一致或真实外部模型服务验收。


## 2026-11-13 Material 双栏与真实派发修复（验收中）

- 用户已从三个可交互原型中选定“双栏工作台”。原型与浏览器验收脚本位于 `docs/prototypes/norma-staff-material/`；3 种布局、14 组行为检查通过，原型不连接真实派发。
- 真实失败根因：本机已安装旧版 `norma-hook` 会转发当前 `control_run_id`，但不认识新增 `dispatch_id`；幕僚过滤因此丢弃 SessionStart，60 秒后报 Worker 未就绪。隔离 HTTP 重放证据在 `artifacts/staff-m1/replay-installed-hook.py`；App 生产 drain 回归先红（Ready 队列为空）后绿。
- 最小修复：字段缺失时仅兼容当前 owned 进程身份；显式旧编号、畸形编号、其他进程、无身份消息仍拒绝。未就绪保留对应原因，不再全部压成一般执行失败。独立审查未发现新增身份绕过。
- UI：单一顶部工作空间/视图导航，280px 任务栏，项目筛选与按需搜索在左侧，去除重复标题/统计页脚/状态组标题；圆角对话区、紧凑 Worker 行、Markdown 正文与输入焦点反馈。600px 支持任务浮层，全屏收起列表，单次 Escape 返回。
- 独立审查发现并修正长工作空间名越界及窄窗浮层残留导致 Escape 被吞；右栏最宽回归先红（对话总宽600px）后绿，现扣除280px任务栏与36px留白后仍保留320px对话。
- 已验证：Runtime 401 通过、11 忽略；App staff 聚焦 15 通过；app check/debug/release build、Swift 场景编译、i18n（2288 双语键）、行数/allow/资源生命周期护栏通过。
- 真实窗口验收尚未完成：`NORMA_E2E_STAFF_HOOK_BINARY=... bash scripts/desktop-e2e.sh staff-workflow --visual` 前置检查屏幕锁定，退出码3、0/1、未发键鼠。已请求用户解锁；此版本不得复用上一版视觉通过结论。最终限定独立复核未发现新阻塞。用户随后明确要求先提交并 push、不要合并：本轮保留验收中状态推送至 PR #235；中文旧 helper 派发闭环与新版视觉检查、英文项目/流式检查仍待补齐。
