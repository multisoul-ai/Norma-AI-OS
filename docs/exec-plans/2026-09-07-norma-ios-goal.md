# Norma iOS · Goal 实时实施计划

更新时间：2026-09-12 · 版本：133（已修复空实时输出持续加载；TestFlight28已发布）

**发布规则（用户最新确认）**：只在用户明确要求发布时上传/分发 TestFlight；完成修改后默认仅做本地构建和验证。

**指挥台替换专题（2026-09-13）**：[实时指挥台 Goal](2026-09-13-norma-ios-realtime-command-goal.md)统一跟踪已选「陪伴」布局、真实角色、语音与 Mac 工具接入。该专题取代本文旧指挥台布局要求；项目、任务、设备等其他模块交互仍沿用现状。新原生能力尚未实现，本文历史验收不能作为其完成证据。

## 当前状态

| 项目 | 当前事实 |
|---|---|
| Goal状态 | 当前工具查询无活动Goal；本计划继续保留原始目标及全部未验收项，未将整体目标标记完成 |
| 产品目标 | iPhone 通过局域网 / Tailscale 操控多台 Mac 上的 Norma，查看任务、对话、处理决策与接收通知 |
| 本轮 Goal | 按本计划推进 G1–G5；iOS 与 Norma 通信保持低延迟与高性能。用户已更新 Goal 文本，既有已确认功能与交互仍保留 |
| 当前阶段 | v133已修复Claude空实时输出持续加载；包含v131/v132任务项目搜索合并及冗余提示精简。TestFlight 0.1.0（28）已VALID / IN_BETA_TESTING并加入现有Norma内测组；桌面最近包仍为v119，未替换运行实例 |
| 当前问题 | 已复现并修复实时通道收到空屏后仍转圈，尚未确认是用户实际Claude卡持续加载的唯一触发路径；历史缺失会有限重试后报错，已有记录读取正常。桌面全量回归仍有3项会话恢复失败（首项fake Claude未按2秒预算启动，另2项锁中毒），本次未修改桌面恢复代码。Claude自绘终端排版及物理iPhone键盘/锁屏仍待验收 |
| 已有证据 | v133原生UI先红后绿，3项聊天回归通过，62项Swift Core及4项聊天网络/回收测试通过；Computer Use实际点击Claude并验证空屏→正文自动更新。v132项目搜索与筛选、v129发送拒绝/未知结果保护、v128键盘恢复证据继续保留 |
| 尚无证据 | 物理iPhone/双Mac/LAN/Tailscale延迟、切网和耗电，完整SwiftUI空闲CPU、真实云模型规划质量及完整CUA。单流隔离测量不能替代这些证据；APNs按用户决定不部署 |
| 下一动作 | 构建28已发布，用户更新后重点复测实际Claude卡片及切网/锁屏；若仍持续加载，需要具体项目、卡片及同卡终端表现。Claude独立终端排版、多机、LAN/Tailscale及云规划验收继续保留，不自动重启桌面 |

产品依据：[功能与方案](../product-specs/2026-09-06-norma-ios-companion.md)；指挥台交互依据：[实时虚拟人原型](../prototypes/norma-realtime.html)。v28 工作台 HTML 已归档：[archive/2026-09-06-norma-ios](../prototypes/archive/2026-09-06-norma-ios/norma-ios.html)。项目、任务等其他模块以原生实现与用户后续明确修正为准。

更新规则：后续每次推进工作时，更新本页阶段状态、最新证据、未解决问题和下一动作。只有验收条件全部满足才勾选完成；尝试过、界面可点击或请求已接收均不等于功能完成。计划变化在末尾追加记录，不以预计日期推算进度。

## 功能实现与验收分开跟踪（v64）

用户最新要求：先完成可独立开发的功能，再统一进行界面、真机、跨网和推送验收；硬件不可用不阻止功能开发。下表描述**原生首版代码状态**，不是G2–G5验收通过；历史段落的“尚未接入”仅代表对应版本当时的状态。

| 功能 | 当前代码状态 | 未完成事项 |
|---|---|---|
| 验证码/扫码、多电脑、独立授权与撤销 | Mac设置、SwiftUI配对、TLS/WSS与Keychain已接入 | 物理双Mac/iPhone、网络切换统一验收 |
| 项目与卡片管理 | 空卡创建、删除确认、真实状态、品牌图标与动效已接入；v64补齐项目/卡片搜索、状态和Agent筛选 | v75补齐默认看板、项目切换、按电脑记忆项目与同项目指挥台入口；原生卡删除已在v72通过Runtime/SQLite E2E与CUA确认页，真实手机删除仍待验收 |
| 终端查看与输入 | v85接入原PTY、120行分页、WSS变更行增量、文字/快捷键与独立历史；原屏幕/输入序号保护双端操作，修复纯PTY输出未及时唤醒订阅 | 真实Codex/Claude经macOS Swift和隔离iOS均通过；构建6已发布。完整Computer Use、真机键盘/手势由用户验收，新能力需要配套Mac包 |
| 聊天、草稿、可靠队列、中断与继续 | v86补齐可展开工具卡片，支持完整输入/输出、状态与大字号；v69 改用原卡同一 PTY 输入和准确会话历史；首条草稿、增量正文、去重和电脑草稿保护已验证；原受控队列能力仍保留 | v74两种原生卡生成停止、准确回执、落盘及原会话续聊通过；Claude原卡Todo/队列通过。v84 Codex原卡Todo入队/撤回/重排/队列在macOS Swift与隔离iOS通过；两种原卡审批中停止通过。真机键盘/长消息交用户验收 |
| 指挥台审批 | 多电脑WSS汇总与原受控请求级审批已实现；v69 原生等待确认显示电脑处理提示，不阻塞整个审批列表 | v77原生Claude Bash长任务/失败与v78 Codex原卡Bash两次决策经macOS Swift/iOS TLS/WSS通过；v84 Codex长任务/exit7与Claude Write拒绝/批准通过；完整或已验证菜单可手机决定，未知/裁切菜单交电脑。完整页面与物理多机竞争交用户验收 |
| 方向、计划与Todo | 正式模型入口、建议存储、确认/拒绝、执行终态和安全重排已接入；v64补齐默认待办/全部记录，v66接入持续WSS同步 | v72指挥台直接呈现最新计划与持续待办进度；v74确认过滤的是运行能力而非原卡模式，Claude原卡Todo/队列已通过。v79跨端最新计划、项目脉络自动化与Computer Use通过；v84 Codex原卡Todo通过。真实云模型规划质量与完整页面由用户发布后验收 |
| 会话记录、变更与成果 | 受控会话归档、完整正文、冻结diff/图片、验收回执和修改意见已接入 | 完整原生页面验收；未登记旧会话不自动补录 |
| 任务提醒 | iOS系统注册、Mac事件投递、私有APNs relay及点击回源代码已接入 | 用户决定暂不部署服务；真实锁屏推送当前不可用，保留延期项 |
| 低延迟通信 | 项目/聊天/审批/计划持续WSS、分页分片、缺口恢复、无正文心跳已实现；v64筛选只使用当前快照，不额外请求网络 | 物理LAN/Tailscale性能与真机空闲耗电实测 |

实现依据：`ios/App/`的页面、`ios/Sources/NormaCore/MobileCommand.swift`的十一种正式命令、`SnapshotSearch.swift`/`PlanTodo.swift`的筛选、`crates/app/src/mobile/`与`services/push/`。不把原型P1的独立语音按钮、Live Activities/小组件或分享页面宣称为已完成；这些未进入后续确认的G1–G5首版实现范围。

v72 原型一致性进展：主导航已恢复项目 / 居中指挥台 / 任务，电脑管理走公共入口；项目中卡片优先两列显示，辅助字号一列。任务页已支持项目、状态、文本筛选和原会话入口。指挥台已用行动计划/待你决策分段，默认直接显示当前方向、工作简报与Agent待办。v75已将项目目录移入搜索切换面板，默认直接进入卡片看板，并将方向入口接到同项目指挥台；v79已补跨端新计划发现与真实项目脉络，完整决策/验收快捷入口和Computer Use继续保留，当前还原未完成。原生卡删除已解除模式限制，原生手机审批仍未完成；v74已验证两种原卡生成停止/续聊，以及Claude原卡Todo/队列。规划和队列使用Runtime导出的手机能力，原卡具备该能力时不要求改变模式。

## 已确认的首版边界

- [x] 手机负责查看、输入、决策和调整方向；代码、凭据、Agent 与执行上下文留在各台 Mac。
- [x] 使用局域网 + Tailscale 验证码或扫码配对，支持多台电脑；首版不自建任务控制中继。
- [x] 关闭 Norma 窗口后，其中的 Agent 不继续运行；手机断线或切换电脑不终止 Mac 任务。
- [x] 指挥台汇总全部获授权电脑的待决策事项；项目、任务页按当前电脑浏览。
- [x] 移动端执行 Agent 覆盖 Codex / Claude / Cursor；Norma 负责总结、规划与协调；OpenCode 后续接入移动端。Cursor 的结构化审批和可靠停止仍以 Provider 能力验证为门槛。
- [x] 首版包含待确认、完成、失败的锁屏通知，使用轻量 APNs 通知服务；通知服务不执行控制指令。
- [x] 纯黑外观、紧凑字号、底部居中指挥台、极简品牌卡片、运行描边与图标动效、正常聊天输入。
- [x] iOS 与 Norma 服务通信以低延迟、高性能为实现要求：实时更新复用持续WSS，分页读取历史；补测响应延迟与空闲开销，局域网/Tailscale实测在统一验收完成，不以本机回环数据代替。
- [x] 项目内先创建卡片，之后才安排任务；不恢复独立发布页面。
- [x] 按用户 v69 最新修正，手机直接向原有 Agent 卡片输入并读取同一会话结果；新卡可启动 CLI 待输入，不自动派任务，旧卡不要求更换运行模式。
- [x] 已将Codex/Claude原生Bash、完整预览Claude Write审批及原卡停止回执接回同一PTY/run/session，并通过真实Provider检查；未知或裁切菜单不生成自动批准选项。

用户已明确指定：手机端使用 SwiftUI 原生客户端；Mac 接入层复用现有应用与任务协调入口，网络和持久化工作后台执行。详细协议选择由 G1 的真实验证结果约束。

## 阶段与依赖

执行顺序：G0 → G1 → G2 → G3 → G4 → G5。G1 未通过前不锁定 Provider 接入方式与工期。

| 阶段 | 状态 | 依赖 | 可验收结果 |
|---|---|---|---|
| G0 范围与实施计划 | 已完成 | — | 记录已确认边界、现有证据和以下各阶段验收条件 |
| G1 Agent 接入验证 | 原生输入/历史、停止/续聊、已识别原生审批、Todo与终端经真实Codex/Claude验证；Cursor 创建、Hook身份、消息队列与本地历史已接入隔离回归，真实Provider验收及停止/审批能力边界保留 | G0 | Codex / Claude 分别跑通真实消息和请求级审批；Cursor 复用已有PTY与官方Hook身份，不伪造未验证能力 |
| G2 最小真机闭环 | 进行中 | G1 | iPhone 配对两台 Mac，经两种网络查看状态、发送指令并收到可信回执 |
| G3 项目、卡片与聊天 | 已接入卡片管理、对话/终端、发送/草稿、中断与可靠队列，代码和自动化通过，真机验收待完成 | G2 | 卡片生命周期、真实消息历史、发送、任务状态和取消形成完整闭环 |
| G4 指挥台与通知 | 审批、计划、Todo终态、安全重排、冻结交付、成果读取/验收/反馈已接入；APNs代码已接入，服务按用户决定不部署 | G3 | 跨电脑决策、计划 / Todo、成果验收、锁屏通知和回源定位可用；通知延期单列 |
| G5 真机验收 | 进行中：双本地服务隔离通过，物理设备与完整界面验收待完成 | G4 | 弱网、权限、多端竞争、生命周期和无障碍验收完成，形成可评审构建 |

### G1 · Agent 接入验证

- [x] 记录 Codex / Claude 实际版本和受支持接口；逐项核验消息读取、消息发送、运行状态、批准与拒绝（独立协议与测试 PTY，正式接入仍见后续项）。
- [x] 在已确认受控方式的 Agent 会话中关联一次真实消息与任务，确认回复完整性、增量事件与历史读取的来源。
- [x] 分别触发真实审批，按具体请求批准 / 拒绝；核验超时、桌面先处理和重复响应。
- [x] 形成接入结论与可重跑证据。若需要改变 Agent 启动方式或迁移现有会话，先记录方案差异并讨论，不将其当作已获确认的实现细节。

当前源码事实：Codex 受控 app-server 与 Claude 双向 Hook 均已接通正式 App / Runtime 入口；完整历史独立分页，不使用原有 1200 字符摘要替代聊天正文。新卡不派任务，旧卡显式重开，原生终端与关闭回收已验证。网络认证和 iPhone 派发属于 G2–G3，尚未完成。

依据：[Hook 接收端](../../crates/app/src/settings/agent_hook_ingress.rs)、[会话摘要](../../crates/agent/src/conversation_tail.rs)、[发送队列](../../crates/app/src/app/agent_runtime/session_reply_queue.rs)。

实时验证结果：[G1 Provider 接入验证](../design-docs/2026-09-07-norma-ios-provider-validation.md)。正式测试使用 Codex 0.153.4 / gpt-5.6-sol 与 Claude Code 2.1.202 / ark-code-latest；批准、拒绝、中断、完整历史、受控重开、多端竞争和外部 Hook 边界已有证据，G1 在 v12 收口。下文保留 v4–v11 的历史推进记录，最新结论见 v12。

已实现的第一段代码：`AgentSpec.mobile_control` 缺省为 false，旧快照不会自动迁移；`Runtime::create_mobile_agent_card` 仅创建 Codex / Claude 元数据，不启动进程。三条旧恢复入口均不以 bypass 模式启动受控卡片，后台桌面历史发现也不能为受控卡片认领会话。创建 / 保存 / 重开 / 删除的 SQLite 闭环已通过。Claude 现已有 App 显式启动入口；Codex 受控启动也已在 v6 接通；手机派发入口仍未完成。

验证：`cargo test -p canvas -p runtime`、修复后的 `cargo test -p runtime`、`cargo check -p app`、`cargo test -p e2e --test mobile_agent_card_persistence` 均通过；存储 / 性能 / 行数 / allow / diff 护栏通过。完整 `cargo test -p e2e` 为 197 通过、5 失败、4 忽略；失败集中在 Chrome auth 与语音用例，独立重跑仍有失败，尚未对未修改版本作基线复验，不声明这些失败已解决或必定为既有问题。此处 SQLite 集成不等同于真实 App / iPhone E2E。

第二段正式代码（v5）：

- `MultisoulApp::start_mobile_agent` → 后台 `MobileAgentLaunch::prepare` → 前台准确附着。Claude 使用 `--permission-mode default`、原生 PTY 和本次进程的 Hook 设置，不修改全局配置、不关闭外部 Hook。独立启动身份防止快照重载后旧回调取消或占用新一轮启动。
- `HookApproval` 绑定 Workspace token、卡片、运行实例、Provider session、Claude `prompt_id` 与一次 Hook 调用。单次决定、超时、断开、关窗和注销撤销均有聚焦证据；标准 HTTP workers 不承担长审批等待。
- 旧 Hook 原行为保留；普通事件先推进回合，再接收审批。旧运行/旧回合事件不能覆盖新状态。接收端观察到 Hook 断开后，尚未提交发送的 allow 被撤销；输出不允许扩大授权或替换工具输入。
- 新 Claude 空会话的 ID 只存在内存，首条真实 UserPromptSubmit 后才写入持久化绑定，避免恢复不存在的历史；关闭仍复用既有 PTY kill / 进程树回收。
- 新增 headless GPUI + **真实 Claude CLI** 的启动检查已通过，验证后台准备、原生 CLI 界面与关闭。它没有发送模型任务，也不等同于真实 `.app`、正式审批执行闭环或 iPhone 验收。

本轮回归：`cargo test -p norma -p runtime` 已通过；`cargo test -p app --lib agent_hook` 的 9 项检查、`cargo check -p app`、SQLite 集成和行数 / allow / 存储 / 性能护栏均通过。并发检查曾复现 HTTP 分包被非阻塞读取误判为坏请求，已在共享入口修复，并通过 10 轮共 30 次传输检查；EOF 测试明确等待接收端观察到 FIN 后再验证撤销。完整 `cargo test -p e2e` 仍为 197 通过、5 失败、5 忽略（新增真实 Claude 检查默认忽略，已单独显式运行通过），失败项与 v4 所列一致；没有未修改 HEAD 基线，不能宣称为既有问题或已解决。`cargo test --workspace` 在 App 单测阶段以 788 通过、1 失败、7 忽略结束：`test_packager_enables_the_production_data_feature_for_the_main_app` 断言 Packager 缺少 `production-data`；未修改 Packager 配置，未做 HEAD 基线复验，不能声明全仓通过。

第三段正式代码（v6）：

- Claude 正式 `App → Runtime → 原生 CLI → PermissionRequest Hook → Runtime 决策` 已跑通两轮精确临时写入：先拒绝（文件不存在），再批准（文件内容准确）；重复响应被拒绝，等待真实 Done。测试使用现有 ark-code-latest 配置，未关闭外部 Hook，37.62 秒通过。
- `norma::codex::CodexConnection` 使用私有 Unix WebSocket；初始化、乱序并发 RPC、完整长消息和原始审批 request ID 已验证。队列有界，事件缺口导致显式断开及在途请求失败；断开后不重放命令。
- `MobileAgentLaunch::with_initial_task` / `MultisoulApp::start_mobile_codex` 接入正式 Codex app-server；真实首条任务产生 rollout 后，由第二客户端确认可恢复，再启动原生 TUI。空白任务不能启动，也不发送假提示。每个 host 的私有目录、PTY 与连接随卡片回收；启动中的取消也传到后台进程，输出持续在后台消费。
- Codex 已读取协议完成状态和完整分页历史，Runtime 与 headless GPUI + 真实 CLI 检查均通过。初始化采用 read-only / untrusted / user；保留用户外部 Hook。CLI 与协议参数并不完全一致：TUI 不接受 `--ask-for-approval untrusted`，已用真实失败修正为只附着已配置的服务端会话。
- 新增失败测试发现关闭执行器后可能仍显示 Thinking，已在受控会话关闭的共享入口改为 Ended；已完成和失败结果保持原状态。Claude 启动实现按职责拆出，避免继续把 Codex 逻辑堆入同一文件。

验证记录：`cargo test -p norma -p runtime`、`cargo check -p app`、行数 / allow / 存储 / 性能 / diff 检查通过；完整 E2E 为 **197 通过、5 失败、7 忽略**，仍是先前记录的 Chrome auth / 语音用例，未做 HEAD 基线。Codex 历史断言进一步限定为 assistant `agentMessage` 的精确文本，两条真实检查均重跑通过（生命周期修复后 Runtime 8.53 秒，App 7.65 秒）；Claude 正式审批检查也已显式通过。`cargo test --workspace` 再次在 Packager 缺少 `production-data` 的断言结束（App 单测 789 通过、1 失败、7 忽略），没有 HEAD 基线。生命周期修复后 Release 增量构建通过（1 分 59 秒），域层与 App check 也通过。这里全部仍属于域层 / headless GPUI + 真实 CLI，不能算作真实 `.app` 或 iPhone 验收。

最后的进程检查发现并修复了退出竞争：原实现仅让后台线程回收 host，测试主进程可能在 SIGSTOP 与 SIGKILL 之间退出，曾留下停止态 Codex 包装进程。已清理明确识别的本次测试进程；现由 `LaunchControl` 持有 host 和原生终端的进程句柄，创建和取消共享所有权锁，关闭返回前完成现有 kill 流程。输出与目录清理仍在后台。新增独立子进程测试在 `kill_all` 后立即 `exit`，确认 host 不存活；同时覆盖关闭后迟到的准备不会再 fork。该机制也用于 Claude 的后台原生终端准备，已重跑两种 CLI 启动 / Codex 真实消息检查；检查没有遗留本次测试的 Codex host。

最新复验：生命周期修复后 Claude 原生启动通过。真实审批曾两次未收到请求，终端证据显示任务已经 Done：模型将测试中的 `NORMA_MOBILE_APPROVED` / `denied.txt` 误判为授权标记，未调用工具。测试改用普通 `hello-from-norma` 文本及 `first.txt` / `second.txt` 临时文件，没有改审批策略、关闭 Hook 或扩大授权；随后批准 / 拒绝两轮和精确磁盘断言在 **24.11 秒**通过。测试也会在 Provider 已结束却没发起审批时立即失败，避免再空等 120 秒。

第四段正式代码（v7）：

- Codex 审批绑定运行 / 线程 / 回合 / 工具项和原始 RPC ID；本地请求 ID 独立递增。错误运行、桌面先处理、重复响应、过期、旧回合以及 ID 复用均不能误授权。请求结束后离开 WaitingApproval；真实新回合可从 Ended 继续，迟到工具事件不能复活任务。
- 新增项目数据库 `approval_audit`。事件按运行与序号追加；相同内容重试幂等、不同内容冲突整批回滚。App 使用已绑定 ProjectStore 的真实路径，不推导默认路径。
- 决定先标记 Chosen，由有界后台 SQLite writer 提交；Runtime 消费成功回执并再次检查请求后才发响应。数据库忙时不提前发送，提交失败关闭受控通道；桌面在落盘期间处理后也不补发。关闭及重启均不会自动重放旧决定。
- Chosen、Submitted、Resolved 分别表示选择、入队、Provider 清理请求。审计同时保存准确的 Provider 响应；Resolved 不披露竞争胜方及最终 choice，不能宣称工具已执行。异常退出前未收齐的最终结果仍须回查 Provider，不能由本地审计推断成功。
- 本机 Codex 已从 0.152.1 升为 **0.153.4**。真实请求只提供 accept / execpolicy amendment / cancel；拒绝按实际选项使用 cancel，表示拒绝并结束回合。绝不把单次批准扩大为 session / execpolicy 权限。正式 host 保留外部 Hook，隔离测试继续使用已有授权。

当前验证：请求状态机、真实 Unix WS + Runtime、SQLite 项目隔离 / 幂等 / 冲突回滚 / 锁库 / 写失败检查通过；App 正式入口接真实 Codex 的无工具消息复验通过（8.75 秒，headless GPUI + 真实 CLI）。新版本拒绝 / 批准 / Provider 中断及磁盘效果在 36.57 秒通过；取消由测试直接调用 Provider RPC，公开手机中断指令仍待接入。完整回归：`cargo test -p norma -p runtime -p storage` 合计 **291 通过、2 忽略**；最后补的无回合身份事件回归也在完整 Runtime 测试中通过，App check 通过。完整 E2E 仍为 **197 通过、5 失败、7 忽略**，失败位于 Chrome auth / 语音用例；`cargo test --workspace` 停在 App 的 Packager `production-data` 断言（789 通过、1 失败、7 忽略）。没有未修改 HEAD 基线，不声明这些失败已解决或与本次无关。行数 / allow / 存储 / 性能 / diff 护栏通过，域层依赖无 GPUI；真实测试未遗留本次 Codex host。最终 Release 增量构建通过（2m 05s）。

第五段正式代码（v8）：

- `Runtime::interrupt_mobile_codex` 绑定节点 / 运行 / 回合，防止重复及旧回合中断。发出后撤销未发送的选择和迟到审批；只有真实 turn/completed 才完成回执。RPC ack 不等于已经停止，自然完成仍返回 completed；RPC 失败关闭控制通道且返回错误。新回合恢复正常审批。
- 正式中断入口替换测试中的直接 Provider RPC，Codex 0.153.4 的拒绝 / 批准 / 中断和审计在 **33.04 秒**通过。锁库期间中断、晚到落盘回执、失败和自然完成的 Unix WS + Runtime 检查也通过。
- `norma::claude::read_transcript` 按文件游标读取完整主会话消息，保留用户 / assistant 内容、工具块和父消息身份；每条记录验证 session / cwd。游标检测文件替换、截断及非行边界；尾部半条 JSON 留待下一次读取，不跳过、不截断正文。单行上限 8 MiB，超过时明确报错；每页扫描最多 16 MiB，避免读取无限增长的文件。
- `Runtime::mobile_claude_history` 只从当前运行已验证的 Hook 取得路径；手机不能传任意文件路径。App 在后台执行 IO，关闭前后均检查运行身份。测试覆盖 5000 字中文、工具块、半行追加、跨项目 / 会话拒绝和关闭后撤销。
- Claude Code 2.1.202 的真实 App → Runtime → CLI → Hook 链路通过两轮拒绝 / 批准及分页历史读取（23.62 秒）；进一步验证第一轮取游标、第二轮增量追加且不重复旧消息，在 **32.36 秒**通过。这是完整消息块 / JSONL 级增量，尚不是逐 token 推流；手机显示层仍待 G3 实现。

当前限制：Claude 统一决策持久化、旧会话显式重开、正式多端竞争和外部 Hook 兼容仍未完成；G2–G5 尚未实现。没有真实 `.app` / iPhone / 双机 / APNs 验收。v8 最终回归：`cargo test -p norma -p runtime` 为 **227 通过、2 忽略、0 失败**，App check 通过（5.53 秒），Release 构建通过（2m 44s）；行数 / no-allow / 存储 / 性能 / diff 护栏通过，域层依赖无 GPUI，未遗留本次 Codex host。完整 E2E 为 **197 通过、5 失败、7 忽略**（两项 Chrome auth、MiniMax manual voice、large voice catalog 与 legacy bilingual install retry，后两项涉及锁 poisoned）；全仓测试停在 App Packager `production-data` 断言（789 通过、1 失败、7 忽略）。没有未修改 HEAD 基线，不声称上述失败与本次无关。

第六段正式代码（v9）：

- Claude 复用项目 `BackgroundApprovalAudit`，后台启动时打开真实项目库；缺少审计库则拒绝受控启动。没有新建另一套存储，也不修改全局 Hook。
- `HookApproval` 增加 Chosen 阶段。选择先持久化；Runtime 消费提交成功回执、再核验请求状态，才转为 Decided 交给 Hook。重复、错误运行、超时、桌面断开、Stop 和关窗均不能复活旧选择。提交失败结束受控运行。
- 审计保留 Pending / Chosen / Decided / Responded / Cancelled 与精确会话、回合、调用及 Provider 输出。Responded 仅表示本地 transport 取出响应，不证明网络送达、竞争胜方或工具执行。异常退出时最后几项后台记录可能尚未提交，未知结果仍须回查真实 Provider。
- 三项 Runtime + 真实 SQLite 检查覆盖锁库、迟到提交、桌面取消、TTL、回合结束后新回合、写失败及关闭。现有生命周期与完整历史回归保留。
- 正式 App / Runtime / Claude Code 2.1.202 / Hook 两轮拒绝和批准，验证精确临时文件结果、每轮四阶段审计及完整增量历史，在 **20.47 秒**通过（此前一轮 21.86 秒）。仍是 headless GPUI + 真实 CLI，不是实际 .app / iPhone 验收。

v9 最终回归：`cargo test -p norma -p runtime` 为 **230 通过、2 忽略、0 失败**；App check 通过（2.90 秒），Release 构建通过（1m 57s）。完整 E2E 为 **197 通过、5 失败、7 忽略**（两项 Chrome auth、MiniMax manual voice 及两项锁 poisoned）；全仓在 App 的 Packager `production-data` 断言失败（App 789 通过、1 失败、7 忽略）。没有未修改 HEAD 基线，不能归因这些失败。行数 / no-allow / 存储 / 性能 / diff 检查通过，域层依赖无 GPUI。G1 尚未完成，G2–G5 未实现。

第七段正式代码（v10）：

- `Runtime::enable_mobile_agent_control` 提供旧卡片显式切换：核对原 session、Provider 与空闲状态；拒绝运行 / 等待审批 / 未确认状态和重复切换。成功后回收旧 PTY，保留原卡 ID、别名、任务名称与历史绑定，不派新任务。已明确退出的旧 PTY 即使缺少最终 Hook，也不会被过时 Thinking 永久挡住；App 层仍保留队列检查。
- `MultisoulApp::reopen_mobile_agent` 同时检查任务协调器、待发送任务、会话回复队列及尚未消费的 Hook。前置失败不关闭卡片、不吞掉队列；切换后即使启动失败也保留受控模式，避免重试退回绕过审批的旧路径。
- Codex 在已有准确会话时支持无提示恢复；新会话仍要求真实首条任务。恢复只执行 thread/resume 与原生 TUI 附着，不发送 turn/start。后台审计、只读沙箱和逐次审批配置仍保留。
- 两种真实 CLI 的 App 重开验收通过：Codex **10.31 秒**，恢复前后完整回合数据一致；Claude 两轮拒绝 / 批准 / 审计 / 历史加重开 **28.54 秒**，控制 run 更新、session 保留、旧游标续读没有新增消息。测试以刚验证的真实历史构造旧卡元数据，不声称启动了用户已有的非受控会话，也不是真实 .app / iPhone 验收。
- 原生 Claude 中断验证取得新证据：当前 2.1.202 的 PermissionRequest 界面使用 Escape。Ctrl-C 未撤销审批；Escape 撤销了 Hook，终端回到输入状态；额外观察 5 秒仍无 Stop / SessionEnd，Runtime 仍为 WaitingApproval。JSONL 追加了带准确 promptId 的 tool_result 与 `[Request interrupted by user for tool use]`。因此 **正式 Claude 中断入口仍未完成**，下一步需结合发送前的历史位置、精确回合身份及发送后的新增中断记录确认终态，避免以按键写入或旧文本冒充完成。

v10 最终回归：`cargo test -p norma -p runtime` 为 **232 通过、2 忽略、0 失败**；包含后补的“PTY 已退出但无 Stop Hook”失败→修复验证。重开时的排队回复 / 启动保护 App 测试通过，最后 App check（2.47 秒）和 Release 构建（1m 44s）通过。完整 E2E 为 **197 通过、5 失败、8 忽略**，失败仍在两项 Chrome auth 与三项语音测试；全仓停在 Packager `production-data` 断言，App 为 790 通过、1 失败、7 忽略。没有未修改 HEAD 基线，不归因这些失败；最后纯 Runtime 退出分支修正后重跑了完整领域测试、App check 与 Release。行数 / no-allow / 存储 / 性能 / diff 护栏通过，域层依赖无 GPUI。G1 继续进行，G2–G5 未实现。

第八段正式代码（v11）：

- 新增 `MultisoulApp::interrupt_mobile_claude` 与 Runtime 正式入口。请求绑定卡片 / run / prompt，先撤销尚未发送的审批；后台捕获完整 JSONL 末尾，前台复核身份后只发送一次 Escape，后台读取新增记录，最后再次核对回合才返回 `interrupted` 并更新 Ended。
- 旧记录、其他回合和用户输入的同名字面文本不能作为中断证据；发送前遇到半条记录明确失败并要求重试，发送后的半条记录保留游标等追加。文件替换、截断、读取失败、换回合、超时和关窗不能伪造成功。自然 Stop 返回 `completed`，StopFailure 返回 `failed`；丢弃后台请求不会永久占用中断入口。
- 保留原生终端，文件读取和轮询均在 GPUI 后台。App 在发送与接受结果前检查待同步 Hook，让常规帧循环先处理已到达的新回合事件及其协调副作用。
- 真实 Claude Code 2.1.202 / ark-code-latest：等待审批时的正式中断在 **9.87 秒**通过；生成中与已执行工具的两项中断合计 **30.57 秒**通过。后者先批准精确临时命令、观察 started 文件，然后中断，继续观察 6 秒确认原定 sleep(5s) 后的 output 未写出。三种场景均返回准确 prompt 的 `interrupted` 与 Ended；仍为 headless GPUI + 真实 CLI，不是实际 .app / iPhone。
- 已验证的限制：在真实回复出现前发 Escape（分别在 UserPromptSubmit 后 1s / 5s）会退回输入框，但没有 Stop Hook 或合格的新增中断记录。正式入口在 30s 后返回“结果未知”，保留最后已知回合状态，不伪造 Ended。生成测试因此以实际回复首行作为开始证据；此限制须进入 G3 的命令回执展示，不能把未知状态显示为已停止。

本轮领域回归：`cargo test -p norma -p runtime` **236 通过、2 忽略、0 失败**；新增 transcript 边界检查与三项 Runtime 竞争 / 失败检查均有失败→实现→通过记录。App check 通过（4.30 秒），行数 / no-allow / 存储 / 性能 / diff 护栏通过。最终 Release 构建通过（1m 55s）。完整 E2E 为 **197 通过、5 失败、10 忽略**，失败在两项 Chrome auth 与三项语音用例；全仓停在 Packager `production-data` 断言（App 790 通过、1 失败、7 忽略）。没有未修改 HEAD 基线，不能把这些失败归因为既有或与本次无关。G1 仍需正式多端竞争与外部 Hook 兼容证据；G2–G5 未实现。

第九段正式代码与 G1 结论（v12）：

- Codex 通过 `hook/started` / `hook/completed` 追踪当前线程、当前回合的同步 PermissionRequest Hook，保留来源类型与文件，不暴露 Hook 命令或输出。外部等待不生成可批准请求；只有 Provider 真实 requestApproval 才进入手机决策 API。完成、换回合与关闭清空旧等待，异步和旧回合事件不混入当前状态。
- 保留本机全部外部 Hook 的正式 `prepare → attach → Runtime` 检查在 **16.15 秒**通过：观察到用户 Hook 的真实等待来源，尝试以 Hook ID 批准被拒绝，正式中断得到准确 `interrupted`，目标文件不存在。未关闭或修改外部 Hook。当前配置含 7200 秒的 Vibe Island Hook；本次证据定位到全局 Hook 文件，不把通知 ID 当成插件胜方的证明。
- Codex 正式 Runtime / 项目审计与另一独立协议客户端竞争通过：另一端先拒绝，或手机已选择 Allow 但 SQLite 提交被延迟时另一端先拒绝，均不补发手机批准、不产生目标文件；保留 Chosen 意图与真实 resolved，不能把 resolved 当作已执行。连同已有拒绝 / 批准 / 中断三回合，五回合检查 **53.09 秒**通过，原生 TUI 保留。仅该隔离审批 host 按既有授权关闭外部 Hook。
- Claude 的正式 App / Runtime / Hook / 原生终端竞争在 **7.07 秒**通过：桌面 Escape 撤销具体 Hook 后，手机迟到 Allow 返回 false，没有响应和磁盘写入；用户外部 Hook 保留。锁库、TTL、晚到提交与关闭仍由 v9–v11 的聚焦状态机及传输检查覆盖。
- G1 退出条件已有证据：两种 Provider 的准确会话消息、完整历史来源、增量读取、真实审批与竞争、旧卡显式重开、原生终端和关闭回收。接入方案已由用户确认。**G1 完成验证，进入 G2；不代表 iPhone / 实际 .app / 双机网络已验收。**

已验证的能力边界继续保留：Codex 外部 Hook 有优先处理权，手机仅能处理实际交给客户端的请求；Claude 生成前中断可能返回结果未知，桌面原生中断后的历史同步将在 G3 接齐；Claude 目前为完整 JSONL 消息块增量，非逐 token 推流。上述差异需要在手机能力与回执中明确展示。

### G2 · 最小真机闭环

G2 首段代码（v12）：

- 新增 `MobileAccessStore`，复用 AppStore / SQLite 与既有 token 哈希比较；设备凭据只保存 SHA-256，设备信息与调试对象不携带凭据。各手机有独立的只读 / 可控制项目范围；首次注册不会覆盖已有设备身份。
- 权限更新使用预期 revision，降权和撤销推进版本；重复撤销幂等，撤销后不能通过旧 token 或权限更新复活。空项目范围可以登记空 Mac，但不能读取任何项目。授权对象是快照，后续派发仍须复核当前 revision，不能当长期凭证。
- 两项真实 SQLite 检查通过，覆盖另一设备 / 另一 App 数据库隔离、错误凭据、精确项目范围、旧版本拒绝、重开后撤销、坏 JSON 权限和无部分写入。仅存储层已落地；尚未开放网络端口，TLS / 配对协议 / Keychain / SwiftUI 仍待接入。

v12 回归：`cargo test -p norma -p runtime -p storage` **307 通过、3 忽略、0 失败**；App check 通过（5.86 秒）。完整 E2E **197 通过、5 失败、11 忽略**；全仓停在 App 的 Packager `production-data` 断言（790 通过、1 失败、7 忽略）。没有未修改 HEAD 基线，不能声称这些失败与本次无关。Release 构建通过（1m 57s）；行数 / no-allow / 存储 / 性能 / diff 护栏全部通过。



G2 第二段代码（v13）：

- `norma::mobile` 实现 TLS 1.3 身份与 WSS 握手；AppStore 保留稳定 Keychain account，证书和私钥保存在本机 Keychain。并发初始化不覆盖既有身份，损坏记录明确失败。真实隔离 Keychain 保存 / 重开 / 损坏拒绝已通过。
- 一次性 256 bit 二维码凭据和 120 秒单调时钟期限；扫码只产生待核对申请，Mac 明确选择真实稳定项目及只读 / 控制范围后才登记设备。写失败、错误申请号、过期与取消不产生有效授权；短连接中断后可用手机预存独立凭据认证查询。
- `MobileServer` 提供显式私网 / 回环 WSS 监听，最多八条连接，拒绝非 TLS、错误路径、query 和浏览器 Origin。网络只暴露 pair / authenticate，不暴露本地批准、撤销或原始 IPC。权限更新或撤销断开目标手机连接；Drop 关闭已接受连接。
- 真实回环 WSS → 扫码 → 本地项目授权 → 手机认证 → 撤销 → 关闭已通过；另有 SQLite 项目 / 电脑隔离与过期、取消检查。发现并修复 macOS accept 继承非阻塞标志导致握手失败的问题，聚焦回归已通过。
- 尚未启用默认监听，没有修改实际 Settings 页面；网络 owner 还未挂到窗口生命周期。SwiftUI、真实扫码、任务网络派发、命令持久化回执和双机 / 真机仍未完成，不能勾选 G2。

协议与接入说明：[连接与配对](../design-docs/2026-09-07-norma-ios-connection.md)。v13 最终回归：`cargo test -p norma -p storage` **180 通过、1 忽略、0 失败**；该忽略项为已单独执行通过的真实隔离 Keychain 检查。三个新 WSS / SQLite 集成用例在最终依赖配置下再次通过，包含未认证 Ping 不续期的 10 秒真实网络检查。App check 通过（7.51 秒），Release 通过（2m 19s），行数 / no-allow / 存储 / 性能 / diff 护栏通过。

完整 E2E **191 通过、11 失败、11 忽略**：语音显式激活断言失败、两项 Chrome auth 失败、八项语音模型锁 poisoned。首个语音激活用例在同一构建的隔离运行中通过（0.45 秒），不能把该结果写成全量已通过。全仓仍停在 App 的 Packager `production-data` 断言（790 通过、1 失败、7 忽略）。没有未修改 HEAD 基线，不归因这些失败。完整记录见 [G2 连接验证证据](../design-docs/2026-09-07-norma-ios-connection-evidence.json)。

G2 第三段代码（v14，用户明确要求 SwiftUI）：

- 建立 `ios/Norma.xcodeproj` / `project.yml` 和本地 `NormaCore` Swift Package，最低 iOS 17，无 WebView、无第三方 Swift 依赖。原生黑色 TabView / NavigationStack / Sheet，指挥台居中；采用系统语义字号、标准触摸反馈与可缩放布局落实 Apple Design。项目和指挥台尚无数据接入，明确显示未同步状态。
- 原生多电脑列表、切换、忘记本地记录、扫码指南、电脑核对与等待授权页面；VisionKit 负责相机捕获、QR 识别和可访问性引导，用户点击后才请求权限。模拟器不伪造相机可用。退出配对或进入后台取消查询，旧电脑回执不能覆盖当前选择。
- SwiftUI 之外的 Keychain actor 保存每台电脑独立的 UUID / 256 bit token，禁止 iCloud 同步；电脑名称、pin 和地址通过原子文件保存，不存 QR secret/token。损坏元数据不会静默清空覆盖；QR 的 Decodable 入口也验证电脑身份，防止绕过工厂校验。
- 原生 URLSession 使用临时会话和 TLS 1.3，验证准确叶证书 pin、有效期、签名及 server usage；仅把该证书设为可信 anchor，拒绝 redirect，没有全局 ATS 例外。只在发送应用请求前尝试备用地址；配对响应丢失后回查授权，不自动重放。六位核对码在 Swift 独立计算，与 Rust/Python 向量一致。授权结果带核验时间，不能当持续在线状态。
- Swift Package 的四项原生检查通过；同一配对 / 授权 / 撤销及错误 pin 检查分别在 macOS Foundation 和实际 iOS simulator XCTest host 中通过，使用真实 Rust WSS 与临时 SQLite，不是 mock。iOS 曾因关闭签名出现 Keychain `-34018`，改用 Xcode 默认本地签名后通过；没有削弱 Keychain 属性或改全局安全配置。构建产物检查发现并修正 XcodeGen 默认 iPad device family 和缺少版本号。

验证范围：模拟器 UI 自动化覆盖添加电脑、扫码指南、相机不可用反馈和取消；Computer Use 实际点击打开配对指南并检查黑色页面，后续操作遇到 `noWindowsAvailable`，未把截图或 XCTest 替代成完整 Computer Use 验收。相机真机扫描、两台 Mac / Tailscale、桌面 Settings / 窗口生命周期、任务数据仍未完成。iOS 模拟器与未签名设备构建均通过；最终计数和日志见 [连接证据](../design-docs/2026-09-07-norma-ios-connection-evidence.json)，开发方式见 [iOS README](../../ios/README.md)。

本轮完整 Rust E2E 的 lib 部分为 **197 通过、5 失败、11 忽略**（两项 Chrome auth、MiniMax manual voice、large voice catalog、legacy bilingual install retry）；该命令在 lib 失败后未继续集成目标，新 Swift 集成目标已单独通过。没有未修改 HEAD 基线，不归因这五项失败。本轮生产 Rust 代码未修改，不重跑 v13 已完成的全 workspace / Mac Release；保留其原始结果，G2–G5 继续执行。

G2 第四段代码（v15）：

- Mac 首页「连接手机」和 Settings → 手机连接已接入：显式启停、系统 QR、核对码、默认不选项目、只读 / 控制授权、权限编辑 / 取消及撤销。复用 gpui-component 与 Core Image；项目读取、Keychain、网络和持久化均在后台，二维码有效期内才轮询申请。
- 同数据库窗口共享一个 `MobileController`，全局只保留弱引用；最后窗口释放立即停止 socket，迟到启动不能重新开启连接。Native `getifaddrs` 发现私网 / Tailscale 地址，固定 9443 端口；不新增 daemon。共享 owner、真实网络关闭与系统 QR → Vision 解码检查通过。
- SwiftUI 重扫已配对电脑时，先沿新地址认证原凭据，保留设备 ID、权限与版本，不再次注册。通过真实失败复现了已使用 QR 导致重复配对报错，修复后 macOS Swift 与 iOS simulator 分别通过两个实际监听地址的重扫检查（0.52 秒 / 16.14 秒）；同时覆盖错误 pin、首次配对、只读授权和撤销。地址元数据在收到可信响应后才更新，丢失响应仍可回查授权。
- 权限编辑取消会丢弃草稿、刷新授权版本，不把未保存的 control 写回数据库；失败→通过的单测验证外部版本从 1 更新到 2 后可重新编辑。三项 desktop core 检查通过；两项 Settings / Launchpad headless E2E 通过。
- 独立真实 `.app` 已启动，截图可见首页入口；Computer Use 关闭按钮使验证进程退出，生产 Norma 保持运行。坐标点击在 Mac 与 Simulator 均持续报 `noWindowsAvailable`，未完成配对页面、启停和权限选择的真实操作验收，不能用 headless / XCTest 替代。临时验证 App / 数据已清理，未创建其 Keychain 身份；可用 `scripts/run-norma-mobile-desktop-l3.sh` 重跑。

v15 验证：Swift package **4 通过**；iOS Xcode **6 通过、1 显式跳过**（无 fixture 网络项，已另行通过）；Storage **72 通过**；App check 通过。完整 E2E 最终 **199 通过、5 失败、11 忽略**，失败为两项 Chrome auth、MiniMax manual voice、large voice catalog 和 legacy bilingual retry；此前本轮全量曾为 193 / 11 / 11，两次结果均保留。本轮此前的全仓检查停在 App Packager `production-data` 断言（792 / 1 / 7），未做 HEAD 基线，不归因、也不宣称全量通过。最终 Mac Release 通过（5m 32s），行数 / no-allow / 存储 / 性能 / diff 护栏通过；完整结果见 [连接证据](../design-docs/2026-09-07-norma-ios-connection-evidence.json)。G2 仍未收口，G3–G5 保持完整范围。

G2 第五段代码（v16）：

- SwiftUI 项目页读取当前电脑获授权目录；进入项目显示真实 Codex / Claude 等 Agent 元数据、最新状态和卡片数量。复用 Runtime / 已有窗口路由，未创建另一套注册表；不启动 Provider，不读 PTY / 文件，不把关闭项目伪装为空项目。跨 App 数据库读取被拒绝，改名 / 删卡 / 关闭即时反映在下一次快照。
- WSS snapshot 每次核验机器 / 设备 / 项目；后台完成权限和 SQLite 读取，再请求前台内存快照。等待前台时释放授权锁，回复前再次认证和比较 revision；途中撤销 / 降权不会返回旧数据。队列 8、前台期限 2 秒、返回体上限 500 KiB，超限明确报错，目前没有分页。
- 原生纯黑极简卡片、独立品牌资源、运行图标 / 环形描边动效已接入；Reduce Motion 静态呈现，Dynamic Type 大字号单列。只对可见项目每两秒读取完整快照，错误即停并清空、离开 / 后台取消，切机器重建导航；仍未实现增量 cursor、缺口恢复和持久化命令回执，不能勾选对应 G2 完成项。
- 失败→通过证据覆盖未授权 snapshot、新 Swift 数据结构与校验、真实 Rust WSS 原生读取。真实网络撤销竞争通过；WSS → 正式 GPUI 窗口路由 / Runtime 及另一安装隔离检查通过（0.24 秒）。macOS Swift → Rust 在 0.33 秒通过；实际 iOS simulator XCTest host → Rust 在 13.30 秒通过。后者的 route 为隔离 fixture，正式路由另有 headless GPUI 检查，均不冒充真实 iPhone / 双机。
- 普通 iOS Xcode 测试最终 **8 通过、1 显式跳过**（无网络 fixture，已单独执行通过），Swift package **5 通过**；Norma / Runtime **241 通过、4 忽略**，App check 和 Mac Release（3m 49s）通过。Computer Use 本轮返回 Mac 锁定、自动解锁失败，已请求用户手动解锁；没有卡片页面的实际操作 / 截图验收证据。

完整 E2E 本轮 **200 通过、5 失败、11 忽略**，仍有两项 Chrome auth 和三项语音失败，无 HEAD 基线。全仓首次停在 App Packager 断言（793 / 1 / 7）；已定位配置带有 production-data，但测试精确字符串不兼容 --no-default-features。仅归一化测试输入里的这个可选参数，未修改构建配置或削弱 feature 要求；聚焦检查已通过；修复后全仓 App 单测 **794 通过、0 失败、7 忽略**，随后停在 E2E（194 通过、11 失败、11 忽略：语音显式激活、两项 Chrome auth、八项语音锁 poisoned），没有宣称全仓通过。行数 / no-allow / 存储 / 性能 / diff 护栏通过。完整结果见 [连接证据](../design-docs/2026-09-07-norma-ios-connection-evidence.json)。G2–G5 继续保持完整范围。

G2 第六段代码（v17，同时接入 G3 空卡创建）：

- 新建手机命令账本，设备 / 操作 ID 唯一，绑定项目、窗口、服务启动实例与完整规范化请求指纹。先落盘预留执行权，同 ID 同参数只返回已有回执，改参数冲突；重开服务时未完成记录显示结果未知，不自动重放。结果终态不可被另一结果覆盖，设备 / 项目 / 安装之间不能串线。两项真实 SQLite 并发、重启和失败测试通过。
- WSS 增加 command / receipt，当前只有 create_card（Codex / Claude）；每次提交核验控制权限和 grant revision，回查核验当前读取权限。前台执行复用现有准确窗口路由 / 布局和 Runtime 元数据创建，不启动 Provider。撤销和 shutdown 与一次性执行权竞争，尚未开始的排队请求失效；已开始的操作保留真实历史结果。全局最多八条未完成处理，过期明确拒绝或结果未知。
- 创建调用现有画布持久化调度器，等待 Clean 后才提交 succeeded 回执，没有另起并发快照 writer，也未在新移动入口同步写 SQLite。若窗口消失、保存失败或执行后回执落盘失败，保留结果未知，不能伪造成功或再执行一次；回执写失败从永久 accepted 修为可查询的 outcome_unknown，并取得失败→通过证据。
- SwiftUI 增加原生创建 Sheet、Agent / 名称选择、只读 disabled、结果查询与未决记录入口；卡片创建不派任务。发送前原子保存机器 / 设备 / 项目 / 窗口 / operation ID，文件不含 token；重新打开只查询，不重发。用户可在电脑核对后显式清除未决记录。离开 / 后台取消手机查询，保留待核实记录。Swift 校验回执的精确身份、成功节点与状态，Unicode 标题边界与 Rust 一致。
- 真实 WSS → 正式 GPUI 路由 → Runtime → SQLite → 成功回执通过（0.42 秒）；四项手机 headless 检查通过，另七项真实 Provider 检查本轮未重跑。两项真实网络撤销 / shutdown / 重试 / 回执写失败检查通过（0.06 秒）。Swift macOS 原生经真实 Rust 创建、丢弃首次回执、重读本地记录、换另一监听地址查回相同卡片并拒绝参数冲突，在 0.82 秒通过；实际 iOS simulator XCTest host 的相同检查在 12.60 秒通过。跨语言测试使用临时数据库和单线程 Runtime fixture，正式 GPUI 路由另外验证，不等于真实 iPhone / 双 Mac。

v17 普通 Xcode 检查 **10 通过、1 显式跳过**（无 fixture 网络项另行通过）；Swift package **7 通过**。Storage 全量 74 项通过；行数检查发现旧 server.rs 新增一个模块声明后达 501 行，已将输入 / 聚焦执行函数移入现有 execution.rs，8 项聚焦回归、最终 App check、Release（1m 56s）和全部护栏通过，复验见 [连接证据](../design-docs/2026-09-07-norma-ios-connection-evidence.json)。本轮 workspace App 单测 **794 通过、0 失败、7 忽略**，随后停在 E2E **201 通过、5 失败、11 忽略**（两项 Chrome auth、三项语音），没有 HEAD 基线。Computer Use 仍返回 Mac 锁定；没有创建 Sheet 的实际操作证据。删除、聊天、任务派发、增量同步、指挥台 / APNs 与真机仍未完成，Goal 保持 G2–G5 完整范围。

G2 第七段代码（v18，同时推进 G3 删除）：

- SwiftUI 项目卡片菜单可删除受控卡；系统 Form 明确确认正在运行任务的「停止并删除」，操作结果页复用创建回执，发送前仍先保存未决记录。保留黑色、紧凑语义字体、44pt 菜单目标与 Reduce Motion。
- Runtime 快照新增真实 turn_id / preparing；删除对照完整快照，旧回合 / 内容变化拒绝，忙碌时需准确运行、会话、回合与确认。未同步、启动准备、排队 / 协调器任务暂不关闭，非受控旧卡仍需电脑显式重开。
- 复用关闭 / PTY 回收 / 审批撤销 / 布局 / 保存，删除成功晚于持久化。已有 quick definition、任务历史摘要和 Provider 对话文件保留；手机归档列表尚未完成。
- 修复同 ID 重复 submit 在终态写失败后把 outcome_unknown 退回 accepted；已有失败→修复测试，重复创建 / 删除均不重放。
- Swift package 8 项、Xcode 11 通过 / 1 显式跳过；原生 macOS Swift / iOS simulator 到 Rust WSS 的创建、删除、换地址回执和保留其他卡片分别在 1.00 秒 / 12.90 秒通过。正式 GPUI 路由的创建 / 删除 / SQLite 与历史保留通过；忙碌确认 / 回合失效 / 终端和审批关闭是 Runtime echo 测试，不是实际 Provider 手机删除验收。

v18 最终回归：Norma / Runtime **243 通过、4 忽略、0 失败**；App check（4.45 秒）、Release（2m 01s）、行数 / no-allow / 存储 / 性能 / diff 护栏通过。最终正式 GPUI 创建 / 删除检查（含只读删除拒绝、历史保留）0.69 秒通过。全仓 App **794 通过、0 失败、7 忽略**，随后停在 E2E **201 通过、5 失败、11 忽略**，失败涉及两项 Chrome auth 和三项语音；无 HEAD 基线，不宣称全仓通过。

本轮 Computer Use 仍返回 Mac 锁定，未取得创建 / 删除页实际操作证据。完整回归见连接证据；聊天、任务网络派发、增量恢复、指挥台 / APNs 与真机继续保持未完成，Goal 不缩减范围。

G2 第八段代码（v19，同时推进 G3 对话读取）：

- 新增 `conversation` 只读网络动作，绑定项目 / 窗口 / 卡片 / 运行 / 会话；后台读取前后复核授权版本和准确窗口。读取期间撤销、跨项目或运行已更换时，拒绝返回正文。路径只来自 Runtime 已验证 Provider 会话，手机不能提供文件路径。
- Codex 复用 `thread/turns/list` 完整回合页，倒序回合转为页内正序；Claude 复用真实 JSONL 游标，完整行向后追加、半行不推进。长正文、稳定消息 ID 和完整工具 / 附件内容保留；拒绝摘要页、重复 ID 和非法角色。Codex 跨页无交集时重建最近页，避免拼出假连续历史。
- SwiftUI 卡片进入黑色紧凑对话页，正文可选择复制，工具 / 附件原生 DisclosureGroup 展开；支持 Codex 加载更早内容、两秒刷新及 Claude 连续读取后续页。后台 / 离开停止；权限失效清空正文。使用系统语义字体和 44pt 分页按钮，落实 Apple Design 内容优先和可访问性。
- 两种真实 Provider 都已跑通原生 macOS Swift 和实际 iOS simulator XCTest host → WSS → 正式 headless GPUI 路由 → 真实会话历史。Claude 分别为 8.44 秒 / 10.30 秒，Codex 为 9.25 秒 / 16.27 秒，均验证完整固定回复和旧 run 拒绝。正式 Codex WSS 用例另外覆盖错 session 与撤销。这里使用实际 Provider、临时数据库和只读测试配对，不是物理 iPhone / 双 Mac / Tailscale，也不替代页面操作验收。
- 当前对话响应仍限 500 KiB，超限明确失败且不截断；缺少字节分页。一次导航绑定一个运行，空卡需在电脑开始会话后重新打开。**输入框、手机发送、可靠排队、草稿与中断尚未接入**，不以只读页面声称正常聊天已完成。项目仍使用完整快照轮询，增量恢复、指挥台 / APNs 与真机范围不缩减。

v19 最终回归：Norma / Runtime **245 通过、4 忽略、0 失败**，Swift package **9 通过**，Xcode **12 通过、1 显式跳过**（真实 Provider 网络 fixture 已另行执行通过）。App check（4.62 秒）、Release（2m 01s）、行数 / no-allow / 指南行数 / 存储 / 性能护栏通过。全仓 App **794 通过、0 失败、7 忽略**，随后停在 E2E **201 通过、5 失败、12 忽略**；失败为两项 Chrome auth、MiniMax 手动语音和两项语音锁 poisoned，无 HEAD 基线，不宣称全仓通过。Goal 继续 G2–G5，下一步接入输入 / 发送 / 可靠排队。

v19 验证和回归详见连接证据。首次 Claude 原生测试因测试 PATH 缺少 Hook 失败，补齐 PATH 并检查启动回执后通过；首次 iOS 新测试文件未加入工程，重新生成 Xcode 工程后通过。Computer Use 最新仍返回 Mac 锁定，未取得对话页实际操作证据；已请求用户解锁，不重复请求。

G2 第九段代码（v20，同时推进 G3 消息发送）：

- SwiftUI 对话页加入系统多行输入框、44pt 发送按钮与键盘避让；输入区固定在底部安全区，保留黑色和紧凑语义字体。空卡可输入真实任务后启动 Agent，后续消息沿用准确运行 / 会话；已授权 Ready 终端即使历史文件尚未创建也能发送首条消息。只读 / 不可达项目禁用发送，待核实消息可查询原回执。
- `send_message` 复用唯一 operation ID、项目控制权限 / grant revision、准确窗口路由和持久化回执。发送前保存完整原文和卡片快照，正文最多 32 KiB UTF-8，仅允许换行 / Tab 等文本输入，不允许终端控制字符。未决文件写入 / 读取同限 256 KiB，iOS 使用完整文件保护，macOS 测试使用 0600 文件；不保存 token。
- Runtime 校验当前卡片快照、run / session / 状态；正在执行、准备、已知桌面未提交输入及协调器 / 消息队列忙碌都拒绝普通发送。发送期间隔离电脑键入，防止两端文本拼接；迟到 Hook 不得清除 Enter 之后的新草稿。未知结果保留防重放状态。
- Codex 冷启动沿用真实首条任务和原生 TUI；续发使用正式 `turn/start` RPC，必须得到准确回合回执。发现真实冷启动历史文件短暂为空导致 TUI 附着失败：新增失败→修复测试，只对当前新线程的空 rollout 使用原有 10 秒有界等待，不重发任务，也不吞旧会话或权限错误。
- Claude 保留原生 PTY，沿用 100ms paste / Enter 间隔；只有准确 run / session 的 `UserPromptSubmit` 同时包含已核验正文和真实 prompt ID，才返回成功。PTY 写入、Enter、服务器入队均不冒充 Provider 接收。结果未知不自动重试；桌面输入缓冲目前保守追踪，导航键也会暂时阻止手机发送，直到 Provider 确认一次桌面提交。
- 两种真实 Provider 均通过 macOS Swift 和实际 iOS simulator XCTest host → 正式 WSS / headless GPUI / Runtime：从空卡发首条、读真实回复、同会话发第二条、确认不同回合、重复 submit 不新增消息、旧空卡快照拒绝。macOS Claude 9.56 秒，Codex 15.29 秒；两种 iOS 检查共 34.65 秒。实际历史仅两条用户消息，原生终端仍在，关闭后运行身份失效。
- 多行边界补验：Ready Hook 可能先于 PTY 模式输出到达。未启用 bracketed paste 时，LF / Tab 在任何写入前明确拒绝；冷启动在现有 25 秒预算内等待，不提前提交。真实 Claude 2.1.202 会把每个 Tab 转为 4 个空格；已用正文首尾空白、连续 Tab 和换行验证，除该原生转换外必须逐字一致，输入区也提示转换。原始请求仍完整保存，不能将其他正文变化当作成功。两种 macOS 原生链路的增强检查在 32.38 秒通过，两种 iOS simulator 链路在 43.03 秒通过。

v20 最终回归：Norma / Runtime / Terminal **296 通过、6 忽略、0 失败**；正式命令 / 对话网络检查 **3 通过、1 显式忽略**；Swift **10 通过**；Xcode **13 通过、1 显式跳过**。两种实际 iOS simulator 多行链路在 **43.03 秒**通过，正文包含首尾空白、换行与连续 Tab，准确回执和去重均通过；当前 Provider 为 Claude **2.1.202** / Codex **0.153.4**。App check（2.94 秒）、Release（2m 41s）与行数 / no-allow / 指南 / 存储 / 性能 / diff 护栏通过。

全仓 App **794 通过、0 失败、7 忽略**，随后停在 E2E **195 通过、11 失败、14 忽略**：两项 Chrome auth、一项语音显式 Apply 断言与八项 poisoned voice lock。新增的语音 Apply 失败单独复跑 **0.25 秒通过**；没有未修改 HEAD 基线，也未修复这些全仓失败，因此不宣称全仓通过。最后代码仅修正注释中的实际 Claude 版本，没有功能变化。

**尚未完成**：忙碌时可靠排队、离开页面仍保留未提交草稿、手机中断 / 审批、项目增量同步、指挥台 / Todo / APNs，以及真实双 Mac / iPhone / Tailscale。当前正在编辑的文本仍仅存在于页面，按下发送后才持久化；不能把它称为完整草稿功能。Computer Use 本轮再次返回 Mac 锁定，未取得输入框的真实操作证据，不重复请求解锁。G2–G5 保持原范围。

G3 草稿实现（v21）：

- 新增 App 共享草稿库，按电脑证书身份 / 项目 / 窗口 / 卡片 / 运行 / 会话隔离。完整原文在编辑时异步落盘，离开页面不取消保存；同文重写也产生新版本。一次只写一个快照，写入期间的新编辑合并为下一份最新快照，避免每个按键堆积文件任务。
- 输入框自动恢复当前准确范围的草稿；冷启动或桌面重开造成运行变化时，旧草稿不自动带入新会话。对话中可进入历史草稿，电脑页提供全局「本地草稿」，保存来源名称和编辑时间，可复制完整正文或明确删除。项目关闭、卡片删除、忘记电脑后，用户自己写的草稿仍可从本地入口找回。
- 发送前等待草稿落盘，并把其版本写进手机未决记录；版本不进入服务器命令协议。只有准确成功回执能清除对应版本，旧回执 / 结果未知 / 其他电脑草稿不受影响。清除失败时，重查不能因内存已移除就假装磁盘保存成功。
- 草稿每条最多 64 KiB（发送上限的两倍），总文件最多 4 MiB / 128 份；不自动淘汰或截断。超限 / 写失败时保留内存新内容和上次磁盘内容，界面显示未保存并允许重试；损坏库不静默覆盖。iOS 使用完整文件保护，macOS 测试使用 0600；进入后台申请系统提供的短暂执行时间完成待写入内容，锁屏 / 硬杀的真机行为仍需 G5 验收。
- Apple Design 落实为系统 TextField / List / Sheet、紧凑语义字体、44pt 操作和即时保存状态；删除才确认，复制不自动发送。Swift 单测 13 项通过；两种真实 Provider 的 macOS Swift 草稿恢复 / 发送 / 清除链路在 22.50 秒通过，最终 iOS simulator 同链路在 33.45 秒通过。

v21 最终验证：Swift **13 通过**；Xcode **16 通过、2 明确跳过、0 失败**，含本地草稿入口 / 关闭导航。两项跳过分别是普通运行未提供网络 fixture、物理设备文件保护；两种真实 Provider 的最终 iOS 草稿链路已另行在 **33.45 秒**通过。iOS 模拟器 Release 构建和 iPhone SDK 的 build-for-testing 均通过；后者未签名、未安装到物理设备，不能作为真机验收。

本轮 E2E **201 通过、5 失败、14 忽略**，失败仍涉及两项 Chrome auth、MiniMax 手动语音与两项 poisoned voice lock；没有 HEAD 基线，不宣称全仓通过。生产 Rust 行为本轮未变，复用 v20 Mac Release；diff / 行数 / no-allow / 指南护栏通过。Computer Use 再次返回 Mac 锁定，未取得输入 / 导航切换的实际操作证据，不重复请求解锁。G2–G5 的可靠排队、手机中断 / 审批、增量恢复、指挥台 / Todo / APNs、真实双 Mac / iPhone / Tailscale 与无障碍范围均继续保留。

- [ ] 建立 Mac 受控网络入口与 SwiftUI 最小客户端；扫码、核对电脑、Mac 授权、Keychain 保存、撤销配对可用。
G3 手机中断（v22）：

- interrupt_turn 复用既有 control / grant revision、准确窗口路由、唯一 operation ID 和持久化回执。请求绑定 node/profile/run/session/turn，同一回合由思考转为工具或审批不使停止失效；旧回合、旧授权和重复执行被拒绝。
- SwiftUI 聊天工具栏使用系统停止按钮，至少 44pt 触控范围与可访问性名称；正在停止使用 ProgressView 与明确状态。先保存请求再发送，重新连接只回查；成功区分 interrupted / completed / failed，接收请求或按键写入都不显示为已停止。
- Mac 复用已有 Codex RPC 和 Claude 后台 EOF/历史证据流程，GPUI 前台只做内存与既有 PTY 操作。等待真实终态期间快照为 interrupting，删除仍须明确确认。修复 Claude 系统 Escape 被当作电脑草稿的问题；既有真实电脑草稿仍保留并阻止混合发送。
- 聚焦 TDD、正式 WSS 权限 / 旧回合 / 幂等、Runtime 和 Swift 检查通过；两种真实 Provider 的最终 macOS Swift 停止 → 回查 → 原请求重放 → 同会话续发在 29.28 秒通过。两种 Provider 的实际 iOS simulator 网络链路在 50.85 秒通过，覆盖准确停止、回查和续发。

本轮验证：Runtime **140 通过、3 忽略**；Swift **14 通过**；iOS DeviceTests **16 通过、2 跳过**（无 fixture 与物理设备文件保护）。App check、Mac Release（2m 21s）、iOS 模拟器 Release 与未签名 iPhone SDK build-for-testing 通过。全仓 App **794 通过、7 忽略**，停在 E2E **201 通过、5 失败、16 忽略**；失败涉及两项 Chrome auth、MiniMax 手动语音及两项 poisoned voice lock，无 HEAD 基线，不宣称全仓通过。行数 / no-allow / 指南 / 存储 / 性能 / 域层依赖 / diff 护栏通过。

后续排队已核对现有接缝：AgentSessionReplyQueue 只存在内存，Enter 写入后就移除条目，没有 Provider 确认或持久化完成状态，不能直接充当可靠手机队列。现有手机命令网关的完成预算为 30 秒、最多 8 个在途操作；排队不能仅通过延长超时伪装实现。下一段需要明确区分持久化 queued / 正在提交 / Provider 已接收 / cancelled / outcome_unknown，先落盘再执行；取消与开始提交竞争须只发生一次；关窗、授权撤销、跨运行恢复均不得静默派发旧消息。复用 v20 的准确发送回执和 v22 的运行校验，不沿用旧队列的 Enter 即成功。

Computer Use 仍返回 Mac 锁定，未取得停止按钮的真实操作证据；不把 XCTest / headless GPUI 等同于真机或页面验收。可靠排队、手机审批、增量恢复、指挥台 / Todo / APNs、双 Mac / iPhone / Tailscale 和无障碍仍属于本 Goal，G2–G5 不缩减。

G3 可靠队列存储（v23，尚未接入手机）：

复用 MobileCommandStore 与设备级操作 ID，新增 mobile_message_queue。入队时在同一事务写入完整正文和“已排队”回执，通用命令回执保持不可变；消息的 queued / dispatching / sent / cancelled / outcome_unknown 独立保存。入队回执表示当时已持久化，不表示当前仍排队或 Provider 已收到；接入端必须继续读取消息状态。

数据库按同 owner / 项目 / 窗口 / 卡片 / run 的实际入队序号领取，跨手机同卡也不能插队；不同卡不相互阻塞。claim 与 cancel 原子竞争，进入 dispatching 后不能声称安全撤回。只有明确输入前拒绝可回到 queued，未知结果不能重试。实际回执复核 node/run/session/turn，终态不可被另一回合覆盖；服务 owner 变化后旧 queued 显示取消，旧 dispatching 显示未知，不被新 owner 扫描执行。正文保留以供用户找回。

容量为每 owner 最多 128 个活跃消息、每条 32 KiB；上限 128×32 KiB=4 MiB 活跃正文。达到容量或写入失败整体拒绝，不静默淘汰；重复原操作仍可查询。新表通过外键关联既有账本，不改旧表状态 CHECK，不破坏已有创建 / 发送回执。

验证：4 项聚焦存储测试通过（含 SQL 第二次写入失败回滚、FIFO、取消 / 发送竞争、容量与旧 owner 防重放）；Storage 全量 **78 通过、0 失败**。既有移动网络命令 / 配对持久化 / 卡片持久化 **4 通过**，App check（3.97s）与行数 / no-allow / 指南 / 存储 / 性能 / diff 护栏通过。此次没有改变 UI 或 Provider 派发路径，未重复进行原生 UI、真实 Provider 或 Release 验证；沿用 v22 的已有证据，不能把这些存储测试称为手机排队验收。

下一段必须完成：WSS 入队 / 状态 / 取消；后台调度在最新 live snapshot、准确 run/session 与当时 control/revision 校验后领取；沿用 CommandLease 处理撤销 / 关闭竞争，再复用原有准确消息发送回执。手机明确显示等待、提交、Provider 收到、取消和未知，取消后正文可找回。不要把后台等待塞进通用命令的 30 秒完成预算，也不要让手机重连自动重放未知消息；需用两种真实 Provider 验证排队、取消、断线回查及同卡顺序。

G3 可靠队列接入（v24）：

- Mac 后台调度复用现有持久化队列与准确发送入口，每 500ms 检查最多 128 条等待消息、最多 8 个在途发送。等待不占用通用命令的 30 秒执行预算；GPUI 前台未增加文件或数据库读写。发送前重验设备权限版本、窗口、卡片、run/session，并与撤销共享 CommandLease。普通远程消息不能越过同卡已有队列。
- WSS 新增 queue_message、queued_message、cancel_queued_message；入队确认与实时发送结果独立。明确输入前拒绝回到队列，开始后响应丢失为 outcome_unknown，不能重新执行；授权变化、窗口或会话失效停止旧队列。撤回与领取原子竞争。
- SwiftUI 聊天忙碌时显示「加入队列」，确认持久化交接后可继续写下一条。队列使用系统 Sheet / List / NavigationStack，正文不内嵌长队列；44pt 系统操作、紧凑语义字号与纯黑界面落实 Apple Design。电脑页也能打开本机记录，已关闭项目 / 已忘记电脑的原文仍可复制；等待 / 发送中 / Agent收到 / 取消 / 未知分开显示，缓存标记上次状态。
- 本机请求先保存再发一次，重启只查询原 ID；成功入队只清对应版本草稿，未决记录继续保存至用户主动清除。原文不因取消或未知而丢弃，也不自动覆盖新草稿。查询核验完整身份、原文和新回合，撤回重新读取当前授权版本。
- 两种实际 Provider（Codex 0.153.4、Claude 2.1.202）均通过「运行时排三条 → 取消第二条 → 停止原任务 → Mac自动按序发送首尾 → 原操作重试不重复」：macOS Swift 36.71 秒，iOS simulator XCTest host 59.32 秒。Claude 会把中断记录写成一条系统生成的 user 消息；测试据真实历史验证这一项，没有删除或改变生产历史。

v24 最终验证：Storage **78通过**，Swift **15通过**；iOS DeviceTests **17通过、2明确跳过**，UI导航 **2通过**；正式移动网络/持久化回归 **6通过**。App check、Mac Release（2分钟）、模拟器Release、未签名iPhone SDK build-for-testing与常规约束均通过。全仓App **794通过、7忽略**，之后E2E **195通过、11失败、18忽略**：两项Chrome授权、语音显式Apply断言及8项poisoned voice lock。该Apply断言单独复跑0.24秒通过，但未修复全仓失败，也未做HEAD基线，不能宣称全仓通过。Computer Use 本轮仍返回 Mac 锁定，未取得队列界面的实际操作证据；原生网络、XCTest 导航和 headless GPUI 不能替代 Computer Use 或真实 iPhone。手机审批、增量恢复、跨电脑指挥台、计划/Todo、成果/diff、APNs 与 G5 全范围仍未完成。

G2 项目增量与恢复（v26）：

- Mac 新增持续 `watch_project`：新连接全量基线，同窗口仅变化卡片/删除项/完整ID顺序，窗口或状态变化回到全量；序号仅随状态变化推进。每次心跳均由新的授权与GPUI读取支撑，不能单凭TCP存活标记实时。
- 复用现有快照入口，前台只递增原子失效信号，后台最多250ms合并变化、闲时2秒核验。没有新增数据库写入、事件历史或前台序列化。订阅占原有八个连接之一，项目/事件超500KiB明确失败。
- Swift检查流/项目/序号及卡片覆盖关系，缺口清空基线；最多三次1/2/4秒只读重连，每次重新读全量，不重发任务或决定。缓冲满显式失效；取消/离开/后台停止订阅。SwiftUI卡片页恢复期间清空可操作状态，使用系统进度反馈并关闭旧窗口/旧权限Sheet；保留未决记录。
- 原生macOS 12.76秒、iOS simulator 35.51秒通过正式WSS → GPUI/Runtime/SQLite的持续保活、空卡增删、降权断流/恢复、写入拒绝与取消；无模型调用。修复实际macOS以NSPOSIXErrorDomain报告断流导致未重连的问题；红测保留。
- Norma **113通过/1忽略**；移动网络与持久化 **11通过**；Swift **17通过**；iOS DeviceTests **19通过/2跳过**、原生导航 **3通过**。全仓App **794通过/7忽略**，E2E **201通过/5失败/22忽略**，失败仍在两项Chrome授权、MiniMax手动语音及两项poisoned lock；没有HEAD基线，不宣称全仓通过。构建与完整日志见连接文档/证据JSON。

恢复得到当前状态投影，不声称重放离线期间所有瞬态；任务和审批仍以准确回执为准。聊天和审批聚合尚未改成事件订阅；原生真实窗口变更、真实Provider状态事件、Computer Use、双物理Mac/iPhone/Tailscale仍需补验收，G2–G5不勾选完成。Apple Design继续通过SwiftUI系统控件、紧凑语义字号、44pt操作与减少动态效果落地。

- [ ] 手机身份独立于本地 Provider 身份；每个请求校验目标电脑、项目、权限及运行实例。
- [ ] 两台 Mac 分别完成局域网 / Tailscale 连接；IP 或网络变化不改变已配对电脑身份。
- [ ] 接入快照与增量事件；事件缺口回到快照恢复；窗口关闭、断网、重启和过期会话不显示为实时运行。
- [ ] 指令包含唯一请求标识并可查询回执；丢失响应后不重复派发；离线写操作不静默重放。

退出条件：真实 iPhone 在两台 Mac 间切换，读取真实任务并发出一条指令；切换网络后仍能确认该指令结果，另一台电脑不受影响。

### G3 · 项目、卡片与聊天

- [ ] 复用项目路由、Agent 定义与卡片生命周期；创建卡片不派任务，忙碌卡片关闭需要确认，任务历史保留。
- [ ] 聊天提供分页历史与增量更新；消息、草稿和回执绑定电脑 / 项目 / 会话，不能因同名项目或相同局部 ID 串线。
- [ ] 发送、排队、运行、中断请求和已中断有不同的真实状态；运行中的 Agent 不被普通聊天输入意外覆盖。
- [ ] 只读、已撤销、离线和过期会话在 Mac 端拒绝写操作；桌面与手机共享状态来源。
- [ ] 按已确认的 Apple 风格实现页面，覆盖键盘、44pt 触控、长文本、焦点与减少动态效果。

退出条件：Codex 与 Claude 均可在手机上完成创建卡片、安排任务、查看对话、追加消息、取消及关闭；退出窗口后手机无法继续控制旧会话。

G3 长对话分段传输（v27）：

- 保留Provider游标与页顺序，小响应格式不变；大于500KiB的页改为同一WSS连接的冻结分片，64KiB原始UTF-8/片，最坏JSON转义384KiB。单页32MiB，不放宽Codex私有RPC8MiB或Claude单行8MiB/扫描16MiB上限；更大页仍明确失败，不冒充无限大小支持。
- Swift收齐并验证完整请求后才返回ConversationResponse；offset缺口、混流、取消、断线均不显示半条消息、不推进历史游标。恢复重新只读原Provider游标，不重发任何任务。没有新增缓存服务、数据库表或依赖。
- Mac复用正式快照来源，在分片起止验证准确窗口/run/session，每片前重验权限；最后一片前运行被替换时拒绝完成。网络/序列化仍在后台，GPUI不新增IO。
- 真实WSS明确fixture验证长Unicode/工具输出、末片前运行变化和撤销；Swift单测验证重组和缺口。原生macOS0.75秒/iOS simulator12.15秒通过大响应，实际Codex/Claude经正式GPUI/Runtime的macOS聊天回归28.20秒通过；该小消息Provider回归不证明真实模型产生过超长消息。
- Norma114通过/1忽略、移动网络/持久化12通过/1忽略（原生大响应已单独显式通过）、Swift18通过；iOS DeviceTests20通过/2跳过。全仓App794通过/7忽略，E2E201通过/5失败/22忽略，仍是两项Chrome授权、MiniMax手动语音和两项poisoned lock，无HEAD基线。构建和日志见连接文档与证据JSON。

聊天仍按两秒刷新，实时事件待接入；真实Provider长消息、长内容页面操作/无障碍、双Mac/iPhone/Tailscale及通知仍待验收。Computer Use本轮再次返回Mac锁定，未取得新页面证据；本轮没有修改SwiftUI布局，不将原生网络检查冒充真实UI验收。

G3 持续聊天与恢复（v28）：

- 新增 `watch_conversation`，绑定准确项目/窗口/卡片/run/session，每连接sequence0完整基线。Claude按JSONL游标连续追加完整页；Codex仅在最近20回合页变化时发送该完整页，不宣称逐token差分。无变化为不推进序号的无正文心跳。
- 复用项目失效信号与后台250ms合并/2秒复验，以及v27的大响应分片。每次新读取、准确状态/授权核验成功才发事件；没有新增缓存服务、数据库或依赖，GPUI前台没有新增IO。
- Swift使用系统unfolding流按消费节奏读取，不用可能丢页的应用缓冲。断流/缺口最多1/2/4秒三次只读重连，先清空历史再建立新基线；撤销/错会话终止。SwiftUI两条独立订阅分别提供控制状态和对话，旧翻页取消并防迟到覆盖，离开/后台停止。
- 实际Codex/Claude经正式GPUI/Runtime的macOS连续聊天事件23.13秒、iOS simulator35.29秒通过；大响应/大事件fixture的iOS无损传输12.48秒通过。新增原生恢复/撤销fixture在macOS与网络检查合计5.80秒、iOS13.91秒通过；不把fixture称为真实Provider断网测试。
- Norma115通过/1忽略，移动网络与持久化13通过/2忽略（原生项另行显式通过），Swift19通过，iOS DeviceTests21通过/2跳过，导航3通过。全仓App794通过/7忽略，E2E201通过/5失败/22忽略；仍是Chrome授权与语音，未做HEAD基线。构建、护栏和日志见连接文档及证据JSON。

Apple Design继续落到SwiftUI系统导航、语义字号、原生输入框/键盘避让和可滚动完整内容。本轮改变聊天交互，Computer Use仍返回Mac锁定；XCTest导航不覆盖实际聊天交互，也不替代Computer Use或真机。G2–G5全部未完成验收项保留。

### G4 · 指挥台与通知


### 明确未发送后的重新安排（v32）

SwiftUI 在服务器证明上次未发送时显示「重新安排」，再次确认原指令、当前Agent和会话；换会话明确提示，忙碌时仍进入原可靠队列。先保存新操作与准确旧决定身份，断线只查原结果，不重放。服务器不接受手机替换原指令。

`retry_of` 条件绑定上一条设备/操作身份。复用原事务与发送执行器，追加不可变 `project_todo_retries`，原计划、决定、命令和队列均保留；每项只返回当前尝试。另一手机已重排后，旧预览不能再提交。旧owner尚未领取的队列在同一事务内真实取消；正在发送、结果未知、已送达、已拒绝的建议均不能重排。已送达后的失败/中断也不视作“未发送”。旧手机尚存的先前查询记录保留并提供明确核实后清理。

验证：Storage **85通过**，移动网络 **14通过/3忽略**，Swift **26通过**，DeviceTests **28通过/2跳过**，导航 **4通过**。网络fixture覆盖撤回→另一手机重排→共享状态→旧预览拒绝→新任务仅派发一次；两个真实Provider经iOS原生链路验证已送达任务不可重排，**39.19秒通过**。成功重排尚无真实Provider/SwiftUI操作验收，不能以网络fixture或导航代替。Computer Use再次返回Mac锁定。

全仓App **794通过/7忽略**，E2E **204通过/5失败/25忽略**，仍为Chrome授权/语音、无HEAD基线；v31首次Claude无效响应原因仍未确定。App check、Mac Release（2m15s）、模拟器Release和未签名iPhone SDK构建通过；红测和原生日志见[连接证据](../design-docs/2026-09-07-norma-ios-connection-evidence.json)。继续成果/diff、APNs与G5，完整Goal保持active。

### Todo 准确回合终态（v31）

Codex 的 `turn/completed` 和 Claude 的 `Stop` / `StopFailure` 绑定准确 node、run、session、turn；Claude 中断只认本次发送后的匹配 transcript 证据。复用既有后台审批写入线程，把终态保存到 AppDB 的 `mobile_turn_outcomes`，按 workspace/run/turn 唯一且不可覆盖。GPUI 前台不打开数据库；查询只有提交后的记录，旧卡片重开不改变过去 Todo。

SwiftUI 保留原生黑色列表，新增“本轮执行已完成 · 待验收”“Agent 执行失败”“任务已停止”。发送结果和回合结果分别校验；没有观察到终态时保留已送达，不能从当前卡片 Done 推断历史任务。完成只证明 Provider 本轮结束，成果验收尚未接入。

验证：Storage **83通过**，Runtime **143通过/3忽略**（含准确中断落盘），移动网络 **14通过/3忽略**（含另一手机读取、撤销与卡片更换后旧结果保留），Swift **25通过**，DeviceTests **27通过/2跳过**，既有导航 **4通过**。真实 iOS Codex/Claude **2通过，51.89秒**；macOS 首次 **Codex通过、Claude失败，43.10秒**，仅补阶段日志后 Claude **6.23秒复验通过**，首次原因仍未确定。计划为固定fixture，执行Provider真实，不证明规划质量或成果质量。

全仓App **794通过/7忽略**，E2E **204通过/5失败/25忽略**：Chrome授权两项、MiniMax手动语音及两项poisoned voice lock；未做HEAD基线。Computer Use仍返回Mac锁定，真实Todo页面、双Mac/iPhone/Tailscale和无障碍未验收。构建与聚焦红测日志见[连接证据](../design-docs/2026-09-07-norma-ios-connection-evidence.json)。完整Goal保持active。

### Todo 决定、发送与原生执行页面（v30）

- 计划详情可确认/不采纳每项Todo；确认Sheet完整展示原指令、电脑/项目/执行Agent和当前任务。执行会话与生成时不同会明确提示，空卡说明确认后才启动。保持Apple Design的黑色背景、系统List/Form/Sheet、紧凑语义字号、完整滚动内容和至少44pt行操作。
- Mac只从已保存的计划取指令，确认绑定用户预览的当前CardSnapshot，同node/profile可显式选择当前run/session；不允许手机替换指令或改派别的卡片。计划原目标和方向不覆盖。项目权限/版本、准确窗口/运行由正式链路复核。
- SQLite以(plan_id,todo_index)保证唯一决定，与原命令/可靠消息队列同事务提交。拒绝只保存决定，不派任务；接受复用原发送/排队/回执。跨手机只能一方决定，重试查询原结果。修复查询决定时重开SQLite导致的迁移锁竞争，复用当前连接。
- SwiftUI从项目事件取得当前控制状态，每2秒只读该计划的共享决定；后台/断流清空可操作状态，旧确认不能用于新会话。先保存本机决定再提交一次，恢复只回查。本人尚未发送的队列可撤回；真实送达后可进入同一实际会话，重开的run/窗口不冒充旧任务。已送达/拒绝/撤回后本机查询记录可清理，Mac原计划和决定保留。
- 「Agent已收到」仅表示实际发送回执，**不代表任务完成**。实际任务终态、成果验收及明确未发送后的显式重新安排尚未接入；未知结果不自动重放。确认后的建议不可改写，后续方向仍生成新计划。

验证：Agent/Storage **154通过**，移动网络/持久化 **14通过/3忽略**（含Todo真实WSS竞争/拒绝/排队检查）。两种真实Codex/Claude经正式GPUI/Runtime与原生macOS **23.44秒**、iOS simulator **42.93秒**通过；原始3项计划为明确fixture，实际执行两条无工具任务并核验真实回复，拒绝项不进入对话。该测试不证明真实Norma生成质量。Swift **24通过**，DeviceTests **26通过/2跳过**，既有导航 **4通过**。全仓App **794通过/7忽略**，E2E **204通过/5失败/25忽略**；5项仍是Chrome授权和语音，未做HEAD基线。Computer Use本轮仍返回Mac锁定，真实Todo页面操作和物理iPhone验收未完成。App check、Mac Release（2m17s）、模拟器Release及未签名iPhone SDK构建通过；护栏与依赖边界通过，详细日志见连接文档及evidence JSON，完整G2–G5仍未完成。

### 计划生成与原生下一步页面（v29）

- Norma复用既有ModelFactory、Provider配置与Rig，只生成项目总结和最多8项建议，没有执行工具或AgentCardAdapter。模型只能引用Mac给出的受控Codex/Claude；不存在、未受控、重复身份或伪造完成字段会被拒绝。生成尚未经过真实云模型质量验证。
- 每份建议保存完整用户方向、项目/窗口、目标卡片及run/session、具体指令；新方向形成新计划，旧建议不覆盖。SQLite发布后才返回`plan_id/proposed`回执，重复操作复用原结果。生成过程不会创建卡片或派任务。
- `generate_plan`复用既有控制权限、授权版本和幂等命令链路；`project_plans`允许已授权只读，按项目严格分页，每页最多4份。模型最多20秒，App关闭窗口会取消本地请求；迟到的已删除/变更目标不能发布。已开始的生成不因权限撤销回滚，撤销后不能读取计划或回执；执行Todo仍须重新核验。
- SwiftUI从指挥台和项目进入「下一步」，原生列表展示方向、总结、执行Agent和完整指令，支持切换电脑、历史分页及调整方向。未决方向先存手机，断线/重开只回查。计划回执与卡片/消息回执分别校验，不伪造node_id或任务完成；Todo接受/拒绝、任务关联和真实完成状态仍未接入。
- Apple Design落到黑色背景、语义字号、系统导航/输入/DisclosureGroup、可滚动完整文本和原生按钮反馈。Computer Use再次返回Mac锁定，因此实际计划页面的操作、长内容和无障碍验收仍未完成；模拟器导航不能替代这些证据。

验证：Agent/Storage **152通过**；正式WSS+GPUI/SQLite的生成、关窗取消和删卡拒绝 **3通过**。原生生成链路macOS **1.21秒**、iOS simulator **8.69秒**通过，使用本地HTTP模型fixture，经真实模型工厂/网络与持久化链路，不等同真实云模型或物理iPhone。Swift **21通过**，DeviceTests **23通过/2跳过**，原生导航 **4通过**。App check、Mac Release（3m26s）、模拟器Release和未签名iPhone SDK构建通过。全仓App **794通过/7忽略**，E2E **204通过/5失败/23忽略**；5项仍为Chrome授权/语音，未做HEAD基线。详细范围及可重跑命令见连接文档和evidence JSON。

下一步：为Todo建立一次确认/拒绝和原始建议到实际命令的准确关联，复用已有发送/队列/回执；继续成果/diff、APNs与G5，完整Goal保持active。


当前推进（v25）：

- SwiftUI 指挥台汇总全部已保存电脑的获授权项目，原生横向分页卡片、完整工具输入、单次批准 / 拒绝和独立决定记录。操作绑定卡片来源，切换当前电脑不改变决定目标；只读不能写，失败电脑显示需要重新核验。
- 手机先保存完整预览与唯一操作，再发一次；后台项目审计确认提交后才给回执。区分 submitted、request_closed 和 outcome_unknown，不把选择、请求关闭或网络回执称为工具执行成功；重新打开只回查，旧决定不能覆盖新请求。
- 保留 Codex 外部 Hook 优先权。统一列表显示仅可在电脑处理的等待，不暴露配置路径 / Hook 命令 / 输出；真实 Hook ID 可能嵌入路径，手机使用带命名空间的 SHA-256 摘要。没有可用选项的外部项不能构造或提交批准。
- 两种真实 Provider 的原生 macOS Swift 审批流程 **46.56s**、iOS simulator **64.57s**通过；核验拒绝文件不存在、批准内容准确、回执恢复与重复去重。Codex 客户端协议测试仅在已获授权的隔离子进程中暂不加载外部 Hook，保留 read-only / untrusted；生产启动与用户配置未放宽。
- Norma / Runtime **253通过、4忽略**，移动网络回归 **10通过**，Swift **16通过**；原生 DeviceTests **18通过、2跳过**，导航 **3通过**。全仓停在 E2E **201通过、5失败、21忽略**（两项 Chrome auth、MiniMax 手动语音与两项 poisoned lock），无 HEAD 基线，不宣称全仓通过。保留全部外部 Hook 的正式等待 / 停止检查另行通过：macOS 12.43s、iOS simulator 21.48s。App check、Mac Release（2m13s）、模拟器 Release、未签名 iPhone SDK build-for-testing 与约束护栏通过；未在物理设备运行。Computer Use 本轮仍返回锁定，未取得真实页面操作证据。

Apple Design 落实为系统 TabView 分页、系统按钮 / DisclosureGroup、语义字号与可滚动完整正文；可操作区域至少44pt，不通过滑动直接执行权限决定。实际长内容、VoiceOver / Dynamic Type / 减少动态效果及真实多机交互仍需 G5 验收。

- [ ] 各 Mac 保存真实决策请求，手机汇总获授权范围；卡片显示来源，操作不依赖当前电脑选择。
- [ ] 决策绑定请求和最新版本；桌面先处理、任务结束、请求过期或电脑离线时，手机不能用旧卡片执行。
- [ ] Norma 的方向、计划和 Todo 关联执行 Agent / 任务；区分建议、已安排和执行完成，用户输入留下回执。
- [ ] 接入受项目权限约束的成果 / diff 预览和验收；不能通过任意文件路径读取未授权内容。
- [ ] 建立已绑定设备的通知注册与撤销；APNs 仅携带最小定位信息和摘要，通知凭据留在服务端。
- [ ] 锁屏收到待确认、完成或失败提醒；点击通知定位正确电脑、项目和任务，并重新核验真实状态。

退出条件：两台 Mac 都产生真实事项，手机可连续处理；任务已在桌面处理时，延迟通知不会再次批准。

### G5 · 真机验收

- [ ] 保存实际 Provider、macOS、iOS 版本及构建信息，按已验证版本声明能力。
- [ ] 真实 iPhone 蜂窝网络通过 Tailscale 连接家庭 Mac；覆盖 Wi-Fi 切换、断线、重连与锁屏。
- [ ] 通信性能：记录项目更新、审批提交与聊天首包的p50/p95，以及空闲CPU/网络流量；区分TLS建连、网络RTT、应用处理和模型生成耗时。本机项目事件150ms护栏不等同于物理LAN/Tailscale指标。
- [ ] 覆盖两台电脑同名项目、重复发送、重复审批、桌面 / 手机同时操作、撤销单机授权和关闭目标窗口。
- [ ] 完成正常聊天输入、语音输入、长消息、Dynamic Type、VoiceOver 和减少动态效果验证。
- [ ] Rust 改动按触及 crate 运行聚焦测试；涉及原生流程更新 e2e；页面验收使用 Computer Use，保留操作与截图证据。
- [ ] 形成可评审 iOS 构建、Mac 构建和验收记录；明确模拟验证、真实 App 与真机结果各自的范围。

后续环境只读核验：Xcode 26.2（17C52）、iOS 26.3 模拟器可用；Tailscale 当前 Stopped、无在线 peers，Xcode 已登记的 iOS 设备 tunnelState 为 unavailable。真实双机与 iPhone 尚未形成可用连接；Apple 开发者签名和 APNs 配置仍未核验。发布、部署或上传前先准备可评审产物，再处理该步骤所需授权。

## 推进记录

| 日期 | 已完成动作 | 证据 / 影响 |
|---|---|---|
| 2026-09-07 | v28 多电脑 HTML 原型及浏览器验证完成 | [产品文档验收记录](../product-specs/2026-09-06-norma-ios-companion.md#7-交互验收与证据)；仅模拟数据 |
| 2026-09-07 | 确认窗口生命周期、APNs、跨电脑指挥台和 Codex / Claude 范围 | 本轮方案讨论；已同步产品文档 |
| 2026-09-07 | 核验现有消息与 Hook 接缝 | G1 所列源码；尚未进行真实 Provider 协议实验 |
| 2026-09-07 | 建立本 Goal 实施计划 | G0 完成；G1–G5 均待启动 |
| 2026-09-07 | 开始实施 G1，建立真实 Provider 协议检查脚本 | 两种聊天和 Claude 批准 / 拒绝通过；Codex 审批 Hook 干扰待处理；G2–G5 未启动 |
| 2026-09-07 | 用户允许仅在隔离测试中暂不加载 Codex 外部 Hook | 批准、拒绝、取消、两客户端竞争与分页通过；不修改用户全局配置 |
| 2026-09-07 | 验证 Claude 真实终端 Hook、桌面先拒绝与超时 | 终端保留；超时回到桌面确认，未自动写入；正式请求生命周期仍需接入 |
| 2026-09-07 | 用户确认新卡片受控、旧会话保持原状 | 可以开始改启动 / 恢复路径；保留原生终端与关窗退出边界 |
| 2026-09-07 | 落地手机卡片元数据、空闲创建与旧恢复隔离 | 旧快照默认关闭；桌面历史发现不能绑错新卡片；Canvas / Runtime 与 SQLite 集成通过，完整 E2E 有待处理失败 |
| 2026-09-07 | 接入受控 Claude 后台启动、请求级双向 Hook 与回合身份 | 域层、真实 bridge / HTTP 并发检查通过；真实 Claude 启动已通过，G1 正式消息 / 审批执行与 Codex host 仍待完成 |

| 2026-09-07 | v6：Claude 正式审批执行通过；接入 Codex 私有 host、真实首条任务、原生 TUI 和分页历史 | App/Runtime 真实 CLI 检查已取得进展；修复 CLI 参数差异和关闭残留 Thinking；G1 审批审计 / 完整同步及 G2–G5 保持未完成 |
| 2026-09-07 | 修复立即退出时后台回收不及的竞争，并复验两种真实 CLI | 独立子进程退出测试、Codex 消息 / 历史、Claude 批准 / 拒绝和 Release 增量构建通过；全量失败项保留，未对 HEAD 做基线 |

| 2026-09-07 | v7：Codex 请求级审批、项目审计、先落盘后响应与失效防护落地 | 聚焦测试通过；真实 0.153.4 发现并修复拒绝选项差异及中断后新回合状态问题，真实三回合执行及审计在 36.57 秒通过 |

| 2026-09-07 | v8：正式 Codex 中断、Claude 完整历史及跨回合增量读取 | 两种真实 CLI 检查通过；保留请求中 / 已结束区别，跨回合不重复消息；G1 尚未完成 |

| 2026-09-07 | v9：Claude 统一项目审批审计与先落盘后响应 | 锁库 / 超时 / 桌面撤销 / 回合结束 / 关闭 / 写失败检查通过；真实 Claude 两轮执行及审计在 20.47 秒通过 |

| 2026-09-07 | v10：旧卡片显式重开、无新任务恢复及 Claude 原生中断实验 | 两种真实 CLI 重开通过；发现 Escape 会撤销审批但不发 Stop Hook，需用精确回合与新增历史确认正式中断终态 |

| 2026-09-07 | v11：正式 Claude 中断与新增历史证据、回合竞争防护 | 等待审批 / 生成中 / 工具执行三种真实场景通过；生成前缺少证据时明确结果未知；G1 继续进行 |

| 2026-09-07 | v12：G1 正式多端竞争与外部 Hook 兼容通过，进入 G2 | G1 已收口；G2 新增设备 / 项目授权存储与撤销验证；网络配对与 iOS 客户端尚未实现 |

| 2026-09-07 | v13：Mac TLS / Keychain / WSS 配对授权基础与真实回环验证 | 未接入实际窗口、桌面配对 UI 或 SwiftUI；G2 继续进行 |

| 2026-09-07 | v14：按用户要求建立 SwiftUI 原生 iPhone 工程，多电脑 / 扫码 / pin / Keychain 客户端 | macOS 与 iOS 模拟器分别连真实 Rust WSS，通过错误证书、配对、授权及撤销检查；实际桌面入口、任务数据与真机仍待接入 |

| 2026-09-07 | v15：Mac 原生配对入口 / 窗口 owner、SwiftUI 重扫重连与权限编辑取消 | 原生网络与聚焦检查通过；实际 Computer Use 配对操作受阻，任务同步、双机 / 真机和 G3–G5 继续推进 |

| 2026-09-07 | v16：原生 SwiftUI 项目 / 真实卡片快照、品牌运行动效、前后权限复核和跨安装隔离 | 真实 WSS + GPUI / Runtime、Swift macOS / iOS simulator 通过；增量、写指令与实际 UI / 真机仍待完成；修正 Packager 测试过时字符串 |

| 2026-09-07 | v17：SwiftUI 空卡创建、设备级幂等回执、撤销 / 关闭竞争与异常结果查询 | 正式 WSS / GPUI / SQLite 和 Swift macOS / iOS simulator 通过；真实页面验收仍需解锁，继续推进任务派发 / 聊天及 G2–G5 |

| 2026-09-07 | v18：SwiftUI 卡片删除、准确回合确认、持久化删除回执与历史保留 | 原生双地址创建 / 删除回查、Runtime 审批取消与正式 GPUI 保存链路通过；继续 G2–G5 |

| 2026-09-08 | v19：原生 SwiftUI 完整对话页、Codex / Claude 分页和准确运行校验 | 两种真实 Provider 的 macOS Swift / iOS simulator → WSS → 正式 GPUI 历史读取通过；输入 / 发送及 G2–G5 其余范围继续进行 |

| 2026-09-08 | v20：SwiftUI 输入、两种 Agent 真实首条 / 续发、准确消息回执与桌面输入保护 | 原生 macOS / iOS simulator 实际发送及去重通过；修复 Codex 新空 rollout 附着竞争；排队 / 草稿 / 中断及 G2–G5 继续进行 |

| 2026-09-08 | v21：SwiftUI 持久化草稿、准确范围隔离、版本回执清除与全局本地草稿入口 | 两种实际 Provider 草稿发送链路、原生导航、模拟器 Release / iPhone SDK 构建通过；锁屏 / Computer Use 与 G2–G5 其余范围待验收 |

| 2026-09-08 | v22：SwiftUI 精确回合停止、持久化真实终态回执及 Claude 系统输入修复 | 原生链路和回归证据见连接文档；继续可靠排队、审批与 G2–G5，其余验收未完成 |

| 2026-09-08 | v23：可靠手机消息队列的事务存储、FIFO、领取 / 取消竞争与重启防重放 | Storage 78项和4项既有移动回归通过；后台调度、WSS与SwiftUI排队仍待接通，G2–G5保持active |

| 2026-09-08 | v24：SwiftUI 可靠消息队列、后台按序派发、撤回和原文找回 | 两种实际 Provider 的 macOS / iOS simulator 排队链路通过；Computer Use受锁屏限制，G2–G5继续进行 |

| 2026-09-08 | v25：SwiftUI 跨电脑审批指挥台、持久化决定回查与 Codex 外部等待提示 | 两种真实 Provider 的 macOS / iOS simulator 审批链路通过；保留 Hook 所有权，G2–G5 全范围继续推进 |

| 2026-09-08 | v26：SwiftUI项目增量订阅、缺口全量恢复与原生降权重连 | 原生macOS / iOS simulator与正式GPUI/Runtime/SQLite通过；聊天/计划/Todo/成果/APNs及G5继续推进，Computer Use仍受锁屏限制 |

| 2026-09-08 | v27：原生Swift长对话无损分段、末片前运行/授权复验 | 真实WSS、原生macOS/iOS大响应及两种实际Provider聊天回归通过；继续聊天事件、计划/Todo、成果、APNs与G5 |

| 2026-09-08 | v28：SwiftUI持续聊天、准确基线恢复和大事件分片 | 两种真实Provider的macOS/iOS连续聊天通过，原生断流/撤销fixture通过；继续计划/Todo、成果/APNs与G5，Goal保持active |

| 2026-09-08 | v29：Norma计划生成、不可变建议存储和SwiftUI下一步页面 | 原生macOS/iOS经真实WSS与GPUI通过，本地模型fixture；Todo确认/执行、成果、APNs与G5继续推进 |

| 2026-09-08 | v30：SwiftUI Todo确认/拒绝、唯一决定与原有发送/队列关联 | 两种实际Provider在macOS/iOS原生链路通过；继续真实任务终态、成果/APNs及G5，Goal保持active |

| 2026-09-08 | v31：SwiftUI Todo准确回合终态与后台AppDB持久化 | 两种真实Provider的iOS终态通过；保留Claude首次失败、全仓5项失败和锁屏限制；重新安排、成果、APNs与G5继续进行 |

| 2026-09-08 | v32：SwiftUI明确未发送后的重新安排、旧决定条件校验与不可变尝试记录 | 并发重排/旧队列取消/旧回执保留通过；真实iOS两种Provider禁止重排已送达任务；成果、APNs及G5继续进行 |


## v33 · SwiftUI 项目变更预览

项目卡片页和聊天页新增原生“项目变更”入口，显示当前未提交文件，可选择 HEAD 原文、暂存区、工作副本及两种 diff。使用 SwiftUI List/导航/系统菜单和原生 TextKit，黑色、紧凑字号、完整滚动、可选择不可编辑；避免多列控件挤压正文。Apple Design 落到语义字体、44pt行操作、动态字体和系统反馈；真实 VoiceOver/大字号操作仍待验收。

后台通过实际窗口绑定的本地目录读取，网络只接收 workspace/route，不接收文件路径；读取前后以及分片结束核验授权与窗口。600000字节中文文件通过真实WSS和原生客户端无损读取。HEAD、暂存版本与工作副本分别保留；排除`.git`、`.norma`、`.multisoul`内部数据，符号链接只读链接文字。文件每版本1MiB、完整内容/diff合计8MiB、128文件上限，超限明确失败；配置clean/process过滤器或子模块的项目暂不支持。

这只是一次冻结的**项目未提交变更预览**：没有任务起点基准，没有持久化交付/验收回执，不声称全部改动由当前Agent完成，也不包含已提交内容。工作树捕获不是全目录原子事务，执行中或预览后仍可能变化；二进制仅显示信息，图片/成果验收仍未完成。

聚焦域层、正式App路由、WSS授权/撤销/分片、Swift和原生TextKit检查通过。初次全仓发现3s测试客户端早于4s服务读取超时，随后仍出现`review_unavailable`；现按128文件的进程开销设10s捕获预算，手机首帧18s（2s×2目录核验+10s捕获+4s传输/调度），后续分片5s。最新全仓App794通过/7忽略，E2E205通过/5失败/26忽略；仍是两项Chrome授权、MiniMax手动语音和两项poisoned voice lock，未做HEAD基线。各次失败日志保留，不宣称完整验收或所有性能问题已解决。

Computer Use再次返回Mac锁定；原生模拟器/测试HostingView与headless GPUI不能替代真实页面操作或物理双Mac/iPhone/Tailscale验收。构建与精确日志见[连接证据](../design-docs/2026-09-07-norma-ios-connection-evidence.json)。Goal保持active，下一步为真实交付记录和验收闭环。

v33最终验证：移动网络15通过/3忽略，Swift27通过，DeviceTests30通过/2跳过，原生导航4通过。最新真实iOS预览检查10.18秒通过；App check14.06秒、Mac Release2m15s、模拟器Release和未签名iPhone SDK build-for-testing通过。文件行数、no-allow、存储/性能/指南及diff护栏通过，canvas/terminal/runtime/workspace依赖无GPUI。完整Goal未完成，未安装到物理iPhone；Computer Use与成果验收等缺口按上述记录保留。

## v34 · 任务起点与真实发送关联

手机端继续使用原生SwiftUI。本段接入Mac任务起点：直发、Todo确认、队列实际派发都先在后台捕获项目状态，最终校验窗口/卡片和授权执行权后才发送。捕获不提前占用执行权；明确未发送或被推迟的尝试不留下错误绑定。Provider确认准确node/run/session/新turn后，后台将起点与原设备/操作一起保存到AppDB；相同请求幂等，另一回合或不同正文不能覆盖。

起点保留用户已有未提交内容；`capture_since`同时枚举任务期间commit的文件，原文优先使用起点工作副本。覆盖授权子目录、首次commit、新增/删除、暂存区和后续修改；不推断各Agent作者。继续沿用128文件、单版本1MiB、总内容8MiB和10s捕获上限。Git的assume-unchanged/skip-worktree会隐藏真实修改，现与filter/submodule一样明确拒绝，不能伪装成干净起点。

捕获失败保存具体Unavailable原因，仍允许真实消息发送；保存起点失败保留真实发送回执并记录错误，后续交付必须报告缺失起点。发送预算为40s（10s捕获+25sProvider确认+5s落盘余量），只查询、不重放。捕获/输入/落盘不是跨进程原子事务，崩溃仍可能缺少起点；本段不宣称交付耐久性全部解决。

验证中的真实问题与修复：等待Provider回执曾阻塞确定性GPUI执行器，改成异步等待后通过；新回合身份校验先有红测再拒绝旧turn；隐藏index标志先复现空快照再明确Unsupported。原生Claude首次失败定位为测试PATH缺少审批Hook，补齐隔离测试PATH后通过；这不解释v31历史失败。新增测试曾误把dispatching视为结束、从已完成卡片读活动turn，已改用真实队列状态和持久化发送回执，未弱化验收。

本段仍未接入手机任务交付读取、不可变交付快照、内容指纹与验收回执。SwiftUI“项目变更”页面仍仅显示v33当前未提交预览；不能将域层支持已commit对比当作手机成果页已交付。APNs、图片与G5继续保留完整范围。精确测试/构建结果见[连接证据](../design-docs/2026-09-07-norma-ios-connection-evidence.json)的`task_baseline_v34`。

v34构建收尾：Mac Release2m10s、App check3.80秒、模拟器Release和未签名iPhone SDK build-for-testing通过；移动网络/持久化15通过/3忽略。最终Codex直发/Todo/队列起点检查21.01秒通过。依赖与机械护栏通过，未提交代码、未安装到物理iPhone；完整Goal继续active。

## v35 · 自动冻结交付与 SwiftUI 任务成果

每次手机实际发送确认后跟踪准确回合，Provider终态落盘后后台捕获与任务起点的变化，再保存不可变交付。直发、Todo和队列共用此链路；手机看到“整理成果”期间可写草稿/排队，不能继续派发、删除或停止已经结束的任务。原生终端可继续操作；若新回合抢先开始，旧交付记录明确标记不可用，不能读取后来内容冒充上一轮。弱窗口引用不保活App，关窗行为不变。

手机聊天页的“查看成果与变更”菜单接入原生SwiftUI任务成果列表，每页20条，按准确回合读取持久化正文。继续复用原有授权/窗口复核、UTF8分片和32MiB上限；当前项目变更与冻结交付分开。文件对比明确标记“任务起点”和“本次交付”。Apple Design落到系统List/导航/菜单、44pt操作、黑色背景、语义字号与可滚动可选择的TextKit正文，未新增界面框架。

交付保存包含准确终态和SHA256内容指纹，后续刷新或任务不能覆盖。时间表示保存时间，不宣称文件读取瞬间；仍是项目范围的非原子捕获，不推断作者。缺少起点或捕获失败保存原因；若准确终态未落盘或数据库写失败，仅记录诊断、不伪造交付，崩溃后的自动补收仍未实现。用户验收、反馈、图片、APNs和G5保持未完成。

验证保留红测与修复：自动保存/列表API/Swift回合校验先失败后通过；SwiftUI菜单出现编译器类型推断错误，使用已有NavigationLink写法并拆出菜单后通过。真实Claude三次检查分别是完成超时、发送回执未知、准确收到但45秒仍Generating；最后一次终端和Runtime一致为thinking，无交付完成证据。不能把这些失败解释为已解决，亦不能反推v31首次失败。完整全仓App795通过/7忽略，E2E205通过/5失败/27忽略，保留Chrome授权/语音失败且无HEAD基线。详细构建/网络与原生证据见`task_delivery_v35`。Computer Use仍返回Mac锁定，未验收物理iPhone/双Mac/Tailscale。

v35补充：红测发现交付跟踪错误阻止了运行卡片的确认删除，已将保护限定为collecting阶段；真实第四轮确认删除、卡片移除和Provider回收在27.50秒检查中通过。Codex iOS原生链路26.38秒通过；600kB固定成果在macOS1.52秒/iOS9.18秒通过。

v35构建收尾：Mac Release2m21s、模拟器Release和未签名iPhone SDK build-for-testing通过；Runtime144通过/3忽略、Storage88通过、Swift29通过、DeviceTests32通过/2跳过。行数/no-allow/存储/性能/指南/diff与域层无GPUI护栏通过。未提交代码，未安装到物理iPhone；完整Goal保持active。

最后网络回归：15通过/3忽略；App check3.99秒通过，日志见v35连接证据。


## v36 · 原生 SwiftUI 成果验收

任务成果详情新增“验收这份成果”，复用现有命令 Sheet、持久化未决记录与原操作回查。只有准确的已完成、可读且无冲突成果，在当前控制权限和卡片空闲时才可新验收；用户明确确认后提交，不派发 Agent。历史成果可以在空卡删除后验收，决定不会继承给后续任务。

Mac 复用正式授权、revision、窗口/route、执行租约和命令预留，后台 SQLite 将准确 key/fingerprint 的验收记录与原命令 succeeded 回执放在同一事务。两手机同时验收仅一个决定成功；回执写失败整笔回滚。成功回执与成果元数据均能回查，Swift 校验准确任务与内容；未知结果不重放。

Apple Design 使用现有系统 List、Sheet 和按钮，保持黑色、紧凑语义字体及至少44pt操作区；正文维持完整滚动与可选择。当前验收标记在详情中，历史列表尚无验收标签；修改意见回原会话尚未实现，不将验收动作当作后续任务完成。

验证：Storage89通过（含真实并发竞争、回执故障回滚、不可用/冲突拒绝），Swift30通过，DeviceTests33通过/2跳过，原生导航4通过。正式GPUI/WSS验收0.66秒通过；原生macOS2.50秒、iOS simulator52.31秒通过600000字节旧正文、持久化命令/回执与验收重读。这里使用明确终态fixture+真实文件/TLS/WSS/GPUI/Swift，不是模型完成或真实页面点击。v35真实Claude三次未完成证据仍保留，本轮没有重跑。

全仓App795通过/7忽略，E2E205通过/5失败/27忽略；两项Chrome授权仍停waiting，MiniMax手动voice断言失败，并触发两项voice锁poison。没有未修改HEAD基线，不宣称与本次无关。

用户已解锁Mac；Computer Use可读取并截图隔离Norma首页，但模拟器仍timeoutReached，桌面点击报告窗口被用户改变，重新读取/连接后仍未完成操作。当前阻塞原因已不再是锁屏。未生成实际配对凭据，未完成真实扫码/页面/物理iPhone/双Mac/Tailscale验收。截图与逐项测试/构建证据见连接JSON的delivery_acceptance_v36；Goal保持active，未提交代码。

v36构建收尾：Mac Release4m21s、模拟器Debug/Release、未签名iPhone SDK build-for-testing和App check通过；行数/no-allow/存储/性能/指南/diff护栏通过。未提交代码，真实页面与物理设备验收仍未完成。


## v37 · 原生修改意见回到原 Agent 会话

成果详情新增“提出修改”，使用原生Sheet/TextEditor，意见按电脑/项目/窗口/运行/会话/成果指纹保存为独立草稿。发送前先保存未决命令，成功后只清除对应版本的反馈草稿；同会话同文字的聊天草稿保留。意见正文保留首尾空格和换行，可从原操作记录核对；页面保留黑色、紧凑语义字号、至少44pt操作区和可滚动输入。

feedback_delivery始终携带准确旧key/fingerprint、原始意见和预览时的卡片。后台先校验并保存不可变反馈，再沿原发送链路捕获新任务起点；捕获之后仍以原快照验证实际会话、卡片与执行租约，不能转换成不带来源的普通消息。Agent收到带旧任务/成果引用的意见，并创建原会话中的新回合；Mac在原命令回执事务内校验新turn与旧run/session，将新任务身份和旧成果引用一起保存。失败/未知只回查，不自动重放；旧成果和既有验收不会变成新任务结果。

Storage90、Swift31通过；正式GPUI/WSS准备态旧反馈拒绝0.85秒通过。两种真实Provider在macOS原生链路37.85秒通过；iOS Codex通过、Claude43.83秒通过，均覆盖真实完成→自动冻结→意见草稿恢复/单次发送→新回合/原文核对→旧成果不变→关闭后拒绝。普通草稿保留，原生终端保留，反馈不产生验收决定。不是Computer Use或物理iPhone验收。

联调先发现父仓库配置Git LFS导致baseline_unavailable，未修改用户配置；独立临时Git项目后通过。iOS首轮Claude因测试模式白名单遗漏而跳过，外层marker校验正确判失败；补齐后实际运行通过。v35的三次Claude失败仍作为历史未解释案例保留，不将本轮不同场景通过当作其根因已修复。

全仓App795通过/7忽略，E2E205通过/5失败/29忽略；两项Chrome授权、MiniMax手动voice和两项poisoned voice lock仍未解决，没有未修改HEAD基线。原生图片预览、APNs和G5继续保留完整范围，Goal保持active；详细日志/构建见delivery_feedback_v37。

v37构建收尾：Mac Release、模拟器Debug/Release、未签名iPhone SDK构建与App check通过；DeviceTests34通过/2跳过、原生导航4通过，网络回归8通过。Mac已解锁，用户打开Norma后Computer Use仍报Simulator timeoutReached；Mac首页可读但连接按钮未产生页面变化，后续点击报user-changed，实际交互未验收。截图见artifacts/norma-ios-v37/mac-computer-use.png。

## v38 · 原生图片成果预览

复用既有获授权成果字节，用ImageIO后台解码PNG/JPEG/GIF/HEIF/TIFF/WebP等系统栅格格式。新增图片默认显示交付版本，删除图片默认显示任务起点；原生版本菜单保留文件与文本diff切换。图片使用系统捏合滚动、VoiceOver放大/还原操作和黑色背景，遵循Apple Design的内容优先、直接操作和减少动态效果。

边界：不请求图片URL、不读取符号链接目标；单图1MiB、源图最多16777216像素，缩略图最长边2048；动画只显示首帧并标明静态预览。损坏/非栅格/超限有明确提示。版本改变取消旧解码并清空旧图。

原生图片/正文宿主检查3通过；DeviceTests36通过/2跳过、导航4通过；真实Git/WSS图片传输与权限回归1通过。Mac已解锁，但Computer Use仍报Simulator timeoutReached，实际页面点击未验收。精确构建、日志及剩余项见image_preview_v38；完整Goal保持active。

v38收尾：模拟器Release、未签名iPhone SDK构建及行数/no-allow/存储/性能/指南/diff护栏通过。完整E2E为199通过/11失败/29忽略：显式语音Apply仍为None，两项Chrome授权停在waiting，随后8项语音检查因共享锁poisoned失败；没有HEAD基线，未声明与本次无关或已经修复。


## 历史验收列表推进（v39）

历史列表已关联真实验收并核验身份与指纹；SwiftUI详情回传更新对应行。Storage90、Swift31、DeviceTests36通过/2跳过、导航4通过；原生macOS/iOS WSS和三类构建通过。两项Chrome测试使用生产drain周期后通过；MiniMax切换失败仍保留，未宣称完整E2E通过。Goal继续active。


## 连接码与模拟器实际配对（v40）

用户已选择可复制粘贴的连接码。Mac在手机连接页显式复制norma-pair-v1连接码，与二维码共享同一签发、证书pin、端点、一次性凭据及120秒有效期；手机在添加电脑页输入并读取连接码，展示电脑身份后才发起配对，仍需Mac批准项目范围。过期或取消后不能复制，未启用时没有假连接码。

iOS另新增系统照片选择与Core Image二维码识别，检查图片大小并拒绝多码、无代码与损坏图片。新入口使用系统Form、TextEditor、PasteButton和PhotosPicker，保持黑色、语义字号及原生取消反馈；未绕过TLS或电脑授权。

验证：桌面连接码复制/到期/取消1项、Settings E2E2项、Swift31项通过；iOS原生聚焦2项与导航1项通过；Mac与模拟器Release、未签名iPhone SDK构建及常规护栏通过。Computer Use已进入真实电脑扫码页和手机图片识别/确认电脑页，但二维码因操作延迟过期；未取得实际授权。后续内容区点击持续noWindowsAvailable，当时请求协助复制长码；该请求现已由 v41 验证码方案替代。


## 六位验证码配对（v41）

用户已改为数字验证码，v40 的长连接码 UI/解码入口已移除。Mac 显示、复制或刷新 6 位 ASCII 数字；手机首次填写 LAN/Tailscale 地址（默认 9443）和验证码，保留前导零。输入页使用黑色原生 Form、数字键盘、oneTimeCode、进度与取消反馈，验证成功后记住地址。二维码和图片扫码继续可用。

验证码不作为设备 token，也不在网络发送明文。`mobile_pairing` 通过 RustCrypto SPAKE2 0.4.0 交换一次性公共消息，使用派生密钥和随机 nonce 的 AES-256-GCM 加密原有 QR 身份/凭据；Swift 只在解密并验证原协议后进入严格证书 pin 的正式配对。有效期 120 秒，所有接口/连接合计最多 3 次交换，取消、刷新、到期或 QR 认领均关闭旧入口；Mac 仍需批准具体项目权限。两端共用小型 Rust 库，生成的 XCFramework 不包含 GPUI/runtime，支持 macOS 与 iOS 模拟器 arm64/x86_64、iPhone arm64。

验证：Rust Norma/配对库 117 通过、1 忽略；Swift 32 通过；原生 UI/输入校验 2 通过；真实 macOS 与 iOS WSS 均完成错误码拒绝、第三次成功/第四次锁定、正确码解密、Mac 只读批准、项目读取及撤销。QR 网络/持久化回归 4 通过，桌面复制生命周期 1 通过。小窗口弹窗先复现末行超出窗口（y=786.5px），复用组件内置滚动并限制高度后 Settings E2E 3 通过。

完整 E2E 为 207 通过、3 失败、29 忽略：MiniMax 手动语音选择断言仍失败，其余两项为该锁 poisoned；未声明完整 E2E 或完整 Goal 通过。Computer Use 已实际开启新版桌面服务并查看验证码，也打开了 iOS 配对向导；模拟器内容点击仍间歇性 noWindowsAvailable。当前尚未完成用户实际桌面注册，已请求停留在验证码输入页；没有物理 iPhone、双 Mac 或真实 Tailscale 验收。Goal 保持 active。

v41构建收尾：Mac Release（2m28s）、模拟器双架构Release、未签名iPhone SDK build-for-testing、App check与配对库Clippy通过。最终版本已更新隔离桌面App与iPhone 17 Pro模拟器，保留已有数据。

## v42 · 实际配对确认、多电脑隔离与通知服务

用户确认已成功连接当前桌面 Norma 与 iOS；这是用户操作证据，未扩大为物理 iPhone 或双 Mac 验收。保留当前已连接的 v41 App，未因构建或测试重启、重装用户实例。

截图中的 10.8.0.37 属于普通 VPN 点对点接口，实际可达 LAN 为 192.168.110.118。共享接口发现已排除非 Tailscale 点对点地址，保留 Tailscale IPv4/IPv6；验证码超时改为检查电脑地址/网络的指引。地址回归 1 项、App check、Mac/模拟器 Release 通过，v42 构建尚未覆盖当前运行实例。

新增两个独立 WSS/SQLite 服务，故意复用项目名、项目ID、手机ID、卡片/运行/审批/操作ID：错误电脑/凭据拒绝；A撤销取消其待执行决定，B批准保留；A关闭后B能回查，重复请求不再次投递。3项网络回归通过。这是网关到执行入口的隔离证据，不冒充两个物理Mac或真实Provider工具执行。

用户有 Apple Developer 账号，尚未配置推送服务。新增 [私有APNs relay](../../services/push/README.md)，使用 Node 标准库实现HTTP/2、ES256签名、JWT复用、最小通知、入口认证及有效期内去重。2项本地HTTP/HTTP2测试通过，未向Apple发送真实通知。Mac/iOS的token注册、撤销、真实事件投递和点击定位仍待接入；G4通知不能勾选完成。

完整E2E为208通过、3失败、29忽略，失败仍为MiniMax手动语音选择及两项锁poisoned；不声明全仓通过。Computer Use可读取当前桌面，但Simulator返回noWindowsAvailable/timeoutReached，未新增手机页面验收结论。后续从设备通知注册继续，Goal保持active。

## v43 · 加密连接中的通知订阅与撤销

Mac AppDB新增每个手机独立的APNs token/环境/注册时间/版本；读取和写入复用现有WSS、证书pin与独立手机凭据。手机只有经过Mac授权后才能管理自己的订阅，只读项目也可订阅，通知注册不会提升项目权限。配对默认关闭提醒。

更新采用明确版本校验：注册、换token、关闭和失效均推进版本；迟到旧更新返回变化错误，需要重新读取，不能自动重放。关闭后保留版本并移除token，防止旧启用请求复活。Mac撤销授权在同一事务中清除token；APNs失效只作用于投递时的准确注册版本，且失效时间不能早于重新注册。调试描述不输出token。

SwiftCore已提供读取和显式更新接口。Storage91项、Swift33项及配对/命令/订阅网络回归8项通过；真实macOS Foundation和独立iOS模拟器XCTest host均跑通只读注册、拒绝旧版本、关闭、重开及撤销后拒绝。新测试使用Norma Acceptance iPhone（5265591D-704C-4515-868D-C2D64F94A7FA），未重装或关闭用户已连接的iPhone17Pro/桌面实例。这里只使用隔离token，未向Apple发送通知。

完整workspace仍停在E2E（208通过、3失败、29忽略），失败为MiniMax手动语音与两项锁poisoned；不能宣称全仓通过。当前没有新UI变更。下一步接入真实事件发布、iOS系统注册回调和用户设置；服务部署、签名和物理锁屏验收仍未完成。


## v44 · Mac事件投递、SwiftUI任务提醒与回源

- 修复多电脑订阅串用：状态/回执绑定真实Mac和当前Keychain手机身份；旧电脑或另一手机状态在发送前拒绝。现有注册、关闭、撤销和CAS协议继续保留。
- Mac后台消费者接入既有审批源与AppDB准确回合终态分页；私有HTTPS配置、最小事件、最新授权/订阅复验、过期丢弃、不重试未知投递、APNs失效清理均接入。测试使用有界前台fixture、真实WSS owner/SQLite/HTTP；并非本轮真实Provider→Apple验证。
- SwiftUI通知设置、系统授权/成功失败回调、按电脑订阅、仅对已启用订阅续token和通知点击定位已接入。通知内容不携带控制动作或新网络地址，旧窗口/回合/请求拒绝；冷启动等待本机电脑目录加载。Info.plist与entitlement共用APS_ENVIRONMENT，Debug/Release分别为development/production。
- Computer Use恢复：已实际点击独立模拟器中的Norma、任务提醒入口、黑色空态。5项原生导航通过，注入APNs回调与目标验证通过；配对后的通知开关、系统通知中心点击、VoiceOver/Dynamic Type及锁屏验收仍未完成。用户正在使用的v41实例保持原状。
- 当前验证：Storage91、Swift35、relay2项、WSS投递/订阅2项和其他网络7项通过；原生macOS与iOS订阅闭环通过。全仓停在E2E208通过/3失败/29忽略，仍是MiniMax manual voice和两项poisoned lock；没有全仓通过。构建、截图与最终检查日志记录在evidence JSON。

下一步需要用户确定relay部署位置并配置APNs Key ID和本地.p8文件（不在聊天中粘贴密钥）。随后完成真实token获取、已配对通知设置/点击及锁屏；继续物理双Mac/Tailscale和完整G5验收。服务代码已可评审，尚未部署，也未向Apple发送真实推送；Goal保持active。

签名账号已按桌面Packager.toml沿用公司Team `2BF8G9AN4L`。原通配描述文件缺少推送能力；Xcode自动更新已成功生成专用描述文件与已签名Debug构建，签名校验及实际aps-environment=development检查通过。没有进行App Store/TestFlight上传。当前登记的物理iPhone 11不可用，锁屏验收仍待连接设备和APNs服务配置。


## v45 · Mac 通知服务配置与运行中替换

手机连接设置增加「任务提醒 → 配置服务」，使用现有 gpui-component 的输入框、遮蔽凭据、环境选择和明确保存/停用按钮。按照 Apple Design 保留紧凑层级、焦点/选中/禁用反馈与可关闭的滚动对话框；不新增表单框架。服务地址、环境和凭据原子绑定在 Keychain，界面不回显保存过的凭据；清除后留下明确停用标记，不回退到环境变量。

替换服务只更新后台通知发送端，不重启 WSS、配对或 Agent。代次检查拒绝切换前读取中的事件，保留当前生命周期的审批去重，终态从新的水位开始；关窗后拒绝迟到配置。无配置或配置读取错误不会阻止手机配对，保存也不表示 APNs 送达。

配置单测2项与 Settings E2E3项通过，通知worker真实本地HTTP测试覆盖停用、更换地址/凭据、不重放审批和授权保留。Computer Use 在独立 bundle `com.norma.push-settings-validation` 实际完成输入、遮蔽、HTTP拒绝、HTTPS保存、关闭重开、清除停用及关闭验收实例；原已配对v41桌面PID21316未重启。验收构建使用测试凭据存储，不能扩大为正式Keychain跨进程恢复或真实APNs验证。截图和最终构建/网络结果见连接evidence JSON的 `push_configuration_v45`。

完整E2E本轮202通过、9失败、29忽略：`background_setup_install_requires_explicit_voice_activation` 的显式Apply断言失败，另8项为共享语音锁poisoned。未修改HEAD基线未核验，不声称全仓通过，也没有扩大到语音模块调查。APNs部署位置问题仍待用户答复；Key ID/.p8、真实token、配对后通知开关与点击、物理iPhone锁屏和双Mac/Tailscale验收继续待完成。Goal保持active，未提交代码。


## v46 · 原生通知开关与冷启动崩溃修复

新增独立Rust通知fixture和原生iOS验收：真实TLS配对、Keychain、SQLite订阅路径，明确只读授权；无Provider执行，卡片与窗口为合成验收数据。原生状态检查覆盖开启、token变化后的续登记、关闭后不自动复活、撤销后清理；独立模拟器随后通过系统通知授权取得真实APNs token，并经原生UI开启/关闭这台测试电脑的提醒。这证明系统登记和本地WSS订阅，不证明relay向Apple投递。

本地 `simctl push` 加SpringBoard通知点击首次重现冷启动崩溃：UIKit报 `Call must be made on main thread`，栈落在 `PushNotifications.userNotificationCenter` 的异步回调完成。改用显式completion-handler代理，目标状态更新与completion均在MainActor执行；前台展示回调也收口同一线程边界。修复后真实原生UI测试通过：点击通知冷启动并向准确Mac/项目回源；替换窗口后第二条旧通知被拒绝且不显示打开对话入口。截图来自XCTest，不标作Computer Use。

原生配对/订阅生命周期1项、系统注册开关UI1项、冷启动/旧窗口UI1项、忘记电脑UI与系统回调2项通过；交互fixture正常结束并撤销授权，测试手机通过正式忘记流程回到空态。已签名Debug iphoneos与Release simulator构建通过；签名校验通过。完整Rust E2E为208通过/3失败/29忽略，仍是MiniMax手动语音断言及两项共享锁poisoned，不宣称全仓通过。证据、红灯日志与截图见连接JSON `notification_acceptance_v46`。

Computer Use连续读取Simulator仍返回noWindowsAvailable/timeoutReached，不能用XCTest替代完整界面验收。当前登记的物理iPhone 11仍unavailable；APNs部署位置问题待答复，Key ID/.p8和真实锁屏投递仍未配置/验证。用户已配对v41桌面PID21316保持运行，未覆盖其模拟器。Goal保持active，下一步继续未完成的G5与真实APNs验收。


## v47 · 大字号卡片与聊天验收

复用现有独立只读WSS fixture，通过原生XCTest分别按系统默认Large和最大辅助字号AX5导航项目/卡片/聊天。系统审计真实发现项目与聊天顶部电脑名称截断；键盘展开并输入五行后，对话滚动区域仅剩28pt。修复后辅助字号采用纵向电脑信息和完整卡片标题，输入框最多显示两行并原生滚动，五行原文完整保留；默认仍显示最多五行。权限提示保留，未开放只读发送。Apple Design落实为语义字号驱动布局、内容完整与原生文本滚动，不缩小用户选择的字号。

两项原生UI测试通过，覆盖两页的textClipped/hitRegion/sufficientElementDescription审计、五行输入/清空和只读拒绝；AX5正文滚动区域117pt，默认260.3pt。正式忘记流程清理通过，两次隔离fixture均正常退出并撤销授权。Debug已签名iphoneos与Release simulator构建、签名校验和代码护栏通过。完整Rust E2E为202通过/9失败/29忽略：本地语音显式Apply断言和8项共享锁poisoned；没有未修改HEAD基线，不宣称全仓通过。

Computer Use读取Simulator再次timeoutReached，不能将系统审计/XCTest截图等同真实Computer Use或VoiceOver操作；本组fixture没有真实Agent对话正文，不重复声称发送链路已验收。减少动态效果、真实语音输入、物理双Mac/Tailscale、APNs投递和锁屏仍待完成。用户已配对的桌面PID21316未重启，其模拟器未覆盖。详见连接JSON `accessibility_acceptance_v47`；Goal保持active。


## v48 · 桌面回归失败的根因修复

MiniMax 目录父容器缺少 flex 布局，使列表只有边框宽度、音色行宽度为零；补齐两层布局后，实际点击可更新 Draft，显式 Apply 才切换运行音色。选择状态原为进程静态变量，新建第二个 App 会清空第一个已激活的音色；现与各自的 DialogueControlState 和常驻客户端一起保存，后台捕获准确选择。新增第二 App 的确定性回归先失败后通过，没有用测试专用替身隐藏竞争。

GPUI 0.2.2 的 debug_bounds 未清理旧帧，也未重放缓存视图的选择器。现按绘制顺序记录，随帧清理并随缓存区间重放；移除标签必须消失、缓存标签仍存在且只渲染一次的回归通过。原 Zipformer 升级 UI 的 Install/Retry 互斥检查随之通过。还修正了 Review Setup 测试点击视口外按钮的假通过：复用滚动 helper，返回后同时检查 Ready 存在与 Done Reviewing 消失；安装激活等待真实状态而非固定空转次数。

完整 `cargo test -p e2e` 已通过：headless 212、集成 18，共230通过、0失败、34忽略。受影响语音38项单测、App check、聚焦红绿回归和代码护栏通过。Mac Release构建通过（3m46s）；`cargo test --workspace`完整通过，共1737通过、0失败、62忽略。忽略项仍需对应外部环境，不能计作真机或真实Provider验收。详见连接evidence JSON的 `desktop_regression_v48`。

已增加仅验收构建可用的 voice-catalog 预览，使用隔离数据库、合成目录和假PCM，不访问真实MiniMax。真实App已启动，但Computer Use坐标点击报noWindowsAvailable，随后明确报告Mac已锁屏；未完成音色目录真实点击验收。本轮隔离实例PID97898已回收，用户配对实例PID21316继续运行，原iOS未重装。正式推送配置的Keychain跨进程恢复本轮只核对读写分支，尚未验证。APNs部署/密钥、物理锁屏、双Mac/Tailscale、完整Computer Use与剩余无障碍验收仍待完成；Goal保持active。


## v49 · 正式推送凭据恢复与真实规划检查

正式 `security_framework` 路径已完成11个独立进程步骤：未配置回退、保存/重读Sandbox、另一安装隔离、更换/重读production、停用后不复活、损坏后明确失败且不覆盖、删除测试项。L3 worker独立构建，明确拒绝 `test/e2e-test-support` 替身；仅使用随机临时安装、合成凭据和 `.invalid` 地址，无网络请求。1项真实Keychain集成检查1.51秒通过且已清理；此证据不包括正式App窗口重启或升级签名ACL兼容。

真实Ollama规划暴露了格式错误和提示词示例污染。复用Rig原生JSON schema约束Ollama输出，保留OpenAI兼容协议、原有目标/大小校验和20秒预算，不新增工具；移除硬编码任务示例，并明确事实、方向与可观察验收条件。原生schema测试先失败后通过。

真实模型结果必须分开看：`qwen3.5:0.8b` 出现格式错误、超时和不遵循方向；`qwen2.5:1.5b` 最后两次请求结构、数量和卡片7/8绑定通过，但人工阅读发现虚构“已完成所有任务”、视觉任务偏向网络且缺少可观察验收条件，**规划内容不合格**。不再反复调提示词试运气。保留可重跑测试和完整合成结果；该测试明确只检验结构与目标，不能证明一般规划质量。正式云配置的Keychain读取需要用户交互，当前尚未调用云模型。

最新环境核查：Computer Use报告Mac锁屏；登记的iPhone 11通道不可用且Developer Mode关闭；Tailscale为Stopped。未修改这些状态，也未重装用户已配对的桌面/模拟器。APNs服务与Key ID/.p8、物理锁屏、双Mac和完整G5仍待完成。详细日志与真实模型原文见连接evidence JSON的 `keychain_and_planning_v49`。


v49追加验收：原生Todo测试增加明确未发送的前次决定、精确重排关联和旧回执保留检查。原失败是持久化fixture，后续手机重排、派发、回合结果与去重均走正式Provider；Codex和Claude在macOS原生客户端均通过，共54.87秒。仅两条应执行原文出现，拒绝项不执行，原失败没有被新成功覆盖；不能扩大为原失败由真实界面触发。

用户确认曾删除 `/tmp`，导致旧日志和模拟器testmanagerd socket丢失。已按用户授权通过系统管理员验证恢复 `/private/tmp` 为root:wheel、01777；重启独立验收模拟器后重新生成测试socket。用户已配对Norma PID21316未重启。删除前已观察的测试结果保留，但旧临时文件无法恢复；后续证据改存工作区 `target/norma-ios-evidence/v49/`，模型原文已保存在证据JSON。目录恢复后全workspace再次1738通过/0失败/64忽略，App check/Mac Release通过；Swift35项、正式Keychain复验1项通过。iOS模拟器的两种真实Provider重排复验均通过，共77.26秒；原失败仍为持久化fixture，不能替代真实SwiftUI操作或物理iPhone。APNs与完整G5仍未完成。


## v50 · 通知跨弹窗与连续目标刷新

Computer Use实际复现：打开「任务提醒」设置后将App放后台，点击模拟器锁屏通知，旧设置挡住了目标页。改用当前scene中的原生呈现链，在已有Sheet/嵌套页面上方显示通知，关闭后恢复原上下文；显式传递scenePhase，保持回前台核验和后台取消。按Apple Design使用系统模态过渡与明确关闭入口，无固定延时，不清空导航或草稿。

新增原生失败回归后通过：设置弹窗、前台连续通知、嵌套验证码输入、冷启动与旧窗口拒绝，共5条路径（51.026秒）。前台收到相同event_id但不同route的通知曾错误保留旧任务；现在以完整目标作为页面身份，重新回源，不能用旧项目/卡片作为新目标结果。测试还核验关闭后订阅状态与未发送地址不丢失。Swift35项通过，原有机器/项目/窗口/运行/回合授权校验继续复用。

Computer Use已实际开启/关闭测试电脑提醒，并在呈现修复后从模拟器锁屏通知进入正确Mac/项目和任务入口，截图保存到`target/norma-ios-evidence/v50/`。之后控制连接报告noWindowsAvailable/timeoutReached，最终完整目标刷新只完成XCTest；关闭恢复、嵌套输入也仅有XCTest证据，不能视为完整Computer Use或物理iPhone/APNs验收。

首个交互fixture超过15分钟退出，已按完整电脑元数据限定清理独立模拟器上的测试配对。之后两次fixture均通过正式UI忘记电脑、写ui-finished、撤销授权并停止服务；用户桌面PID21316仍运行，未重装用户模拟器。所有新日志位于工作区，未依赖已删除的旧/tmp文件。APNs部署/密钥、物理双Mac/Tailscale、完整无障碍与真实云模型规划质量仍待完成，Goal保持active。

v50收尾验证：Swift35通过；原生DeviceTests42通过/4明确跳过（物理文件保护与需显式fixture项），导航5通过，最终通知5路径通过；模拟器Release和未签名iPhone SDK构建通过。两次交互fixture与正式忘记均正常结束；行数/no-allow/指南/diff护栏通过。无Rust生产变更，沿用v49完整workspace证据，不以本轮局部检查替代真机或完整Goal。


## 环境复核（v51，无代码变更）

2026-09-08 20:05 CST：仅读取当前配置的OpenAI模型与准确Keychain条目；10秒内仍需系统交互，已终止该读取进程，没有发送云模型请求、输出密钥或更改ACL。Computer Use明确报告Mac锁屏且自动解锁失败；iPhone 11仍unavailable，Tailscale为Stopped。已向用户请求解锁并在系统界面处理钥匙串提示。没有重跑已通过的本地测试，也没有把环境记录计为新增功能或验收通过；最新实现/回归仍为v50。

阻塞审计：上一轮为实际进展；本轮无进展，是本次连续环境阻塞的第1轮，尚不满足标记blocked的三轮门槛。完整Goal保持active。待Mac解锁后优先完成真实云模型内容和剩余Computer Use；APNs部署/密钥、可用物理iPhone与第二台Mac仍属必要外部条件。证据保存在`target/norma-ios-evidence/v51/`。

2026-09-08 20:07 CST第2轮环境复核：Computer Use仍明确报告Mac锁屏，iPhone 11仍unavailable，Tailscale仍Stopped。上一轮与本轮均无进展；未重复读取钥匙串、重启已结束测试或更改用户配置。现有解锁请求待答复，连续阻塞计数为2，Goal仍active。

2026-09-08 20:08 CST第3轮环境复核：锁屏、iPhone不可用与Tailscale停止的条件持续，v50通知相关源码哈希未变化。连续3轮无进展且无可独立完成的剩余真实验收步骤，已调用Goal工具标记blocked；没有缩减G1–G5范围，也没有标记完成。恢复时先解锁Mac并处理系统钥匙串提示，再继续真实模型内容与Computer Use，随后完成物理双机/跨网和APNs锁屏验收。

## v52 · 项目成果入口与通信延迟

用户要求先推进其他功能、之后统一验收，并在Goal中增加低延迟/高性能通信要求。Goal恢复并保持active；v51的blocked只作为历史环境记录，不再因锁屏反复停止独立开发。

项目页新增「项目成果」，复用已有SwiftUI成果列表/预览/验收。服务的node过滤变为可选，旧单卡请求继续严格限定卡片；项目范围沿用电脑身份、项目授权与当前窗口双重复核。新增(workspace_id,sequence)索引，20项倒序游标分页，列表只传摘要；删除卡片后仍能发现已冻结成果、读取原文件并保留验收记录。项目关闭或离线仍不可读取，完整归档聊天入口未接入。Apple Design落实为黑色原生列表、语义字号、44pt入口、明确来源与只读/错误反馈，页面Computer Use按用户要求后置。

实时项目/聊天流复用持续WSS，将250ms固定合并与50ms空闲轮询改为条件变量唤醒：活跃突发最多每50ms读取一次，空闲仍每2s回源鉴权；关闭/撤销会唤醒等待。所有数据读取与编码保持后台，锁只保护版本计数。小帧启用TCP_NODELAY。原生Swift测试另行复现了慢页面消费时的假eventGap；现由ProjectTimeline逐条校验/应用原始增量，界面缓冲只保留最新完整快照，恢复事件清除旧状态，不跳过协议序号校验，也不增大无界队列。

失败→修复证据：项目级查询原先invalid_request；本机事件初测290.84ms，改后独立20次采样p50 61.60ms、p95 75.04ms、最大80.54ms，均低于本机150ms预算。完整并行回归曾测得2428.72ms，因此时延门槛放在显式独立性能检查；普通功能回归仍核验实际关闭事件/授权/旧状态清理。该高负载记录保留，不能声称高负载或物理LAN/Tailscale指标达标。另有SQLite内存诊断：21份各600kB原文件的独立成果，首页20项摘要查询均值70.06ms、响应约6.4kB；这不包含磁盘/网络延迟，也未引入推测性的缓存或连接池。

本轮自动化：Storage92、Swift36、全workspace1740通过/0失败/65忽略；独立本机性能20次通过，通知相关旧证据未重复计数。原生macOS Swift→正式WSS→headless GPUI的卡片删除后600kB冻结成果/验收回查1.61秒通过；慢消费者暂停4.5秒后恢复、真实创建/删除空卡、降权/取消14.78秒通过。原生客户端凭据与隔离fixture正常清理，无Provider任务。模拟器Release与未签名iPhone SDK构建通过；App check与Mac Release构建通过（2m20s）。没有重装用户模拟器或重启已配对桌面PID21316，真实页面与真机验收后置。证据目录：`target/norma-ios-evidence/v52/`。

## 受控会话归档身份与只读路径验证（v53）

新增轻量`mobile_sessions`索引，复用现有MobileTurnStore的AppDB/项目范围以及审批后台writer，不复制聊天正文。保存准确node/profile/run/session、启动时项目路径与标题；同run重复登记幂等，换节点/Provider/session冲突回滚。相同Provider会话重开时保留各次独立run；20项倒序索引分页，不通过“最新会话”扫描认领身份。

正式启动已接入：Codex收到thread/start或thread/resume的准确ID后，在turn/start之前提交身份；Claude在创建PTY前提交身份。写入失败不能继续发送首条模型任务。Claude只从当前run/session的已验证Hook追加文件引用，同run禁止换路径；引用经既有有界writer提交，删除卡片不清理已保存索引。Hook处理方法移入既有claude职责文件，保持Rust文件≤500行。原实时历史接口和旧卡片恢复语义未放松。

验证先复现Codex启动后无索引、Claude删卡后路径丢失，再接入生产流程。存储三项检查、准确启动顺序/冲突拒绝、Claude立即删卡后台提交均通过。正式App的Codex真实首条任务、WSS历史、关闭、重开及删除后两份run索引通过；Claude真实批准/拒绝、完整正文与重开通过（22.68s），空闲原生启动通过（3.97s）。这些是headless GPUI+真实CLI，不是页面或真机验收。

另以真实Codex执行一个无工具回合，关闭原app-server，再开独立只读app-server，仅使用initialize、thread/read(includeTurns=false)、thread/turns/list(itemsView=full)：完整回合与关闭前相等，仍只有原回合，metadata状态notLoaded，无thread/resume或turn/start。单次本机冷启动读取328.93ms；这是协议可行性证据，不是正式归档API或性能分位数。两次测试进程均已退出。

回归发现两处测试夹具缺口并已修复：Claude中断echo夹具遗漏新的启动身份登记；旧真实Claude E2E仅进入画布，没有走正式项目身份/路由注册，因此启动报workspace unavailable。修复夹具后重新运行，不放松生产检查。第一次Claude精确测试名写错而执行0项，未计入验证证据。

当前覆盖新增或显式重开的受控会话，不自动迁移已结束旧会话。Claude引用只有收到Hook且后台commit后才有持久化证据；未验证进程在入队至提交之间被强杀的窗口。没有新增Provider正文备份、桌面守护进程或归档手机页面；文件被Provider清理后的恢复不在本轮证据内。后续需接只读鉴权/分页传输、复用SwiftUI正文、明确旧记录缺失与文件不可读状态。

全workspace回归1744通过/0失败/65忽略；随后新增启动顺序测试单独通过。App check与Mac Release（2m15s）、行数/allow/存储/性能/指南/diff护栏通过；最终Runtime/E2E复验376通过/0失败；精确命令见连接证据`session_archive_index_v53`。iOS源码未修改或重新安装；用户已配对桌面PID21316仍运行，统一界面/真机/APNs与跨网络验收继续后置，Goal保持active。

## TestFlight首次发布准备（v54）

用户明确要求发布iOS到TestFlight，优先于下一段归档聊天页面。已确认Team `2BF8G9AN4L`、Bundle ID `com.norma-ai.os.ios`、Apple Distribution证书和环境中的App Store Connect API凭据可用；未输出或复制私钥到仓库。开发者网站已有专用Bundle ID，但该API账号查询到的App Store Connect应用列表中无对应App。

已生成`0.1.0 (1)` Release归档与App Store分发IPA，4MB以内。本地验证通过：正式分发描述文件、`get-task-allow=false`、`aps-environment=production`、`beta-reports-active=true`、准确Team/Bundle ID与codesign严格校验。补充隐私清单（自有UserDefaults与容器文件访问），图标按发布要求合成到黑底去掉透明通道，未重绘品牌内容。iOS工程重新生成并完成最终归档/导出；没有重装用户模拟器或重启桌面实例。

发布尚未完成。Apple API明确拒绝`POST /v1/apps`：apps资源仅允许读取/更新，不支持创建；Computer Use报告Mac锁屏且自动解锁失败，无法用网页创建记录。已向用户请求解锁或告知手动创建结果，并为当前任务排队打开App Store Connect入口。`altool --validate-app`虽返回进程exit0，但日志明确ERROR12：无法根据Bundle ID查到Apple ID，因此此项按失败记录，没有把退出码当作上传成功。

下一步：在App Store Connect创建App（iOS，名称Norma，主要语言简体中文，Bundle ID如上，SKU norma-ai-os-ios）；查询真实Apple ID和构建号后上传已导出的IPA，等待处理，处理实际加密问卷并核验TestFlight可测试状态。配对模块包含SPAKE2与ring AES-GCM，不能直接声明“仅使用Apple系统加密”；本轮未替用户填报出口合规或宣称豁免，也没有提交外部Beta审核、邀请测试人员或发布到公开App Store。

产物、签名和未完成状态见`target/norma-ios-testflight/package-evidence.json`与[发布记录](2026-09-08-norma-ios-release-records.md#release-testflight)。这是本轮新增实际进展，完整Goal继续active，未标记完成或blocked。


## 项目会话记录与原生完整聊天（v55）

用户已取消TestFlight发布。签名IPA仅保留为冻结产物；不创建App记录、不上传、不发送测试邀请，解锁后也不自动恢复发布。

项目新增「会话记录」原生入口，读取既有20项倒序SQLite索引；删除卡片后仍可按准确Provider/run/session进入原始对话，不暴露Provider磁盘路径。正文使用独立archived_conversation请求，复用既有分页和32MiB有界分片；原实时conversation仍要求当前卡片/run/session，未放宽为归档请求。前后项目/机器授权与路由校验、大正文分片期间授权和最终路由复核保留。

Claude直接复用已登记的文件引用和完整JSONL读取器；丢失文件/身份不符明确失败，不扫描最新文件或改用摘要。Codex在需要时启动短时只读app-server，验证thread/read返回的ID与cwd，再使用thread/turns/list(full)读取；不调用thread/resume/turn/start，也不附着TUI。启动和诊断消费复用原有受控PTY所有权；最多8个读取对象，Runtime关闭、释放及换项目会撤销，完成后回收host，没有常驻执行服务。

SwiftUI复用原有聊天消息行，保留全文选择和工具详情展开；会话列表/详情均为黑色原生导航、紧凑语义字号与44pt入口，满足Apple Design的内容优先、明确来源与直接反馈。原始对话页只读，按Provider的历史方向分页；刷新、断线、撤权与后台切换清理旧内容。用户已配对的桌面PID21316和用户模拟器未重启/重装；只操作独立验收模拟器。Computer Use仍报告Mac锁屏，页面点击与完整无障碍按用户要求统一验收，不把XCTest当成Computer Use证据。

测试先取得归档入口缺失、正文不可读、Codex关闭后不可读、Runtime释放后读取未撤销的失败结果，再逐项接入。正式GPUI/WSS读取删除卡片后的7000字Claude文件fixture，拒绝错会话、旧实时接口、缺失文件、越权项目与失效route。原生macOS及实际iOS模拟器又验证两条合计621000字节的正文（第二条600000字节），走真实多帧传输且逐字一致；fixture不冒充真实Claude生成。真实Codex首条任务、原生终端、关闭、重开、删卡和只读原文比较在25.87s通过，无新增回合或残留归档host。

全workspace1747通过/0失败/66忽略；Swift37通过；iOS DeviceTests44通过/4专项跳过/0失败；独立iOS归档传输3项通过（整轮含编译启动40.77s，测试本身0.254s），macOS原生完整场景1.49s。这些时间不是物理LAN/Tailscale的延迟分位数。回归中因抽取诊断helper暴露了旧测试依赖父模块PtyEvent导入的问题，已改为测试显式导入并重跑全仓。App check、模拟器Release、未签名iPhone SDK与机械护栏通过；Mac Release通过（2m24s）。完整证据见连接证据session_archive_mobile_v55及target/norma-ios-evidence/v55/summary.json。

当前归档覆盖已登记的受控/显式重开会话，不补造未登记的旧历史，也不备份Provider原始文件。旧记录发现、文件被清理后的恢复策略、后台提交前强杀窗口、真实规划质量、APNs和物理双机/跨网/完整界面验收继续保留，Goal未完成。

## 通信分阶段性能采样（v56）

将既有单次项目延迟检查扩展为一次命令自动跑20个独立场景，保留原有授权/项目隔离/关闭后无旧卡片与无执行器断言。每次分别测TCP/TLS/WebSocket握手合计、匹配Ping/Pong的已建连往返、初始授权快照、项目切换到客户端收到关闭事件；使用同机单调时钟，不跨设备相减时间戳。保留全部原始样本，nearest-rank p50为第10项、p95为第19项，没有剔除最慢项。仍逐样本执行150ms更新门槛。

| 毫秒 | p50 | p95 | 最大 |
|---|---:|---:|---:|
| TCP/TLS/WS握手合计 | 23.57 | 26.39 | 26.80 |
| 已建连WebSocket往返 | 0.048 | 0.069 | 0.077 |
| 首份授权项目快照 | 6.40 | 11.13 | 12.32 |
| 项目变化到收到更新 | 60.38 | 65.66 | 68.35 |

独立性能检查1项通过（20次，整项6.92s），完整E2E 231通过/0失败/37忽略，行数/no-allow/diff护栏通过。仅改测试与文档，未修改生产路径、iOS界面或重启用户实例；沿用v55正式构建与原生测试证据，未重复全workspace。日志和原始样本见`target/norma-ios-evidence/v56/summary.json`及连接证据`transport_latency_sampling_v56`。

本轮范围是本机Rust客户端、headless GPUI、真实SQLite/WSS。快照/更新包含授权、调度、序列化和网络处理，握手为三个阶段合计，不能把差值当作纯应用或纯TLS耗时。审批提交、聊天首包、空闲CPU/流量、SwiftUI呈现和物理LAN/Tailscale仍未测；不据此勾选G5。真实云规划仍缺已解锁配置的实际调用证据，本轮没有再次尝试小模型、读取Keychain、改模型或ACL。用户安排的统一界面/真机/APNs验收继续后置；TestFlight保持取消，Goal未完成。

## 真实Provider与原生通信采样（v57）

已有原生消息/审批验证可通过`NORMA_MOBILE_MEASURE=1`增加测量，开关只在隔离测试中生效，不进入正式iOS App。每种Provider分别20次准确读取同一待审批请求、20次读取已经完成的真实短对话；不为采样新增模型任务。原两次拒绝/批准分别记录本机保存→请求接收，以及保存/恢复→持久化submitted回执；原任务的精确磁盘效果和防重放断言保留。记录完整assistant消息首次实际到达订阅的时间，包含模型生成，不能称作纯网络延迟。

| 独立iOS模拟器测量 | Codex | Claude |
|---|---:|---:|
| 审批列表读取p95（各20次） | 29.23ms | 31.68ms |
| 已完成聊天页读取p95（各20次） | 78.12ms | 36.23ms |
| 两次真实审批持久化回执 | 51.34 / 202.41ms | 186.82 / 180.90ms |
| 一次新回复出现（含模型生成） | 4562.30ms | 3442.52ms |
| 单条聊天订阅空闲6s，客户端单核CPU占比 | 0.286% | 0.283% |

读取包括Keychain、新建TLS/WSS、正式服务/Provider历史及整页解码；不是首字节或第一分片计时。审批回执包含100ms查询轮询和新连接；submitted不是工具完成，2次样本不能证明稳定分布。CPU来自原生probe/XCTest进程的getrusage，不包括服务端、完整SwiftUI或物理iPhone；网络字节未测。所有原始样本和nearest-rank计算结果均保留。macOS批次的Codex审批末尾与Claude聊天启动有约2s重叠，不能宣称该批次为完全空载；iOS四个实际场景严格串行。

红测先证实旧probe没有采样结果。首次直接运行Codex审批遇到4项外部Hook提示，未批准或绕过；改用用户此前明确授权的现有隔离脚本，正式产品Hook不变。Claude首次调用缺少target/debug PATH，未找到Hook；按已有命令补齐并构建Hook后复验通过。这两次环境调用失败保留在证据中。

最终真实原生场景macOS4项/iOS4项通过；E2E231通过/0失败/37忽略，Swift37通过，iOS44通过/4专项跳过；行数/no-allow/diff护栏通过。仅改测量用测试代码与文档、重新生成测试工程；不重建发布IPA、不更改生产协议/页面，用户桌面PID21316保持运行。完整数据见连接证据`native_transport_sampling_v57`和`target/norma-ios-evidence/v57/summary.json`。

源码另确认实际等待来源：`CommandCenterView.observe`每轮读取全部电脑和项目审批后再sleep3s，尚未接入已存在的变更事件。下一步应复用WSS与既有变更信号完成指挥台实时订阅，保留多电脑隔离、断线清空、撤权和旧请求拒绝。真实云规划、首包/纯TLS/服务端CPU/流量、物理LAN/Tailscale、APNs和完整界面仍未验收，Goal不能完成。


## 指挥台实时审批订阅（v58）

正式SwiftUI指挥台已删除每轮全量读取后sleep3s，改为每台电脑一条持续WSS汇总全部授权项目。复用现有项目变更信号、50ms突发合并和2s鉴权心跳；心跳不重传工具正文。首次sequence0完整基线，实际内容变化递增，客户端逐帧校验后仅合并完整状态。断线、换流、序号缺口或降权清除旧输入，最多1/2/4s只读重连，不重放决定；各电脑独立，离开页面或进入后台取消消费。

目录、全部项目审批与最终路由核验共享4s预算，不按项目数累加超时；整份响应上限500KiB，单项目错误为空items并显示原因。读取期间项目关闭或授权失效丢弃旧数据；空卡数量或名称改变仍保留有效路由，采用最终当前元数据。新连接沿用access→sockets锁顺序标记手机，撤销不会漏掉订阅。原准确审批身份、手机决定持久化、只读权限及外部Hook边界保留。

TDD先取得旧服务不支持watch_approvals和Swift缺少类型的红测；增加读中元数据变更检查又发现空卡数量变化导致误断流，修复为只以workspace/state/route判断失效后通过。真实WSS/SQLite检查涵盖授权范围、关闭项目、心跳无正文、清空、路由竞争与撤销。独立低负载单次更新65.329ms通过150ms门槛；此处为Rust回环客户端和前台fixture，不是物理网络或SwiftUI性能。

实际Codex/Claude保持同一原生审批流，完整拒绝/批准、精确磁盘效果、防重放和移除在macOS两项48.17s、iOS两项64.92s通过。项目与审批同时订阅的降权重连、完整基线和取消在macOS14.69s、iOS22.63s通过；保留正常外部Hook的iOS信息提示与停止24.91s通过。Codex客户端审批检查仍仅使用此前授权的临时隔离Hook脚本，正式配置不变。这些用时包含模型/测试宿主启动，不当作网络响应分位数。

全workspace1748通过/0失败/67忽略，其中E2E232通过/38忽略；Swift38通过；iOS DeviceTests45通过/4专项跳过，现有导航5通过。App check（5.06s）、Mac Release（2m 22s）、模拟器Release与未签名iPhone SDK构建通过；行数/no-allow/存储/性能/指南/diff护栏通过。证据见[连接记录](../design-docs/2026-09-07-norma-ios-connection-evidence.json)的approval_feed_v58和`target/norma-ios-evidence/v58/summary.json`。

界面沿用Apple Design的黑色、紧凑语义字号、原生分页/滚动和44pt操作，只更新实时状态反馈；没有本轮Computer Use点击或VoiceOver验收。用户桌面PID21316仍是旧构建，未重启，用户模拟器未覆盖，统一验收时才升级。真实云规划、首分片/服务端CPU/字节指标、物理LAN/Tailscale、APNs和完整界面仍待完成；未登记旧历史策略不擅自迁移。TestFlight保持取消，Goal active。


## 空闲心跳流量与聊天首分片（v59）

沿通信指标检查发现实际浪费：项目和聊天订阅都在每次全局唤醒后发送心跳，即使变化来自其他项目/设备且当前内容未变。新增真实WSS回归先失败：30次无关唤醒在3.36s内使两条流各发送30帧。现只在投影变化或距上次发送达到2s时发送；共享等待计算剩余保活时间，避免无关事件暂停后把心跳推迟。读取仍核验当前授权/运行，真正发送前保留最终复验，实际内容仍立即发送，序号不虚增。

修复后同一场景项目392字节、聊天333字节，各1帧；合计21750→725字节，减少96.7%。计数为排除基线的UTF-8应用消息，不包含TLS/TCP，也不是物理iPhone或服务端CPU数据。全局信号仍可触发其他订阅的后台读取，没有引入缓存或新的订阅路由框架。后续CPU优化必须以独立测量为依据。

长响应既有无损测试新增独立20次采样，每次均保留准确中文/工具内容、分片顺序、尾片前换run拒绝和撤销检查。首个完整WSS分片在JSON解析前计时，p50/p95=24.04/45.36ms；整页重组并核验p50/p95=71.28/139.79ms。每页43帧、4809022字节应用JSON，握手独立记录；不把整页耗时当首包，不把冻结fixture当模型首token、原生UI或物理网络。项目更新20样本仍全部<150ms；审批更新单次78.487ms通过同一门槛，未宣称比v58更快。

真实Codex/Claude连续两轮消息与事件在macOS2项22.65s、iOS2项39.41s通过。原生聊天恢复/撤销macOS5.39s和iOS13.93s通过；iOS项目/审批同时订阅降权与取消22.91s通过；iOS大正文与大事件7.94s通过。这里共8个原生场景，测试总用时含模型与宿主启动，不能作为网络分位数。没有修改手机源码，使用v58原生客户端连接本轮Mac服务。

最终全workspace1749通过/0失败/68忽略，其中E2E233通过/39忽略；App check（3.21s）、Mac Release（2m 14s）及行数/no-allow/存储/性能/指南/diff护栏通过。Swift/iOS源码未变，沿用v58完整客户端构建与单测证据，本轮另以新Mac服务完成上述8项原生场景。完整结果见连接证据idle_watch_and_fragment_v59及`target/norma-ios-evidence/v59/summary.json`。真实云规划、服务端CPU/TLS字节、物理LAN/Tailscale、APNs与完整界面/无障碍继续待完成，G2–G5不能勾选完整验收。用户旧桌面PID21316和用户模拟器保持原状，TestFlight保持取消，Goal active。

## 原生首分片与独立服务线程测量（v60）

仅扩展现有隔离测试：复用正式PinnedConnection、Keychain、响应校验与分片解码，每个平台20次独立连接并逐字核验完整中文/工具fixture。首分片在完整Data到达、WireReply解码前计时；整页计时包含43帧重组与完整内容校验。准备和TLS/WSS-ready独立记录，后者含ready Ping/Pong，不等同于纯TLS。没有向正式App加入计时API或遥测开关。

| 毫秒（最终批次，全部样本保留） | macOS p50 / p95 | iOS模拟器 p50 / p95 |
|---|---:|---:|
| Keychain与请求准备 | 1.81 / 2.13 | 0.47 / 0.49 |
| TLS/WSS-ready | 12.34 / 28.60 | 17.00 / 24.77 |
| 首个完整分片 | 25.39 / 45.67 | 24.09 / 25.14 |
| 全文重组校验 | 134.81 / 191.47 | 123.99 / 126.43 |

每页4810140字节应用JSON；不是模型首token或TCP首字节。两平台采样依次执行，采样窗口内没有并行构建/回归，但未控制其他系统后台负载，不丢弃较慢样本。

大页基线后由独立Task持续消费公共conversationUpdates，经过1s稳定期，再由服务owner启动有界系统采样。libproc按准确线程句柄和名称分别记录norma-mobile-listen/peer，拒绝线程消失、计数回退或额外peer；线程CPU单位已用100ms忙算与独立getrusage交叉验证。nettop仅监控隔离测试PID，保留原始CSV，用两次累计值相减排除首个大页。

约6s内两平台各收到3次后续心跳，无恢复/提前终止；服务网络线程总单核占比macOS0.522%、iOS0.520%，系统TCP计数均发送1158字节、接收0字节。这里是加密连接的TCP字节计数，未包含完整TCP/IP线速开销；CPU窗口含nettop启动/退出，与网络两次快照并非完全相同。CPU排除fixture工作线程、GPUI和Provider，不能声称完整服务开销。客户端测试宿主占比0.265%/0.920%，包含大页timeline合并及100ms测试协调轮询，不代表完整SwiftUI或物理iPhone耗电。

首分片和空闲阶段分别先取得旧probe缺少标记的红测，再通过macOS完整场景11.54s、iOS场景22.60s（含宿主启动，XCTest3项通过）。Swift38通过；iOS DeviceTests45通过/4专项跳过；最终E2E233通过/39忽略。第一次整套E2E中“关窗取消规划”遇到3s socket WouldBlock，精确单项0.34s及完整复跑通过，未改生产代码或放宽超时；根因未重现，不声称已修复，失败日志保留。

本轮只改测量/测试与文档，重新生成并编译原生测试工程；沿用v59生产Mac check/Release/workspace与v58 iOS生产构建，不重做发布。行数/no-allow/指南/diff护栏通过。完整数据见连接证据native_fragment_and_idle_v60和`target/norma-ios-evidence/v60/summary.json`。用户桌面PID21316未重启、用户模拟器未覆盖；真实云规划、APNs部署/锁屏、物理LAN/Tailscale和完整Computer Use/无障碍继续待完成，TestFlight取消，Goal active。

## 聊天心跳不重复合并正文（v61）

沿v60的空闲消费路径检查，发现原生ConversationEventSequence在心跳不带page时仍返回缓存完整页，SwiftUI每2s再次调用ConversationTimeline.merge，重建历史数组。现通过原有身份/流/序号校验的空心跳返回独立heartbeat；页面继续消费但不更新正文，完整基线与真实内容仍立即合并。项目订阅仍刷新连接核验时间，取消、撤销、缺口与重连清空路径保留；没有改Mac协议、心跳周期或新增缓冲/遥测。

TDD先确认旧实现心跳返回完整页导致单测失败，原生持续订阅也因重复内容回调失败。补充无效心跳序号、无基线心跳拒绝及缓存清空断言。修复后macOS和iOS分别约6s收到3次有效心跳、0次内容事件/全文合并，完整中文与工具正文仍相等。系统TCP发送均为1158字节，说明本轮改变客户端处理，未减少服务端心跳。原始CPU样本保留，但单个6s窗口不能证明稳定CPU收益、SwiftUI帧数或物理耗电改善。

macOS/iOS大正文场景11.23/22.77s通过；真实Codex/Claude连续消息各2项，在macOS23.45s、iOS38.17s通过；恢复/撤销macOS5.41s、iOS13.48s通过，共8项原生场景。消息内容准确、无重复发送，后台/取消与新基线检查沿用正式客户端；用时包含模型/宿主启动，不作为网络分位数。

Swift38通过；iOS DeviceTests45通过/4专项跳过；完整E2E233通过/39忽略，v60关窗规划超时本轮未重现，未宣称其根因已修复。模拟器Release和未签名iPhone SDK构建、行数/no-allow/指南/diff护栏通过；Rust生产代码不变，Mac构建沿用v59。证据见连接记录native_chat_heartbeat_v61与`target/norma-ios-evidence/v61/summary.json`。

本轮没有布局/样式改版、Computer Use或真机验收；0次正文合并不等同于0帧重绘。用户旧桌面PID21316未重启、用户模拟器未覆盖，TestFlight取消。真实云规划、APNs部署/锁屏、物理LAN/Tailscale与完整界面/无障碍仍待完成，Goal保持active。

## 真实撤回后的Todo重新安排（v62）

删除原生Todo场景中人工写入prior-phone/prior-unsent/card_changed的前置状态。复用已有真实无工具生成使Agent忙碌，手机使用正式prepareTodo/提交/回查将首项入队，确认尚未派发且不可重排；再调用cancelTodoQueue取得真实cancelled状态、完整原文和可重排标记。原生停止准确生成回合，确认同run/session保留，重排前历史中没有任何Todo指令，再明确重新安排首项。

随后拒绝第二项、按原队列发送第三项。最终完整user历史为原生成及Provider中断记录，加两条准确Todo原文；两个不同真实turn完成，拒绝项无执行结果。重排关联原手机/操作ID，重放原入队与撤回仍得到原queued回执和cancelled状态，不能覆盖新尝试或中断新任务；已送达/已拒绝任务不能再次重排。原取消与全部后续操作均走正式WSS/GPUI/Runtime/SQLite，不再预写失败；计划的三项内容仍是固定fixture，不证明Norma云规划质量。

先取得旧fixture被“初始不应有决定”断言拒绝的红测0.83s（尚未启动Provider）。改用真实前置后，Codex/Claude在macOS两项24.77s、独立iOS两项45.67s通过。完整E2E233通过/39忽略，Swift38通过，iOS DeviceTests45通过/4专项跳过，行数/no-allow/指南/diff护栏通过。只修改测试和文档，正式产品代码不变；使用v61 iOS Release/未签名iPhone构建与v59 Mac Release，不重新打包或发布。

原始日志与状态断言见连接记录native_todo_cancel_retry_v62及`target/norma-ios-evidence/v62/summary.json`。四个场景是真实Provider与原生客户端的回环验收，不是SwiftUI点击、物理双Mac/iPhone/Tailscale或锁屏验收，测试总用时包含模型和宿主启动，不能当网络延迟。用户桌面PID21316及用户模拟器保持原状；真实云规划、APNs部署与完整统一验收仍待完成，TestFlight取消，Goal active。

## 统一验收环境复核（v63，无实现进展）

2026-09-09 01:34–01:35 CST：Computer Use报告Mac锁屏且自动解锁失败；已登记iPhone 11为paired但tunnelState=unavailable；Tailscale为Stopped，在线peer为0。用户桌面PID21316仍为原实例，未重启或覆盖用户模拟器。

仅检查当前任务中APNs和NORMA_LIVE环境变量是否存在，均未设置；已知relay默认密钥文件不存在。这不表示全机器没有配置。未重试需要系统交互的Keychain读取、调用云模型、发送APNs或改变VPN配置；relay部署位置仍未提供。

本轮无代码变更或新增测试/验收结论，最新有效回归仍为v62。真实云规划、完整SwiftUI/Computer Use与无障碍、物理双机/LAN/Tailscale及APNs锁屏继续保留。v62是实际进展，本轮是其后的首次无进展环境核验，Goal保持active，未达到连续三轮blocked条件。证据：`target/norma-ios-evidence/v63/summary.json`；TestFlight保持取消。

2026-09-09 01:40 CST第2轮实时复核：Computer Use仍报告Mac锁屏，iPhone 11的tunnelState仍为unavailable，Tailscale仍Stopped且无peer；PID21316保持原实例。前轮与本轮均属无进展，连续计数为2，没有运行中测试可等待；未重复测试、访问Keychain或更改用户配置，Goal继续active。原始证据：`target/norma-ios-evidence/v63/environment-recheck-2.json`。

2026-09-09 01:41 CST第3轮实时复核：Mac仍锁屏，iPhone 11仍unavailable，Tailscale仍Stopped且无peer；当前任务的APNs/云模型环境仍未设置，未访问密钥值或重试Keychain。连续三轮无进展且剩余验收需要外部条件改变，已调用Goal工具并确认status=blocked。完整G1–G5目标保留，未标记完成；最新回归仍v62，无新增测试，用户桌面PID21316未重启。恢复入口为解锁Mac并连接验收设备/网络，随后补齐推送部署与正式模型配置；证据：`target/norma-ios-evidence/v63/environment-recheck-3.json`。

## 功能优先收口（v64）

用户要求先完成功能，再统一验收。本轮新增原生项目/卡片搜索、状态及Agent交集筛选，支持任务标题、多词、本地化大小写与重音匹配；只读取当前已授权快照，不新增网络请求或结果缓存。实时增删后筛选结果随快照更新，切换电脑清除上台条件；空结果与无项目/卡片分开。

Todo新增默认待办/全部记录：已完成、未采纳退出待办；送达、排队、撤回、失败、停止及未知结果仍需跟进。未决本机操作继续可见，原计划、决定、回执和成果不删除，执行完成不等于验收。Apple Design落到系统搜索框、菜单Picker与分段控件、紧凑语义字号、黑色背景和44pt清除入口。

Swift40通过；iOS DeviceTests47通过/4专项跳过；原生macOS/iOS实时搜索各1项通过，macOS Codex真实Todo1项通过。完整E2E首次212通过/1失败/31忽略，失败为规划用例未取得当前route；精确单项0.46s及完整复跑233通过/39忽略均通过，未改生产Rust或放宽超时，根因尚未确定。iOS模拟器Release、未签名iPhone Release及常规护栏通过。新增搜索UITest已留待统一验收，不将原生API或HostingView测试冒充Computer Use。证据：`target/norma-ios-evidence/v64/summary.json`。

iPhone诊断进一步确认：当前USB枚举没有iOS设备；Xcode仅有已配对记录、tunnelState=unavailable、ddiServicesAvailable=false，保存的developerModeStatus=disabled。最后连接记录为2026-09-06 20:18 CST；离线详情可能缓存，不能断言手机此刻的开发者开关状态或Norma安装情况。后续Xcode真机验收需数据连接、信任及开发者模式，当前不阻止功能开发。

本轮是实际实现进展，连续无进展计数归零；用户桌面PID21316未重启，未覆盖用户模拟器。完整Goal未完成，正式云规划质量、APNs部署与真机/界面统一验收保留，TestFlight取消；不再以反复轮询外部条件替代功能推进。

## 规划route间歇失败诊断（v65，无生产功能变更）

针对v64首次E2E中“当前route”缺失，增强原断言的失败上下文，记录响应status、error和测试项目快照，不输出完整grant或配对token。三个规划fixture用例并行运行30轮（90次），以及原完整库级E2E连续3轮（每轮213通过/31忽略），均未复现；这里的库级检查不包含独立集成target，完整生产回归仍沿用v64。

本轮未定位根因，不声明修复，不放宽生产超时、关闭权限核验或增加静默重试。保留可重跑命令与逐轮日志，停止没有新证据的重复测试；最终诊断改动的聚焦测试和行数/no-allow/diff护栏通过。证据：`target/norma-ios-evidence/v65/summary.json`。

未轮询真机或锁屏状态，未访问云凭据，未重启用户桌面或覆盖用户模拟器，TestFlight仍取消。首版功能与外部验收继续按v64表格分开跟踪，完整Goal未完成。

## 计划/Todo实时订阅与TestFlight恢复（v66）

计划与执行页现复用持续WSS接收跨手机决定、队列撤回/派发和准确回合终态，删除2秒整份计划轮询。服务器在持久化后唤醒已有变更信号，50ms合并、2秒无正文心跳；后台认证读取继续支持已关闭项目的历史计划。手机验证项目/计划/手机/授权版本、流身份和连续序号，缺口清空并有限只读重连，不重放决定。UI只让有序流更新可见状态，操作后的单次查询仅核对本机回执，避免晚到响应覆盖新进度。

Swift41通过，iOS48通过/4专项跳过，完整E2E233通过/39忽略；真实WSS覆盖持续6秒心跳、另一手机决定、撤回、排队终态、项目隔离和撤权。macOS/iOS原生Codex Todo分别20.62s/29.63s通过，终态通过新订阅获得。最初两次macOS原生检查在中断后校验失败；只补非敏感诊断后通过，根因未定，不称为生产修复。常规护栏与App check通过，源码与日志见`target/norma-ios-evidence/v66/summary.json`。

用户本轮重新授权尝试TestFlight，并指定Chrome。已通过Chrome创建Norma应用记录（6810038463，既有Bundle ID）；最新0.1.0(1)归档/分发签名/导出/Apple校验和上传通过，Apple处理为VALID。用户确认不在法国分发后已保存加密问卷；「Norma 内测」含1个构建与1名测试员，当前账号已按用户授权收到邀请，内部状态IN_BETA_TESTING；中文测试说明已保存。后续以[发布记录](2026-09-08-norma-ios-release-records.md#release-testflight)为准，不再将旧取消状态视为当前授权。没有重启用户桌面或覆盖用户模拟器；APNs部署、正式云规划质量、物理设备/跨网及完整界面验收仍未完成。


## v69 · 用户纠正：复用原生 Agent 卡片（实施中）

- 用户已明确否定 v67 手机启动占位和 v68 桌面“开始对话”表单。原卡片/终端是唯一操作主体，iOS 将消息输入同一终端并读取相同会话结果。
- 创建卡片可启动 CLI 到待输入状态，但不能自动派发任务。旧卡不应为了手机输入被要求重开或转成另一套会话。
- 保留身份校验、授权、消息去重、电脑草稿保护和现有安全策略；取消把“创建卡片不派任务”解释成“不能启动终端”。
- v68 表单测试属于被替代方案，不再算最新交互验收。v69 已补原生卡片远程输入回归，先复现 mobile_control_required，再验证修复；真实 CLI 与手机完整历史验证仍在进行。


### v69 原卡输入闭环与旧 Hook 回归

移除桌面开始对话表单，手机新卡使用原有 TermLive 和原生 CLI，旧卡自动登记同一会话观察。Codex 初始化画面也有空提示符，因此就绪校验必须等 model/directory 加载完成；输入仅 Paste 后提交一次 Enter，等待准确 Provider 回执。Codex 原生提交 trim 首尾空白，Claude 仅允许原生 Tab 展开，其余正文严格核对。后台缓存 Codex 只读历史连接并持久化原生会话及准确回合成果；不另开执行会话。

Codex / Claude 各连续两回合经真实 CLI + macOS Swift TLS/WSS 客户端通过，同一 PTY/run/session，正文、完整回复、成果、重复去重与旧快照拒绝均有断言。读取已有历史各20样本：Claude P50/P95=23.01/29.34ms，Codex=70.70/81.66ms。这是本机回环测量，不是物理 iPhone、Wi-Fi 或 Tailscale 指标。证据：`target/norma-ios-evidence/v69/native-final-measured.log` 与两份 `*-native-metrics.log`。

最终全仓测试揭示此前独立 E2E 运行后加入的迟到 Hook 守卫造成3项桌面回归：无 turn_id 的旧 Stop 被丢弃，桌面任务未完成、跨项目结果不可读、续聊被误排队。聚焦旧桌面 E2E 和新 Runtime 测试均先失败；修复仅允许没有已知回合身份的旧 Hook 沿用桌面状态流程，不产生手机准确回执/成果。已知回合仍拒绝缺失、错误及重复 Stop；无需放宽超时或修改旧 E2E 数据。Runtime150通过/3忽略，最终全仓1755通过/73忽略；补丁后真实Codex/Claude两项复验34.64s通过（包含CLI/模型用时，不作网络延迟）。原始失败与修复日志保存在同目录，不能再沿用较早 E2E 通过结果声称最后代码全绿。

原生手机工具审批与停止回执仍未接通：等待确认提示在电脑原卡处理，iOS 按服务端 can_interrupt 能力隐藏未实现的停止操作，旧客户端命令也被服务端拒绝。这个防护只保证不虚报能力，不能替代原型要求。APNs 按用户决定暂不部署；真实云规划质量、物理多机/跨网、完整 SwiftUI/Computer Use 与无障碍验收继续保留，Goal active。


### v70 验证与内测交付

旧Hook补丁的Release构建、严格签名校验与随包Hook测试通过。新包位于`target/norma-mobile-desktop/v70/Norma Mobile Validation.app`；这是沿用本任务的隔离本地验证包，ad-hoc签名，未作为公证DMG对外发布。确认旧桌面无子Agent进程后正常退出，并以相同持久数据目录启动新包；切换前SQLite备份为`target/norma-mobile-desktop/v70/before-switch/app.sqlite`（0600）。已通过Computer Use菜单和原生文件选择框打开原dph-plugin项目，没有恢复用户已删除卡片。

iOS0.1.0(3)沿用相同加密实现及已确认的不向法国分发范围，Apple状态VALID/IN_BETA_TESTING；中文说明明确原卡输入及未接通的原生手机审批/停止、未部署的推送服务。加入原内测组，没有新邀请、扩大测试员或提交外部Beta/App Store。IPA与AppleAPI证据见TestFlight发布记录。

隔离iPhone模拟器已重新启动并打开最新安装的SwiftUI App；未覆盖用户原模拟器。当前隔离App仍待配对。Mac已显示手机连接开启及旧配对记录，但GPUI坐标点击持续noWindowsAvailable，现停留在项目的启动弹层，已请求用户关闭弹层并打开配对新手机页面；不将API测试写成真实界面联动通过。全部本轮日志与聚合结果见`target/norma-ios-evidence/v69/summary-final.json`，Goal保持active。


### v71 原型导航、项目卡片与实时任务

主导航恢复为项目 / 居中指挥台 / 任务；电脑管理提到三个模块的公共入口，选择电脑后返回原页面。项目页把Agent卡片放在记录/成果之前，默认两列、辅助字号一列，品牌图标和运行描边继续复用；创建入口在卡片下方。按Apple设计原则使用原生导航、系统字号和至少44pt触控区，完整搜索能力保留，搜索框提示缩短以避免裁切。

任务页读取当前电脑的全部授权项目，使用已有projectUpdates持续WSS，不轮询或读取完整聊天历史。空白卡片不当作任务；支持项目/全部/进行中/待处理/已完成与多词搜索。点击进入同一原卡对话；列表退场保留导航来源，重新进入先清空并核验。流消费逐次检查电脑与授权版本，授权变化使其他旧订阅失效；相同心跳快照不重复触发列表赋值。当前一项目一条WSS，尚未验证大量授权项目的连接上限；不能宣称已完成聚合性能或物理网络指标。

新增Rust隔离UI fixture第二张空白Claude卡及可控运行/不可读状态，仅在显式UI模式使用，保留原通知目标身份。真实UI红测复现了“项目不可读却显示没有匹配任务”；修复后服务端运行/完成更新、筛选、搜索清除、原卡聊天返回、不可读清空/恢复均通过。无障碍检查还发现搜索提示裁切和电脑入口触控区偏小，修复后默认字号与AX5的文本裁切/触控/描述审计通过；五行输入后的正文空间分别约260pt/117pt，发送保持只读禁用。

证据范围：Swift43通过；完整cargo test -p e2e为234通过/44忽略；最终导航6项、任务联动与对话失败状态2项、搜索/布局/聊天无障碍3项通过；未签名iPhone SDK Release构建通过。Computer Use通过隔离模拟器真实点击项目/原卡/聊天草稿/公共电脑管理并返回，截图保存在target/norma-ios-evidence/v71/cua-project.png、cua-chat.png、cua-computers.png。聊天fixture不提供真实Provider正文，截图中的读取错误不能算真实聊天输出通过。该截图另揭示最终读取失败后仍保留“正在重新读取”提示：新增真实UI红测后删除重复加载文案，加载反馈统一由既有loading状态控制，避免把失败显示为持续等待。首次用于保持模拟器AX桥的Computer Use harness因未及时写结束标记超时；后续真实点击、显式结束及正式忘记测试电脑2项通过，已回收本次只读配对、测试草稿和临时服务。坐标点击依然报noWindowsAvailable，项目/卡片用AX，聊天返回使用原生Command+[。不能宣称整个应用Computer Use验收完成。

用户真实v70桌面保持运行，手机连接已开启，但本轮没有在该桌面新建授权，也没有覆盖用户原模拟器。TestFlight仍为已分发的0.1.0(3)，不包含v71新UI。原生请求级审批/停止、旧卡规划/删除、指挥台计划直接呈现、真实云规划、多机/跨网、完整无障碍和真实桌面联动继续保留；APNs按用户决定暂不部署。Goal active。

后续回归：任务联动与聊天失败提示2项通过；通知系统登记/开关1项与5次本地通知注入组成的暖启动、不同目标、嵌套输入、冷启动、旧窗口拒绝1项均通过。它们不连接真实APNs投递服务，不改变用户暂不部署的决定。最终未签名iPhone SDK Release构建通过。补拍聊天错误修复后的Computer Use截图时Mac再次锁定，故只结束AX桥会话；该次harness结束不作为新的Computer Use通过证据，最后视觉确认仍待补。

两轮隔离fixture均已通过正式忘记流程清理配对，并显式结束服务；未留测试电脑或草稿。最新逐项结果见target/norma-ios-evidence/v71/summary.json。

## v72 · 指挥台直接计划与原生卡删除（2026-09-09）

- 指挥台默认显示当前电脑/项目的最新已保存计划，直接复用ProjectTodoView的确认、拒绝、持续WSS进度、回执与安全重排；项目Menu切换和调整方向复用ProjectPlansView。决策分段仍汇总全部电脑，通知聚焦进入该分段。读取错误、未授权项目、空计划分别反馈。最新计划在进入、关闭调整方向页和刷新时重读；跨端新建计划的实时目录发现尚未实现。
- Apple Design落地为原生分段、Menu、List与sheet，紧凑黑色界面、至少44pt的项目选择与方向操作；AX5改为纵向排列。项目Menu原先只提供动作label，最终以label/value分别表达用途和已选项目，首屏与待办审计通过。固定段落高度和移除文字选择的尝试未解决问题，已撤回；没有过滤无障碍失败。
- 原生Codex/Claude卡删除不再依赖mobile_control标记。Runtime根据受支持Provider、准确快照、可确认状态和忙碌回合校验；App仍先检查项目控制权限、授权版本及排队工作，再复用现有关闭/保存路径。Swift请求构造和菜单使用同一范围，旧卡运行模式不被修改。原生卡SQLite重开/拒绝旧快照/删除/保留同名另一张卡、忙碌确认及不支持Provider均有具体证据。规划、Todo和停止未随此变更放开。

验证：导航红转绿6项；最新计划与只读边界1项、AX5首屏/待办审计1项通过；Swift44，Runtime150通过/3忽略，全仓1755通过/73忽略。Mac Release与未签名iPhone SDK Release构建通过；行数、no-allow与diff护栏通过。Computer Use实际展开完整指令、打开调整方向、核对只读禁用；另在显式隔离项目授权中打开mobile_control=false旧卡菜单、删除确认页并取消，两张卡保留。菜单AX索引失效时用原生Down/Return；没有点击最终删除。实际删除和保存由Runtime/SQLite E2E覆盖，不冒充真实Provider或物理手机验收。

首轮交互fixture超过15分钟上限，但超时前正式忘记测试电脑已通过；后续runner自动清理并结束，最终计划fixture与卡片fixture均通过撤销/关闭检查。真实桌面v70仍运行，用户已开启手机连接；Computer Use读取该窗口仍报noWindowsAvailable，本轮未完成真实桌面配对。TestFlight仍为0.1.0(3)，不含v71/v72；APNs继续延期。证据：target/norma-ios-evidence/v72/summary.json，截图同目录cua-command-instruction.png、cua-command-direction.png、cua-native-card-delete.png。Goal保持active，优先继续原生审批/停止、旧卡规划/队列、真实桌面联动与原型项目切换。


## v73 · 原生Claude停止与同会话续聊（2026-09-09）

原生mobile_control=false的Claude卡复用现有MobileClaudeInterrupt：后台验证会话路径并捕获完整EOF，前台重新核验run/turn后只向原PTY发送Escape，后台只接受该promptId新追加的Provider中断记录。发送按键或读取到旧标记不算成功。自然Stop返回completed；新回合、SessionEnd或会话替换取消旧请求，不能中断新任务。原卡、唯一PTY和原生审批模式保持。正式手机网关复用MobileTurnStore，后台持久化准确终态后才发送远程成功回执，失败返回结果未知。

两个原生停止单测真实红转绿；历史读取曾因新增多余执行器过滤导致既有测试失败，已修正，最终Runtime152通过/3忽略、全仓1757通过/74忽略，Swift44通过。macOS Swift客户端与隔离iOS模拟器分别通过真实CLI停止、查询、同操作重放和同会话续发，SQLite确认原回合Interrupted。iOS本次从提交到查询到确定终态为347.63ms，这是单次本机模拟器样本，不能当作物理iPhone或LAN/Tailscale延迟分位数。App check、Mac Release、行数/noallow/存储和性能护栏、diff检查均通过；本轮未改iOS产品页面，仅补能力断言和测量，原生iOS测试宿主已编译运行。

Computer Use通过菜单/快捷键打开真实v70设置页，确认手机连接已开启，进程同时监听局域网与Tailscale的9443端口；配对按钮坐标点击仍报noWindowsAvailable，键盘未到达配对页。本轮未新建真实桌面授权，未操作用户原模拟器。停止链路证据来自原生测试客户端，不冒充Computer Use按钮或物理设备验收。

原生Codex仍明确can_interrupt=false；请求级手机审批、旧卡规划/Todo/队列、跨端新计划发现、项目看板切换与完整多端验收继续进行。运行桌面v70未重启；TestFlight仍为0.1.0(3)，不包含v71–v73改动；APNs按用户要求延期。Goal active。证据：target/norma-ios-evidence/v73/summary.json。


## v74 · 原生Codex停止与原卡Todo验证（2026-09-09）

真实Codex 0.153.4探针证明，独立只读host的thread/turns/list会把未结束的原生回合重建为interrupted，不能据此确认手机停止。Escape令终端返回输入界面，但30秒内原rollout与正式timeline均未出现终态。Ctrl-C则追加准确turn_aborted，并在thread/timeline/list新增同turnId的turnCompleted(interrupted)。另一个只读探针验证limit=1返回最新终态，早期条目在nextCursor之后，支持有界读取最新页。原状态断言替换为准确新增事件断言，调查失败日志与最终通过证据均保留。

复用并统一既有MobileClaudeInterrupt为MobileTerminalInterrupt，不创建第二执行会话。Claude继续使用完整EOF与新增transcript中断记录；Codex复用Runtime已有的只读连接，准备时记录最新时间线位置，前台重新校验run/turn后向原PTY发送一次Ctrl-C，后台只接受此位置之后同turnId的正式终态。自然完成保持completed，旧位置、其他回合、正文伪装、未知状态和重复矛盾边界不能确认停止。原生网关继续先后台落盘再返回成功回执。停止入口和发送前均保护电脑未提交草稿，避免Ctrl-C清除原生输入。底层运行模式、PTY与session保留，原受控Codex RPC路径仍使用原实现。

能力检查先红转绿，正文/旧边界/其他回合和自然完成等聚焦检查通过。既有原卡测试仍期待不可停止，已按新能力及电脑草稿拒绝语义更新；草稿保护也先真实失败再修复。Runtime最终154通过/3忽略。macOS Swift两种原卡停止/回查/重复提交/续发共2项通过，隔离iOS模拟器同流程2项通过；本次iOS停止确认Claude297ms、Codex1552ms，后者包含首次只读host启动，不代表物理网络分位数。最终全仓1759通过/77忽略，App check、Mac Release、行数/noallow/存储与性能护栏、diff检查通过。

对旧卡Todo的代码核对发现，规划/队列读取的是运行中CardSnapshot手机能力；原卡spec.mobile_control=false不等于能力false。增加真实原生Claude回归，复用既有Swift Todo检查，完整入队、撤回、停止、重新安排、拒绝和续发通过，始终保留唯一原PTY、run/session及原模式。该检查的计划内容仍为固定fixture，不冒充云模型规划质量或SwiftUI操作。

本轮未进行Computer Use停止按钮、真实桌面新配对、物理iPhone或跨Mac/Tailscale验收；等待审批时停止和两种原卡手机批准/拒绝继续保留。运行桌面仍v70，TestFlight仍0.1.0(3)，不含v71–v74；APNs按用户决定延期。Goal active；证据：target/norma-ios-evidence/v74/summary.json。


## v75 · 项目默认看板与项目上下文联动（2026-09-09）

项目Tab直接复用ProjectCardsView显示当前项目的原Agent卡片。目录移入顶部“项目”按钮打开的原生搜索面板，保留名称/状态筛选、当前选择与只读提示；关闭项目不展示旧卡片或创建按钮。ComputerModel在应用会话内按电脑分别记忆项目，只能从当前授权目录选择；切换电脑清空旧目录，项目变化用SwiftUI视图身份取消旧WSS订阅、清除旧卡片与筛选，不增加网络轮询或新执行会话。创建卡片入口继续独立，创建与安排任务的行为不变。

项目“去指挥台调整方向”与任务页内项目入口均切至居中指挥台，并选中同一项目和行动计划分段；指挥台项目菜单共享这一选择，决策分段仍汇总全部电脑。通知仍聚焦原待决策事项。方向生成页移除了“另建已启用手机控制卡片”的误导文案，仍按真实Provider能力与授权核验。

Apple Design落地为系统Sheet/Menu/搜索、44pt项目操作、黑色双列卡片和AX5单列。初始指挥台AX5首屏反复报Text clipped，系统未提供具体元素；修改Menu语义/图标未解决，未过滤错误。将电脑名称与当前方向收在同一内容区、用较小字体表达电脑上下文、让方向与简报按完整内容高度布局后，首屏和待办无障碍审计通过。项目菜单用下拉箭头表明可切换。早期失败日志全部保留。

证据：默认看板先红后绿；两个只读项目复用node_id=1的关闭/打开/切换/指挥台/返回流程通过，旧项目卡片和计划均有负断言；Tab挂载断言补有界等待后通过。原生ComputerModel单测验证两电脑独立选择、撤销后的目录回退和断线清空。Swift44、E2E234通过/48忽略；项目/聊天普通字号、AX5和搜索3项通过；指挥台AX5/正常计划流程2项通过；未签名iPhone Release、行数/noallow和diff检查通过。

Computer Use实际从项目进入指挥台、通过原生Menu与Down/Return切至第二项目并确认其计划空态、展开完整待办、打开正确项目的方向页并确认只读禁用。顶部项目按钮未出现在Simulator的CUA AX树中，坐标点击仍报noWindowsAvailable；该按钮的切换/搜索由XCTest覆盖，不能宣称完整CUA验收通过。一轮CUA会话因未及时写结束标记触发180秒超时，正式忘记测试电脑仍完成；后续简短会话明确结束，fixture与清理通过。截图及日志位于target/norma-ios-evidence/v75/，汇总见summary.json。

本轮没有切换运行桌面、上传TestFlight、触碰用户原模拟器或部署APNs。TestFlight仍0.1.0(3)，不含v71–v75。项目脉络的真实方向/简报、跨端新计划发现、原卡审批、Codex原卡Todo、真实桌面双向UI与物理iPhone/LAN/Tailscale性能继续保留；隔离固定计划与模拟器证据不替代这些验收。Goal active。


## v76 · 原生Claude请求级审批（2026-09-09）

真实Claude 2.1.202探针确认PermissionRequest没有tool_use_id；其PreToolUse与会话tool_result有准确工具ID。原生No不会发Stop/PostToolUseFailure，必须同时读取该工具的明确拒绝和同prompt的新增中断边界。原PTY方向键加Enter和数字快捷键均验证实际文件效果；Hook只观察、不返回决定、不阻塞原生确认。保留最初编译错误、缺少Hook终态与历史计数失败日志，不能把它们当作通过证据。

本轮复用原卡、原CLI和现有TLS/WSS命令/回执，不启用受控执行器，不改变原卡模式。Runtime以唯一待执行工具的精确输入绑定PermissionRequest，只有与真实原生Bash确认屏幕匹配才给出批准/拒绝。并发/重复/缺身份/未知布局拒绝操作；电脑先输入即废弃旧手机目标，两台手机不能重复预约。后台捕获完整EOF，前台重验后向原PTY发送一次对应数字，后台分页读取准确新结果。拒绝还须确认同回合中断边界并后台落盘；按键写入不是成功回执。App异步实现遵循gpui-async，IO和持久化不上GPUI前台。

聚焦回归已红转绿：真实工具快照、电脑接管、并发歧义、错误屏幕/命令前缀、重复预约、后台准备期间接管、旧记录/其他工具排除及拒绝中断边界。原生Swift客户端的正式TLS/WSS两次决策、持续订阅、丢响应后回查、重放、旧目标拒绝、准确文件效果和原会话续发通过。原生No在历史中额外保留真实中断标记，测试改为逐一核对两个准确任务正文，而非错误要求只有两条user记录。

当前支持范围是已验证的Claude原生Bash确认；其他Claude工具和Codex原生审批仍待接入。完整Computer Use、等待审批时停止、多机竞争和物理iPhone/LAN/Tailscale验收尚未完成。桌面62572进程仍同时监听LAN与Tailscale9443端口，本轮未中断或替换用户会话；运行桌面仍v70，TestFlight仍0.1.0(3)，不包含本轮代码。APNs延期。最终全仓在4个并行测试下1763通过/79忽略，Swift44通过，Runtime158通过/3忽略；Mac Release与存储/性能/行数/noallow/diff护栏通过。默认并行首次有一个计划快照返回unavailable，单独复查和两次4并行全仓均通过，初次时序失败的根因未确定，不宣称默认并行首次通过。隔离iOS模拟器5265591D-704C-4515-868D-C2D64F94A7FA的真实XCTest host经正式TLS/WSS完成批准、拒绝、持久回查、重放、旧目标拒绝和原会话续发，共1项通过；未操作用户原模拟器。结果见target/norma-ios-evidence/v76/summary.json；Goal active。

补充边界：确认屏幕匹配已通过“去掉命令内部空格会混淆不同命令”的红转绿测试，保留内部空格，无法准确重建的换行布局交回电脑。批准当前等待准确成功tool_result；长Bash任务超过20秒或工具失败仍可能返回outcome_unknown，即使批准键已发送，不能把这一点宣称为完整通用审批支持。后续应优先补更早的请求级执行证据和这些错误路径，再扩大到其他工具/Codex。最终iOS闭环验证覆盖了严格空格匹配版本；本轮没有将新Mac产物切换到用户正在运行的桌面。


## v77 · 原生Bash长任务回执与审批订阅过渡状态（2026-09-09）

真实Claude 2.1.202探针确认，30秒前台Bash在完成前没有transcript progress/开始记录，准确tool_result约31.36秒后才追加。另一次隔离写入后exit7，真实PostToolUseFailure含准确tool_use_id、完整input与is_interrupt=false。批准已生效与命令成功必须分开，不能只延长网络超时或把按键写入当作成功。

复用terminal原有PTY出生身份和进程树查询，增加有界只读的直属子进程观察：后台准备捕获审批前进程，执行时核验原CLI/子进程出生身份、父子关系及完整argv；仅支持已观察到的Claude macOS zsh包装，命令的引号和内部空格逐字匹配。Darwin参数只按argc解析argv，环境变量不返回、不记录；读取失败、超过64个直属子进程或未知包装不产生早期成功。原有精确transcript结果继续兜底；准确非中断PostToolUseFailure也可证明批准生效。所有查询复用既有后台路径，原PTY、手机/电脑竞争保护、20秒回执预算与持久化协议保持不变。

正式Swift闭环复现了第二个问题：Hook已到但菜单尚未显示时，Runtime发出Bash加空decisions，违反Swift快照校验并中断持续订阅。现已复用native_terminal只读提示，菜单核验后才发布Bash动作；同时覆盖真实长命令菜单的固定“This command requires approval”提示与中间选项折行，额外数字选项仍拒绝。测试先复现失效，再验证过渡状态、错误Hook/工具/输入、非零结果早于Hook、完整argv与命令空格、进程身份和折行菜单。PTY原有测试移入该crate的tests/unit以保持源码500行限制，终止行为未另建实现。

macOS Swift与隔离iOS XCTest host分别完成原卡拒绝、批准30秒命令、持久回查、持续订阅、重放、旧目标拒绝及实际文件效果；批准回执分别约1.490秒与1.308秒返回，当时目标文件尚不存在。最终命令exit7不改变批准回执，两条真实任务正文各一次，原run/session/唯一PTY保留。这是各一次本机网络样本，不是物理iPhone或LAN/Tailscale性能结论。iOS首次因新增测试mode尚未列入白名单而跳过，被Rust必需证据检查判失败；修正测试入口后实际执行1项通过，不将跳过计为成功。

证据目录：target/norma-ios-evidence/v77/。Swift44项通过，Terminal48通过/2忽略、Runtime161通过/3忽略，Mac Release通过；最终4并行全仓复查1768通过/82忽略，行数/noallow/存储/性能与diff护栏通过，汇总见summary.json。首次全仓4并行时通知设置停用用例失败，单独复查通过，保留初次日志；尚未确立其偶发时序根因。该用例未被删除或放宽断言。

仅已核验的Claude原生Bash路径获得本轮改进；未知shell包装的长任务仍可能等待准确工具终态并超时，其他Claude工具、Codex原卡审批、等待审批时停止、Codex Todo与跨端规划/项目脉络继续实施。真实桌面双向UI、完整Computer Use、物理多Mac/iPhone与LAN/Tailscale性能仍待验收。本轮没有切换用户正在运行的v70桌面、触碰用户原模拟器、上传TestFlight或部署APNs；TestFlight仍0.1.0(3)，Goal active。

## v78 · Codex原卡Bash审批与真实iOS回执（2026-09-09）

真实Codex 0.153.4原PTY探针确认：PreToolUse为Bash，输入只有command，工具ID为exec-UUID；PermissionRequest没有工具ID，只增加description；PostToolUse保留原工具ID和原始command。原生菜单的y是单次批准，Escape拒绝并中断整个回合。允许后的CommandExecution记录与Hook的exec-UUID一致；拒绝没有PostToolUse/Stop Hook，但追加准确turn_aborted。最初只筛function_call的探针看不到当前CLI的custom_tool_call及item_completed，已修正证据采集，不把空筛选结果当作不存在真实结果。

复用NativeApprovals、原PTY、现有TLS/WSS命令与持久回执，以及原生停止的历史读取类型。仅唯一、准确的PreToolUse与已验证本地菜单可操作；只允许已观察到的description字段差异，未知字段、未知环境、不同命令空格、并发和电脑接管都拒绝旧目标。批准等待准确PostToolUse；拒绝先捕获原会话timeline位置，再等待同turn的新增interrupted边界，落盘后才成功。读取和持久化保持后台，原卡模式与运行实例不变，没有新建Agent执行器。原有只读archive host继续用于查历史。

测试先复现Codex原卡只能显示native_terminal、无法预约批准，再验证新实现及Claude回归。新增进程级隔离Codex探针与测试专用archive配置入口；临时目录只生成Norma观察Hook，复用现有登录，不读取/输出凭据，不修改用户全局配置或外部Hook。隔离许可来自用户此前明确同意；只读沙箱和逐次审批始终保留。用户配置下外部Hook占用审批的初次证据保留，不能把等待Hook当作手机网络延迟。

macOS Swift和隔离iOS XCTest host各一次正式闭环通过：先拒绝、再批准准确临时写入、持续审批订阅、持久回查、原操作重放、旧目标拒绝和准确两条历史；保持同run/session和唯一PTY。macOS 20次审批读取P95为51.25ms，拒绝/批准持久回执约1.266s/0.321s；iOS 20次读取P95为47.97ms，回执约1.427s/0.200s。回执包含本地保存、TLS/WSS提交、查询和落盘，不是模型耗时；每种决定只有一次样本，不能外推物理iPhone或LAN/Tailscale性能。

证据目录：target/norma-ios-evidence/v78/。Swift44通过，原生Runtime聚焦17通过；真实原生菜单批准/拒绝探针各通过，macOS Swift与隔离iOS各1项通过；最终Rust全仓1770通过/86忽略（4并行），Mac Release与cargo check -p app通过；行数/noallow/存储/性能与diff护栏通过，汇总见summary.json。隔离模拟器仅5265591D-704C-4515-868D-C2D64F94A7FA，未操作用户原模拟器。本轮不是Computer Use页面点击验收；完整UI、物理多机和跨网仍待验收。

本轮首次全仓编译发现e2e feature合并后，旧AgentHookServer测试字面量缺少新增观察字段；已补齐测试初始化后重跑，初次失败日志保留。

当前支持已核验的Codex原生本地Bash菜单；外部Hook仍有原生审批权，未知布局/其他工具交回电脑。Codex长任务提前回执、非零退出、等待审批时显式停止、Todo/队列，以及跨端规划和项目脉络继续实施。运行桌面仍v70且LAN/Tailscale9443均在监听，未切换或结束用户会话；TestFlight仍0.1.0(3)，没有上传或部署APNs。Goal active。

## v79 · 新方向实时发现与项目脉络（2026-09-09）

旧指挥台只在进入或手动刷新时取最新计划，固定plan ID的后续订阅无法发现其他端发布的新方向。真实iOS红测复现新方向和Todo均未自动更新，项目页也只有固定说明。复用已有watch_plan：省略plan_id即订阅授权项目的最新保存计划，指定ID仍保留原有历史订阅语义。后台复用SQLite按单调序号读取、现有变化信号与50ms合并；无正文2s心跳不重复传计划，没有另建轮询或执行器。

Swift验证空计划基线、首计划、新序号和授权范围，拒绝同ID改写、旧序号回退、已有计划退回空态、串项目/手机/流和事件缺口。指挥台持续接收最新计划，新ID重建Todo页面与确认状态；当前进度直接传给Todo，避免再订阅同一计划。项目脉络显示真实方向、简报和Agent总数/待命数，空项目明确显示未设方向。切项目、授权变化和断线会清除过期内容；读取计划仍不安排Agent，写操作继续沿用实际授权和原卡命令。

Apple Design继续使用原生SwiftUI控件、黑色层级、紧凑字号、44pt方向入口和完整文本换行。正常字号双列卡片、AX5单列，以及新增脉络的文字裁切/触控/描述审计均通过，没有通过截断正文或过滤审计问题过关。

自动化证据：真实TLS/WSS与SQLite验证空态、另一手机正式生成命令、两次新计划各在小于2s心跳的1s读取预算内到达、原stream不变、固定历史订阅不换计划、撤销后断流；这不是物理网络分位数。隔离iOS真实App的停留更新/替换Todo/项目脉络/切项目空态1项通过，既有导航和计划回归2项、正常字号与AX5审计3项通过。Swift45通过；Rust全仓1771通过/86忽略（4并行）；App check、Mac Release、未签名iPhone Release、行数/noallow/存储/性能/diff护栏通过。测试fixture只发布保存的建议，不调用模型或运行Agent，结束后正式忘记测试电脑并撤销授权。

Computer Use本轮getApp返回Mac已锁屏且自动解锁失败，已请求用户解锁，不能将XCTest或导出的截图宣称为Computer Use验收。证据目录target/norma-ios-evidence/v79/，包括契约红测、两次Swift测试编译错误、UI红测和最终通过日志；summary.json状态为automated_validation_passed_cua_pending。

运行桌面62572仍监听LAN/Tailscale9443，保留v70和当前会话；TestFlight仍0.1.0(3)，没有上传或部署APNs。完整原型快捷入口、原卡其他工具/Codex长任务与Todo、实际双端UI、云模型规划质量和物理网络验收仍待推进。Goal active。

## v80：待验收收件箱与TestFlight构建4（进行中）

- v79已在解锁后的真实Simulator完成Computer Use：项目脉络、同项目指挥台、新方向自动更新、切项目空态和只读方向入口。两次180秒桥超时保留为失败；最终360秒桥实际点击并显式结束、正式清理成功，见`target/norma-ios-evidence/v79/summary.json`。
- 待验收计数来自SQLite同一只读事务中的完整结果，20条仅是列表页大小；部分索引排除失败、缺少起点、冲突和已验收记录。复用已有保存记录watch与变化信号，不为计数传文件正文。TLS/WSS两客户端正式验收后同流在1秒预算内2→1，撤销与窗口失效拒绝继续读取。
- SwiftUI指挥台复用已有审批源汇总全部电脑；待验收限定当前电脑项目。成果列表支持待验收/全部、实时移出和有界分页，打开详情时保持选中成果身份。未知或异常数据不当作零。
- 旧Mac拒绝新增读取字段时保留原只读API：旧版可读计划/全部成果并提示手动刷新，不能伪造待验收总数。仅准确旧协议拒绝帧触发兼容，授权、项目和响应校验仍保留。
- Rust全仓1773通过/86忽略、Swift47通过；成果UI点击已通过。最大辅助字号检查曾发现裁切，现已修复并通过；最终成果/数量未知提示UI通过，真实Computer Use完成列表、冻结详情、只读权限、返回与审批入口。坐标筛选与一次菜单点击受工具限制，由XCTest覆盖，不冒充CUA通过。设计继续按Apple原则：内容随滚动、原生筛选和导航、44pt触控与系统Dynamic Type。
- 用户当前桌面v70及会话保持运行；APNs服务不部署。新TestFlight使用现有Norma内测组，版本0.1.0(4)，最终签名IPA已通过Apple校验并成功上传（Delivery ID：8ada36b8-e8dc-4948-8b46-578315ec94a8）；Apple状态VALID，内测状态IN_BETA_TESTING；现有“Norma 内测”组已包含构建4，仍为原1名测试人员，没有新增邀请。

发布前CUA另外发现审批读取失败仍显示0项，已在同一decisionCount路径修复为“数量待核验/待确认事项尚未核验”；聚焦UI从2个断言失败转绿，真实CUA复验文字一致，见`cua-decision-fixed.png`。无新增执行器、依赖或自动命令重放。

发布完成：`target/norma-ios-testflight/v80/export-final/Norma.ipa`，SHA256 `c52ded9a71f72abb155c5ece484b547547309cf039581cc6932bfc73fc8981c8`。签名/Apple校验/上传均成功；最终构建、内测组、测试人员分别见`build-final.json`、`group-builds-final.json`、`testers-final.json`。新版实时待验收及原生卡新增能力需配套新Mac构建；当前v70保留运行，旧读取兼容不会伪造实时计数。G2–G5仍有真实设备与Provider边界验收，Goal保持active。


## v81：原型左右滑动决策卡（代码与原生自动化通过，Computer Use待完成）

原分页TabView改为单张堆叠决策卡。左滑拒绝、右滑允许一次、暂时跳过复用原prepareApproval/持久化/submit路径；按钮保留无障碍替代。完整代码与请求详情可选择，代码区和按钮区不触发审批手势。原型88–118pt/28%阈值和竖向意图锁定保持一致；来源、权限、内容、页面或scene变化取消旧手势。减少动态效果使用静态位置与文字/边框反馈，正常拖动直接跟手、约200ms回位，不通过速度预测授权，不等待动画再发网络请求。

纯Swift新增红测后48项通过；真实TLS/WSS隔离fixture的两项SwiftUI测试通过，覆盖AX5、竖向阅读、只读/短拖/文本区/跳过无副作用、拖动中内容更新取消、两种准确决定及正式回执。普通E2E236通过/57忽略。首轮暴露取消状态跨手势残留和移动坐标低估位移，均已修复；另修正合成fixture缺少完整回执身份，没有放宽生产验证。UI忘记测试电脑一轮失败，随后通过明确身份的原生正式forget恢复；不能将该轮整套fixture记为成功。真实Computer Use手势尚未通过。证据：target/norma-ios-evidence/v81/summary.json。

## v82：选择Agent即创建空卡（按用户最新反馈简化）

用户要求去掉“创建卡片”命名与确认页面。项目顶部加号和底部创建入口统一使用原生Menu：选择Codex或Claude即向当前授权项目创建空卡，名称交由电脑默认分配。成功后保持在看板，不弹创建结果页；正在创建有即时反馈，结果未知或被拒绝仍保留原操作的回查入口，不自动重放。创建不会发送任务或跳转对话；只读、旧窗口/权限、离开页面及重复操作仍由现有路径约束。

移除CardCommandSheet的创建表单，仅保留删除/验收/修改及历史结果。先以旧页面跑失败测试，再通过真实TLS/WSS、正式授权/租约、Runtime元数据及SQLite检查两个入口；收到的恰好是Codex和Claude各一次create_card，title=null，无目标或会话。原生UI1项、完整隔离fixture、正式配对清理和未签名iPhone Release通过；普通E2E236通过/57忽略，行数/noallow/diff护栏通过。此fixture不启动真实Provider，既有原生卡执行证据仍独立保留。

Computer Use已观察到新版真实Simulator看板和原生创建菜单入口，但点击时工具再次返回Mac锁屏，未完成菜单点击验收；AX桥正常结束不能当作点击通过。TestFlight仍为0.1.0(4)，未上传这些新改动，运行桌面v70与用户会话未中断。Goal active；证据：target/norma-ios-evidence/v82/summary.json。

## v83：决策进入准确原会话

决策卡新增「查看任务详情」，仅在打开时读取对应项目快照，核对电脑、项目、窗口、Agent类型及运行/会话/回合后复用ConversationView；返回仍保留原决策位置。入口使用原生44pt按钮和独立导航，沿用Apple设计的直接操作与辅助字号适配，按钮区排除决策手势，详情打开期间不允许提交滑动决定。原生卡和只读项目可查看，发送权限仍由原聊天与服务端校验。通知定位也移除错误的卡片模式门槛，电脑负责的确认可以定位，但不增加批准选项。

先以纯Swift与UI红测确认缺口；最终Swift49项、三项原生UI（原回复/输入框、旧会话和窗口拒绝、AX5、左右决定与完整回执）、完整隔离TLS/WSS fixture及正式配对清理通过；未签名iPhone Release与护栏通过。初次UI测试错误地将系统输入控件限定为TextView，改用现有通用AX定位后通过。普通E2E默认并行两次存在3s网络读取/2s模拟Agent启动超时及测试锁中毒；目标单跑通过，串行236通过/57忽略，没有放宽时限或断言，默认并行稳定性仍待核验。全部失败日志保留。fixture使用合成Provider内容，不能代替真实Provider或物理网络性能验证。

Computer Use在getApp时仍报告Mac锁屏，本轮没有完成观察或点击；没有中断用户桌面会话。TestFlight保持0.1.0(4)，这些改动未上传。Goal active，证据：target/norma-ios-evidence/v83/summary.json。

## v84：原卡审批、待办闭环与构建5交付

用户连续明确：完成功能开发后直接发布TestFlight，由用户安装后统一验收和测试；不以Mac锁屏或物理设备验收作为代码工作的等待条件，APNs仍不部署。

- Codex原生长命令开始后不产生即时PostToolUse，旧20s等待会误报未知。复用原PTY进程出生身份观察器，准备时只绑定唯一Codex native子进程，执行时匹配新出现的直属`/bin/zsh -c <准确command>`；不扫描任意后代、不读取环境、不放宽超时。真实30s写入并exit7在macOS Swift/iOS分别约197.6ms/232.1ms得到持久批准回执，当时文件尚未生成；最终保留exit7与准确磁盘结果。
- Claude原卡Write补齐完整路径/内容与Create file预览核对，复用原生单次选择、Hook、原会话历史和既有回执。不匹配同名异目录、正文/缩进差异、缺行或未知菜单；截断/换行无法完整核对的预览仍回电脑。真实拒绝/批准在macOS Swift与隔离iOS均通过，无新增执行器。
- Codex原卡Todo的入队/撤回/停止/重排/拒绝/队列/终态与持续订阅在macOS Swift和隔离iOS均通过；只有两个准确待办进入原会话。两种原卡在待审批时停止、持久回查、重放、迟到批准拒绝和原会话续发通过；Codex另通过隔离iOS。
- 完整Rust回归1776通过/91忽略，Swift49通过；iOS设备测试61项中57通过/4个需fixture的测试显式跳过。Mac Release和App check通过，新增原生流程单独显式运行，不能把91个忽略记作通过。Manifest测试从硬编码build2失败修正为正整数校验；发布流程独立验证build5递增。最终E2E check无新增诊断；全仓运行中的一处测试括号警告已移除。
- v81–v83原生界面自动化证据继续有效；本轮Computer Use仍报告Mac锁屏，没有点击通过。用户明确接手发布后真机/跨网/完整界面验收。证据在`target/norma-ios-evidence/v84/`，保留所有红测及诊断失败日志；发布结果在下一条记录更新，不将归档等同于已分发。

交付完成（2026-09-09T23:20:41+08:00）：TestFlight **0.1.0(5)**，Apple `VALID`、内部`IN_BETA_TESTING`，同一「Norma 内测」组已含Build `098fb430-2cde-4926-aa97-7feb14607c06`；没有新增测试人员邀请。配套Mac0.1.71已生成并通过签名/DMG/布局检查，未公证、未切换当前v70。具体路径/摘要见[TestFlight记录](2026-09-08-norma-ios-release-records.md#release-testflight)。功能代码与必要回归已交付，完整真机、跨网、模型质量与页面验收由用户接手；整体Goal保留验收待完成状态，不将发布自动等同于G5通过。下一步依据用户实际验收反馈，不重复原样测试或上传。


## v85：原卡终端与构建6

v84原型P0复核发现终端查看/输入/历史入口遗漏，本轮补齐。任务详情使用SwiftUI原生「对话 / 终端」分段，默认对话；纯黑紧凑等宽输出、独立滚动、历史回看、实时状态与44pt快捷键。Apple设计原则落实为直接操作、清晰输入回执、辅助字号和只读/断线反馈，完整视觉验收仍交用户。

- 复用原卡PTY与输入编码，手机文字只填入原终端，独立Enter确认；支持Tab、Esc、退格、方向键、Ctrl-C。历史每页最多120行/512列，不改变电脑viewport；WSS只传变化行。回执written仅表示写入终端，不代表Provider完成或审批通过。
- 绑定准确卡片/run/session/turn、原屏幕签名与输入序号，防止桌面无回显输入或重放覆盖。真实Codex只重绘但屏幕未变曾被revision误拒，红测后改用已有屏幕/输入事实；不放宽旧身份、授权或菜单处理。复用原命令账本、租约与结果查询，不新建执行器或自动重发未知输入。
- 修复纯PTY输出没有唤醒现有订阅的问题，接入原50ms突发合并，WSS的500ms响应回归红转绿；这是同主机传输检查，不冒充物理LAN/Tailscale延迟。
- 最终Rust全仓串行1778通过/93忽略，Swift50通过；隔离iOS设备测试62项中58通过/4个fixture项跳过。真实Codex/Claude终端在macOS Swift与隔离iOS分别2项通过，包含先粘贴后Enter、唯一user、准确assistant和原run、持续增量与旧run拒绝。原有浏览器测试先状态更新后未等33ms重绘就断言；只补最多3×16ms等待，保留生产浏览器代码和全部断言，聚焦和最终全仓均通过，失败证据保留。
- Mac Release打包、iOS Release归档/导出/分发签名、Apple校验通过；归档后227个生产输入hash无变化。构建6已进入原内测组，Build/Delivery UUID为c0c9fdf0-2892-4093-8813-91f2f78a5390，Apple VALID / IN_BETA_TESTING，中文说明已保存。配套Mac为0.1.71（build20260909.155747），Developer ID签名、未公证。没有重启v70或中断用户会话；新终端依赖该Mac包，需在原任务结束后安装。

证据：`target/norma-ios-evidence/v85/summary.json`；发布产物与Apple回执：`target/norma-ios-testflight/v85/`。Computer Use仍报告Mac锁屏，不能把XCTest或headless GPUI当作真实界面点击验收。APNs服务延期，物理iPhone、多Mac、LAN/Tailscale、云规划质量和完整页面由用户发布后验收；Goal保持active。

交付确认（2026-09-10T00:12:14+08:00）：TestFlight **0.1.0(6)** 已可供现有「Norma 内测」组安装；Apple回读构建、版本、未过期状态、组关联与中文说明全部一致。沿用用户已确认的加密用途和不在法国分发范围，没有新增邀请、外部Beta或公开App Store发布。配套Mac包与SHA256详见[TestFlight记录](2026-09-08-norma-ios-release-records.md#release-testflight)。Goal继续保留实际验收待完成状态。

## v86：简单工具卡片与构建7交付

用户反馈iOS对话渲染有问题，要求先做简单工具卡片。复用同一ConversationMessageRow处理实时与归档消息；将现有Claude tool_use/server_tool_use/tool_result及Codex commandExecution/fileChange/mcpToolCall/dynamicToolCall/webSearch渲染为名称、摘要、状态组成的紧凑卡片。点击展开完整输入/输出，沿用ReviewText/TextKit的滚动和选择复制；保留原始记录入口，未知/损坏记录仍可查看。解析在ConversationItem解码时保存，避免SwiftUI body重解析。无新增依赖、执行器、Mac协议或Mac生产代码。

第一版调用和结果分别呈现，不跨消息猜测配对。Claude只有调用记录时显示“已请求”，结果显示“已返回”或失败；未知状态不推断成功。Codex非零退出、MCP result.isError及dynamic success:false覆盖外层completed，防止误标成功。Apple设计原则落实为默认收起、清晰层级、44pt操作区域、ScaledMetric图标和AX5整卡布局；导航用原生principal标题与完整可访问标签，正文保留最大辅助字号。

验证：Swift53通过；隔离iOS65项中61通过、4个fixture显式跳过；Rust E2E串行237通过/64忽略/0失败；两项原生UI、只读TLS/WSS fixture及配对清理均通过。UI覆盖默认收起、700行完整输出、同一订阅运行到失败及展开状态保留、Claude Read/结果；AX5首屏accessibility audit通过，精确滚动后断言整张工具卡位于导航和输入框之间并留图。工具正文是合成fixture，不代替真实Provider或物理网络验收。初始解析/状态/UI红测、大字号及滚动失败证据全部保留，最终证据以ui-ready.log与screenshots-ready为准，不用早期未入屏截图证明卡片完整可见。

本轮Computer Use尝试返回Mac locked，没有完成点击；不能把XCTest当作Computer Use。用户此前已要求发布后由其验收。证据：target/norma-ios-evidence/v86/summary.json；正常字号截图screenshots-ready/AE8C2C8A-5CEA-44D0-84F8-9558531F25E6.png，AX5截图screenshots-ready/C4544B4F-F4A9-477E-BC54-9223F5A83727.png。行数/no-allow/diff护栏通过。

交付（2026-09-10T00:51:25+08:00）：最终Norma-release.xcarchive归档、导出、严格签名与Apple校验/上传成功；229个生产输入hash一致。TestFlight **0.1.0(7)**，Build/Delivery ID `b092d15d-1a8d-45a1-8f0d-70c0bcb31093`，Apple VALID / IN_BETA_TESTING、未过期，原内测组包含构建7，中文说明回读一致。IPA `target/norma-ios-testflight/v86/export-final/Norma.ipa`，4772518字节，SHA256 `9fc755ebb9ab2aabb99d24b009d20be0774642d85ab0dfe12967992f78be205a`。配套Mac沿用0.1.71（build20260909.155747），没有重建或中断运行v70，也未操作用户原模拟器；无新增邀请、外部Beta或公开App Store。APNs延期，真机/多机/跨网/模型规划质量及完整界面验收仍待完成，Goal保持active。

验收阻塞记录（2026-09-10T00:54:49+08:00）：构建7发布后连续三个goal回合均实时调用Computer Use，Simulator因Mac锁屏不可操作；没有仍在处理的构建或测试需要轮询。用户已接手TestFlight验收，当前缺少完整界面、真机/多机/跨网和模型规划质量的最终证据；APNs为明确延期。已将Goal标记blocked而非complete，收到解锁或实际验收反馈后按原范围恢复，保留全部产物和会话。审计：`target/norma-ios-evidence/v86/goal-continuation-audit.json`。

## v87：项目名称置顶与构建8

用户标注将项目名称移到顶部。ProjectCardsView复用原principal切换按钮显示实际名称，移除正文重复标题；切换、关闭项目与返回时名称同步。长名称单行中间省略，完整可访问标签及切换提示保留；沿用既有导航字号上限，正文仍支持最大辅助字号。无新组件、依赖或Mac协议。初始置顶测试5处断言失败，修复后项目切换测试通过。

回归两次复现普通字号聊天VerticalTextView触控尺寸不足；在共用ChatComposer的原TextField本体增加44pt最小高度后，原审计全部通过，没有过滤问题或放宽断言。最终原生UI三项通过（正常/AX5、项目切换、五行输入、只读不提权），正式TLS/WSS隔离fixture与配对清理通过；日志和早期失败均保留于target/norma-ios-evidence/v87/。Computer Use实时尝试仍返回Mac locked，原生自动化不等同Computer Use；用户负责真机验收。

交付（2026-09-10T01:13:00+08:00）：TestFlight **0.1.0(8)**，Build/Delivery ID `cf77c6ef-fe0e-412d-b95b-7bb0cee86923`，Apple VALID / IN_BETA_TESTING、未过期；原内测组包含构建8，中文说明回读一致。IPA `target/norma-ios-testflight/v87/export-final/Norma.ipa`，4776794字节，SHA256 `c681e6dc29f44590719f59827ec907601ef76ec9314b0d37434aeb3f60aa76eb`；229个生产输入hash与最终归档一致，严格签名与Apple校验/上传通过。仅四个生产文件相对构建7变化：项目页、输入组件、两份构建版本配置。没有重启桌面v70、修改Mac生产代码或操作用户原模拟器；APNs延期。

正常字号截图：target/norma-ios-evidence/v87/screenshots-ready/8F0709CE-C5FE-4F39-802F-3592B5DEBCFA.png；AX5：screenshots-ready/5E2B2CFC-16EC-4F14-BE57-6C24A890B4E6.png。汇总：target/norma-ios-evidence/v87/summary.json。原Goal受阻状态保留，不将这一项用户反馈完成等同于全部验收完成。


## v88 · 项目结果入口精简（2026-09-10）

- 删除卡片上方“查看操作结果”，待确认回执复用“记录与成果 → 操作记录”；44pt点击区域与原查询/清理/防重复保护保留。
- 原生UI拒绝/查询/恢复闭环通过，第三次拒绝无新卡、记录查询无重发；两种成功创建仍为Codex/Claude各1张空卡。Rust隔离fixture、TLS/WSS与设备清理通过。证据：`target/norma-ios-evidence/v88/summary.json`。
- TestFlight0.1.0(9)已通过Apple处理并进入原内测组，VALID/IN_BETA_TESTING回读确认。Computer Use实时检查仍因Mac锁屏未完成，真实手机交用户验收；不将局部反馈完成当成整体Goal完成，桌面进程未重启，APNs保持延期。


## v89 · 电脑信息置顶（2026-09-10）

- 顶部项目名下增加电脑名，左侧原生电脑按钮保留一键切换；正文移除电脑信息行，VoiceOver保留完整名称及最近同步时间。
- 原生UI三项通过：电脑/项目切换、普通与AX5字号的项目/聊天无障碍和输入；隔离fixture/TLS/WSS与清理通过。普通及AX5截图已检查；Computer Use仍因Mac锁屏未完成。证据：`target/norma-ios-evidence/v89/summary.json`。
- TestFlight0.1.0(10)已通过Apple处理并进入原内测组，VALID/IN_BETA_TESTING回读确认。用户继续真实手机验收，整体Goal未完成项保留，桌面未重启，APNs延期。

## v90：三页顶部统一与构建11交付

任务“所有项目”和指挥台行动计划项目选择移到原生导航栏；项目、指挥台、任务复用WorkspaceNavigationTitle与左侧ComputerAccessButton。项目/电脑身份使用同一双行字体、间距与省略规则，右侧保留页面操作；正文移除重复项目选择。“待你决策”仍显示全部电脑范围，未缩窄全局审批。Apple Design落实为内容优先、原生导航、一致上下文和44pt点击区域。

最终5项原生UI全部通过：普通/AX5顶部与项目范围、行动计划无障碍、最新计划/Todo、任务状态筛选/搜索/清除及进入聊天返回；独立Rust TLS/WSS fixture和配对清理通过。较早项目看板切换回归也通过。修复审计暴露的任务文字裁切与当前方向文字触控区域，保留所有红测及诊断日志。普通与AX5截图已检查；任务截图的部分不可读提示来自故意关闭的第二fixture项目，不能当作真实网络结论。

2026-09-10T08:55:24+08:00确认TestFlight **0.1.0(11)** 已VALID/IN_BETA_TESTING并关联原内测组，Build ID `b13b1cbd-642f-415c-99df-d320d3308c65`；中文说明回读一致。Release归档/导出、严格签名、Apple校验/上传及229个生产输入hash均通过。产物：`target/norma-ios-testflight/v90/export-final/Norma.ipa`；汇总：`target/norma-ios-evidence/v90/summary.json`。

本轮Computer Use实时检查仍返回Mac locked，未完成点击；XCTest和截图仅为补充，用户负责发布后真机验收。运行桌面与用户原模拟器未操作，APNs仍延期；整体Goal保留blocked，不因本次反馈交付标记完整验收完成。

## v91：统一提示布局、补全图标与构建12交付

项目、指挥台、任务复用WorkspaceStatusView，统一SF Symbol、Dynamic Type文字、20pt内容间距、8pt按钮间距及原生主次按钮；失败/空态可重连、重新读取和切换电脑。正常字号居中，AX5支持滚动触达按钮。Apple Design落实为内容优先、原生反馈和一致恢复路径。

使用内置image_gen重新生成iOS图标，保留人物剪影、X发夹与彩色背景，补全左下两个白色方块和四角背景；1024×1024不透明全出血PNG由iOS裁切圆角。母版assets/app-icon-ios-source.png，正式AppIcon已替换；最终IPA内120px图标解码查看完整。图标前后230个冻结输入仅AppIcon.png变化，未修改Swift逻辑。

最终7项原生UI通过（普通/AX5故障与空授权恢复、顶部/项目范围、计划、任务与原会话返回）；真实隔离TLS/WSS fixture及清理通过。完整cargo test -p e2e为237通过/0失败/64忽略，未将忽略项计为通过。Computer Use本轮仍返回Mac locked，XCTest和截图仅为补充；完整物理设备/跨网/模型质量及界面验收保留，整体Goal仍blocked，APNs延期。

2026-09-10T09:48:43+08:00确认TestFlight **0.1.0(12)** 为VALID / IN_BETA_TESTING、未过期，并关联原内测组；中文说明回读一致。Build ID `61b61f0f-9f2b-429d-8806-90e5079877b1`。Release归档/导出、严格签名、Apple校验/上传和230个输入hash通过。IPA `target/norma-ios-testflight/v91/export-icon/Norma.ipa`，4914749字节，SHA256 `77ea375bf7131c68f8440d943e79a60a661ad19576cf0eec0a724e9897018613`。汇总：`target/norma-ios-evidence/v91/summary.json`。运行桌面未重启，用户原模拟器未操作，没有新增邀请或公开App Store发布。

## v92：聊天实时输出、键盘、草稿与滚动

发送不清空的根因是SwiftUI对相同文本的回写也生成了新草稿版本，导致正确回执无法匹配。共享草稿入口仅在文字真正变化时更新版本；保留真实编辑、未知结果、写盘失败与跨电脑隔离保护。发送成功不再重建项目/聊天订阅。

原生键盘工具栏提供“收起键盘”，聊天/终端一致；收起不丢草稿。初次打开和同ID正文增长时跟随末尾，iOS18及以上使用原生滚动几何/阶段识别历史阅读，iOS17保留手势兼容路径；历史阅读不被增量拉走，提供44pt回到最新按钮。Apple Design落实为原生交互、内容优先、用户可中断自动跟随。

真实原生Claude/Codex长回复探针显示：Claude历史约40.15s才一次出现2261字，期间原屏幕更新188次；Codex约50.10s才出现1515字，屏幕更新278次。因此生成时复用原PTY的现有WSS增量显示最近24行实时预览，完成后呈现正式消息与工具卡，不切换执行器、不伪造逐字输出。预览不是结构化token流，正式完整历史继续保留。

红测分别复现键盘无出口、相同文字导致回执清空失败、未跟随正文、向上阅读被拉回以及历史落盘前没有输出。修复后Swift54通过，完整E2E237通过/0失败/64忽略，4项聚焦原生UI通过；真实Provider长输出2项通过。Computer Use成功点击原卡，并看到第一句→第二句→正式最终消息；typeText/setValue未写入，坐标点击报noWindowsAvailable，不宣称完整键盘CUA验收。早期CUA准备误用card1产生fixture失败，按正式清理流程回收，最终桥与清理通过；所有失败日志保留。

用户已明确授权切换配套Mac：v70 PID62572退出，v92 PID37820使用相同数据目录及socket启动；切换前后SQLite一致性备份、配对记录比对和随包Hook测试通过。用户已在界面重新开启手机服务并打开项目；新进程的LAN与Tailscale端口均通过TLS握手且证书一致。此为同Mac回连检查，不冒充物理手机网络验收。iOS构建13已通过Apple处理并进入原内测组，中文说明回读一致；证据见target/norma-ios-evidence/v92/summary.json。


## v94：按用户 v28 原型重做三个主页

本轮以用户最新图1–3及 docs/prototypes/archive/2026-09-06-norma-ios/norma-ios.html 为视觉依据，覆盖此前v89/v90使用系统导航标题的设计：统一纯黑背景、norma/电脑品牌栏和近全宽74pt悬浮底栏，N指挥台居中；项目恢复独立大标题、同步信息、两枚统计胶囊、165pt两列Agent卡；指挥台恢复简报大卡、指标、当前方向和简洁Todo，点击后进入原有确认/执行详情；任务恢复独立标题、所有项目、搜索和白色选中筛选。Codex/Claude原型矢量图仅转换为PDF资产，保持品牌形状，运行动效遵守Reduced Motion。

Apple Design落实为明确层级、内容优先、44pt实际可点区域和可访问字号。新Todo详情保留原命令账本、回执、权限与错误反馈，不更换Agent会话。最大辅助字号下导航区保留可阅读空间，正文完整换行；工具/对话、键盘收起、成功清空与滚动沿用v92。

真实接口没有原型示例的分钟数或阶段计数，本轮未把固定数字当作真实进度。阶段/耗时的数据投影仍需后续补齐；界面还原不能据此宣称任务数据全部一致。完整Goal继续active，物理iPhone、跨网、云规划质量与CUA完整验收尚未结束，APNs继续延期。运行中的Mac v92和用户会话未中断。

验证与发布状态以 target/norma-ios-evidence/v94/summary.json 和下一条发布记录为准。初版红测、触控/换行失败、fixture组合冲突及菜单点击诊断均保留，不把失败尝试改记为通过；正式隔离配对均走原清理流程。

构建14已确认VALID / IN_BETA_TESTING（`64119766-58d6-4011-90e9-15c756ab5176`），原内测组可更新。12项UI、Swift54及E2E237通过；完整证据、数据限制与CUA未完成项见v94汇总及发布记录。


## v95：任务卡补齐真实回合耗时

耗时来自已接受的原卡Hook（Codex/Claude）与既有受控Codex事件，复用同一PTY、run/session/turn。重复提交不重置起点，旧回合Stop不能冻结新回合；正确结束用Mac单调时钟记录完成耗时。未观察到起点的恢复历史、旧服务和缺身份Hook不显示假数字。运行时仅传稳定起点，手机每分钟本地更新，无每秒WSS或全聊天扫描；运行显示仍受手机/Mac墙钟差异影响，完成值不受该差异影响。

任务卡底部恢复状态/时钟耗时布局，正常与AX5字号都可滚动完整读取。Apple Design继续落实内容优先、原生字号、即时状态与一致卡片布局。CardSnapshot的操作目标相等规则继续忽略展示字段，ProjectProjection单独比较展示信息，使计时/能力变化可通过增量送达，不让旧客户端回传目标失效。没有把工具调用次数冒充计划步骤，也没有推测百分比。

最终Swift55通过、全workspace1780通过/0失败/93忽略，任务页5项原生UI通过（正常/AX5计时、旧服务字段移除、筛选与原对话返回、指挥台计划、读取错误）。真实TLS/WSS仅计时变化增量及正式配对清理通过。初次全仓编译漏补测试DTO字段、初次AX5误用指挥台专用滚动helper均已修正；所有红测与失败尝试保留。UI fixture数据为明确合成，不冒充真实Provider或真机测量。

Mac v95 release构建、严格本地签名和随包Hook检查通过；包位于target/norma-mobile-desktop/v95/Norma Mobile Validation.app。当前v92 PID37820继续运行，数据已做一致性备份，已询问用户是否允许切换以免中断会话，未把旧切换授权复用于本次重启。iOS构建15已归档、导出并核对签名及93项冻结生产输入，Apple验证/上传状态见target/norma-ios-evidence/v95/summary.json。

Computer Use再次返回Mac locked，尚不能完成真实点击验收；XCTest和截图只是补充。Goal保持active，阶段/步骤数据、物理iPhone/跨网和真实云规划质量待继续；APNs按用户决定不部署。

2026-09-10T13:32:28.786227+08:00确认TestFlight **0.1.0(15)** 为VALID / IN_BETA_TESTING（`8f4343e0-b52e-448f-987f-4fad6740054e`），原内测组可更新；中文说明回读一致。单次上传、Apple校验、签名和93项生产输入hash通过。产物target/norma-ios-testflight/v95/export-final/Norma.ipa，5233552字节。Mac切换仍待用户确认，运行中的v92及用户会话未中断。

## v96 · 真实步骤来源与Codex状态未确认（进行中）

- Claude原卡同PTY已产生真实TaskCreate/TaskUpdate：两步pending→in_progress→completed；准确session/prompt/tool_use_id随事件送达。Codex 0.153.4当前原生会话缺少update_plan工具，schema存在不等于可调用，不以最终marker或工具次数伪造步骤。证据：`target/norma-ios-evidence/v96/claude-plan-hooks.json`、`codex-plan-hooks.json`。
- 步骤投影当前新增于Runtime（64任务、256确认更新上限），仅确认的Claude任务载荷推进；重复/失败/旧回合不推进，不完整或超界隐藏本回合数字。已接入可选DTO、显示增量比较和App唤醒；Swift解码验证通过，UI仍待补。
- 用户反馈Codex“状态未确认”：CUA看到当前v92的Alan/Chloe/Daisy原卡都停在输入提示符，但启动头部已滚出可见区。红测精准复现unknown；原判断只看viewport，修复读取同PTY有界尾部并使用最新model/directory，仍拒绝loading。
- 真实小卡探针第一次被0.154.0升级菜单阻挡，保留日志与screen证据；增加当前手机原卡进程的`check_for_update_on_startup=false`，不改全局CLI/配置/沙箱与审批。二次真实验证运行中。
- 当前发布仍为0.1.0(15)。Mac仍运行v92 PID37820；本轮尚未打包/发布，也尚未获得新版切换许可。

### v96 最终实现与验证补充

- 原型任务卡已接入3px分段轨道、当前阶段和完成/总数，标准/AX5字号通过WSS增加、推进、移除字段的原生测试，旧Mac不显示0/0。CUA实点任务Tab、读取1/2，再观察自动更新2/2；对话入口可打开，但本通知fixture没有真实聊天数据，返回坐标点击noWindowsAvailable；不夸大为完整聊天/真机验收。
- Claude首次集成探针只有SessionStart，90s未观察到提交，失败保留。第二次真实工具给出“重复完成”成功回执，updatedFields=[]、没有statusChange，暴露投影过于保守而隐藏计划；红测后修复这一已验证的无变化回执。第三次19Hook、真实2/2完成通过，见`v96/claude-progress-final.log`及`v96/claude-progress-final/`。原根目录Hook JSON已被失败重试覆盖，不再当作初次成功证据；使用最终独立目录。
- 第一次全仓在Azure本地请求1s到达期限超时；第二次在卡片创建网络读返回WouldBlock。卡片创建独立重跑1.14s通过；没有放宽生产或测试超时。最终降低测试间资源竞争，`cargo test --workspace -- --test-threads=1`完整通过：1786通过、0失败、96显式忽略。Swift56通过；标准与AX5任务UI2项通过，正式清理fixture成功。
- 物理iPhone/双Mac/Tailscale、完整界面与真实云规划质量仍待验收，APNs服务仍延期。Mac最终v96重新打包中，不重启当前v92；build16归档中，尚未宣称已发布。

最终交付准备：Mac v96 release2m38s、严格签名与随包Hook检查通过，`target/norma-mobile-desktop/v96/prepared.json`为最终二进制hash；已备份app.sqlite且quick_check正常。新的v96切换请求已发出，替代未答复的v95请求，尚未获批。iOS build16归档、导出、签名和93个冻结生产输入通过，IPA sha256为`ed5711747254f2a8db743941822816b9c81e0fc940efeae0df3e0707eff1750e`，正在Apple校验。

发布完成：TestFlight **0.1.0（16）**，Build ID `a0aa1107-684f-4d39-ac24-c806513646f7`，Apple VALID / IN_BETA_TESTING，原内测组与中文说明回读通过。IPA `target/norma-ios-testflight/v96/export-final/Norma.ipa`，5265968字节；93个生产输入在上传后仍一致，无重复上传或新增测试人员邀请。Mac v96切换仍待用户答复，当前v92会话未中断；Codex就绪修复需要此Mac包，不能仅靠更新iOS。Goal保持active。

### v96 交付复核与剩余原型差距

- 再次确认用户进程PID 37820来自Mac v92；待交付v96主程序与随包Hook的SHA-256均与最终`prepared.json`一致。本轮未重启Mac、未重复上传iOS，原切换确认继续有效且尚待答复。
- Computer Use核对v28任务页：每张卡片有具体工作摘要；当前`ios/App/TasksView.swift::taskSummary`仍为状态模板。已有`ConversationTailMemory`只保存完整回复，新用户消息不会清除上一轮回复，且依赖Agent definition；不能把它直接投影成本轮实时进展，也不应为列表逐卡拉取完整聊天。此差距尚未实现，后续须确保摘要属于准确原卡/会话/回合，复用后台事件和现有WSS更新。
- 原型“新建任务”弹层包含项目、执行Agent、目标输入、保留草稿与发送；当前原生入口是先选择既有卡片再进入对话。它与“项目里仅选Agent类型创建空卡”的后续用户要求是两个入口，应分别验收，不能因空卡创建简化而宣称任务弹层已一比一还原。本次仅核对，已关闭原型弹层恢复任务页。
- 本轮未改生产代码，无新增构建或测试通过声明。下一次iOS上传应使用构建17；Mac切换前仍需刷新配对备份。


## v97 · 新建任务按原型还原

- 原型任务加号现在直接进入黑色原生Sheet：项目、执行Agent、目标输入、保留草稿、发送到Mac；Agent菜单按Codex/Claude分组显示真实原卡，避免同类型多张卡时暗中换目标。项目内创建空卡仍只需选择Agent类型。
- 复用ChatComposer、ChatDraftBook及既有命令存储/回执。按电脑/项目/窗口/卡片/原会话隔离草稿，切换目标不搬运文字；只读可编辑保存、不能发送；成功后打开准确原卡对话，运行中的原卡沿用既有队列。没有新增列表轮询、完整历史请求或第二执行器。
- Apple Design与Emil：紧凑左对齐标题、圆形关闭、原生菜单与Sheet、黑色共享样式；普通字号640pt Sheet可展开，AX5和对话使用大Sheet；辅助字号按钮纵排，保留键盘收起与可滚动内容。详见`target/norma-ios-evidence/v97/design.md`。
- 红测准确复现“加号后缺目标输入”；新建任务原卡发送1项、AX5跨项目/替换窗口隔离1项、既有聊天实时输出/发送清空/自动跟随2项均通过。Swift56通过；首次编译遇到SwiftUI表达式推断超时，拆为页面内子视图后通过。未改Rust生产代码，未重复宣称新一轮全仓回归。
- Computer Use实点任务→新建→Claude→实际输入UI draft→保存→返回→重开恢复同卡同内容→关闭；相应隔离fixture正式cleanup通过。初次窗口未前台时typeText无效、AX setValue未触发绑定，保留该限制记录；激活窗口后真实输入及保存往返成功。未用CUA发送任务，发送与准确node/run/session回执由原生UI/真实TLS fixture覆盖；Provider数据为显式合成，不冒充物理iPhone或真实CLI验收。
- 构建17：归档/导出/严格签名/生产环境entitlement/Apple校验通过，94个生产输入冻结一致；IPA `target/norma-ios-testflight/v97/export-final/Norma.ipa`，5362682字节，SHA256 `1564286e35788d23b668bb7798830d87e21930ac78906fa3ca18636a304a8943`。正在上传，尚未宣称TestFlight可安装；Mac v96包不变、切换待确认。


## v98 · 创建卡片表单与入口禁用（已发布TestFlight18）

- 用户最新截图和原型覆盖此前“选类型立即创建”：恢复Agent下拉、可选名称、项目及空闲说明、白色创建按钮、圆形关闭。
- 原共享pending将发送、终端、停止等所有未决操作一起用于禁用创建。现仅未决创建阻止再次提交；两个入口始终能打开说明，只读/未打开项目仍禁止提交。既有操作记录保留。
- 创建沿用同一正式CreateCard、原Runtime空卡和SQLite持久化；不发送任务或启动另一执行器。窗口变化关闭旧表单，授权重连同窗口保留编辑。名称上限按原型40字符，空白使用默认名称。
- TDD先验证入口不可点击，再接入表单；56项Swift测试与e2e默认237通过/67忽略；AX5名称边界/窗口变化原生UI通过。最终完整创建流程29.404s通过；CUA使用最终420pt弹层实际创建Layout helper/Claude/node103并确认空闲，底部重开/关闭无多创建。正式清理通过。
- v97构建17上传与Apple VALID已回读，尚未加入原组；继续交付包含v97+v98的构建18，不能复用已上传的构建号。

- 最终产物：`target/norma-ios-testflight/v98/export-final/Norma.ipa`，0.1.0（18），5,422,093字节，SHA256 `b68348339e81b8582766eb129b71cfe4ddb0f7d51c8f0620a890a03bf5321ec3`。95个生产输入冻结，archive/export/严格签名/production entitlement/Apple validate通过，上传中。
- 证据：`target/norma-ios-evidence/v98/ui-cua.log`、`ui-creation-ax.log`、`cua-observations.md`、`created-cua-final.json`、`final-cua-native.log`及`cleanup-cua.log`。`create-card.png`为最终XCTest附件，视觉判断由真实Computer Use补齐，不混同截图来源。
- 新卡仍是Codex/Claude；原型的Norma选项未新增执行器，遵循已确认首版Provider边界。Mac v96仍未替换，当前PID37820明确运行v92；此前切换确认仍待用户回复。

- 构建18上传成功：Delivery UUID `55e8ac08-5079-4625-aca5-f339cf6440a3`，正式上传文件与最终hash一致，Apple处理/原组关联待回读。

- Apple最终回读：build `55e8ac08-5079-4625-aca5-f339cf6440a3`，0.1.0（18），`VALID / IN_BETA_TESTING`，原组 `1e9af1ec-bf35-4a49-b7fd-01169c421384` 已包含此构建。中文说明与既有加密用途/法国分发范围保持一致；未新增邀请、外部测试或公开发布。详见 `target/norma-ios-testflight/v98/build-final.json`、`group-builds-final.json`、`localizations-final.json`。Goal仍进行中，不代表完整原型/真机/推送全部验收。


## v99 · 任务卡真实摘录与Claude首次提交

- 任务页摘要改为原卡本轮的工具说明/回复摘录，位置、黑色样式和内容层级沿用v28。普通字号限3行，辅助字号完整换行。Apple Design落实为内容优先、原生动态字号与一致的任务卡入口。
- Mac仅在内存保存240个Unicode标量，规范空白并过滤控制符，复用已校验node/run/session/turn的Hook及现有Codex事件。不增加LLM调用、完整聊天拉取、文件扫描或新轮询；只正文变化也唤醒原项目订阅，重复正文/心跳不重传。新回合清空、旧事件拒绝，旧Mac缺字段仍显示状态说明。原生Codex当前实际只披露完成回复，不能宣称所有中间自然语言进展都已覆盖。
- TDD真实红测证明Rust此前无摘录、Swift丢弃字段、原生UI仍显示状态模板；最终Swift57、摘要域层3项、普通/AX5两项UI及Computer Use列表更新通过。CUA点击可进入原卡标题，但只读列表fixture未提供卡1聊天源，返回坐标仍noWindowsAvailable；本轮不能宣称完整聊天往返CUA通过。正式fixture清理通过。
- 真实Claude首次检查发现粘贴留在输入框、30s回执outcome_unknown、90s无Stop。0.05s记录PTY红测准确捕获尚未绘制时提前Enter；共享发送入口现在为所有PTY Agent复用既有绘制后100ms等待，受控Codex RPC不变。Runtime174通过；同一真实Claude完整探针26.81s通过，15个Hook且同run/session的真实Stop摘录为NORMA_PLAN_DONE。Codex真实探针66.46s通过，仅3个Hook/无计划工具，完成摘录准确。
- 首次全仓测试的macOS默认音频调度超时，单独复跑仍失败。保留workspace.log和playback-recheck.log；没有关闭或修改该测试。随后显式跳过该1项运行其余测试；最终结果另记，不能称全仓全部通过。
- iOS构建19已开始归档，95个生产输入冻结；本轮只变TasksView、Snapshot和构建号/生成工程。准备Mac v99需含本轮Claude提交修复及v96 Codex小卡就绪修复，完成后再次校验签名及随包Hook；当前用户Mac v92未重启，原切换确认仍未得到答复。

- v99最终串行回归：1789通过/96忽略，显式排除上述1项已经单独复现的硬件音频测试；此前并发回归的终端用例在初始snapshot处缺route，单独2.76s及最终串行全回归均通过。源码没有禁用测试或放宽校验。Mac v99最终release、严格本地签名与随包Hook通过；仍未启动。iOS19已导出、95输入核对、签名及Apple VERIFY通过，上传中。完整证据见target/norma-ios-evidence/v99/summary.json。

- v99发布完成：TestFlight 0.1.0（19），build 633471ea-0bd7-4624-997d-bccc489987a6，Apple回读VALID / IN_BETA_TESTING，原内测组已关联且中文说明一致。IPA 5429361字节，SHA256 eaba89f3b5830e7940d4ac633bb8d8a14c2b70f6225c002322f31ec0fb27b0ad；未新增邀请/外部测试/公开发布，既有加密与法国分发范围不变。Mac v99已准备但未切换，当前v92 PID37820保留。Goal继续active，真机/跨网多机与完整原型验收未结束，APNs仍按用户要求延期。


## v100：原型卡片选项与按卡隔离（2026-09-10）

- 根因：项目页以项目级第一个未决命令隐藏所有卡片的菜单，且菜单仅有删除；删除页还可能误接另一张卡的待查询命令。
- 实现：每张卡均显示选项入口；恢复黑色底部面板、Provider图标、卡名与项目状态、查看任务／安排任务和删除。安排任务只进入原卡聊天；删除沿用正式确认及持久回执，不改变原PTY生命周期。
- 未决结果按电脑、项目、窗口和卡片匹配；只读、未决、状态未就绪均显示具体说明；授权或项目窗口改变关闭旧面板。沿用Apple设计skill的原生Sheet、即时按压反馈、44pt触达、动态字号与可滚动内容；不增加连接或轮询。
- TDD：旧实现菜单不可见红测失败；首轮聊天断言误用了TextView，改用现有通用AX定位；AX5删除Form需滚动到确认行。两次测试定位失败记录均保留，没有为通过测试改变授权或放宽业务断言。
- 验证：Swift57通过；5条原生UI通过：选项与原卡导航、AX5选项/窗口保护、命名与默认创建/拒绝查询、AX5名称校验/窗口变化、双项目切换。每次正式清理隔离配对通过。Computer Use实际点击卡2选项→删除确认→取消→安排任务→准确卡2完整聊天输入框；没有执行真实删除或Provider任务。
- 证据：`target/norma-ios-evidence/v100/summary.json`、`card-options-cua.png`、`original-card-chat-cua.png`、各`ui-*.log`与`cleanup-*.log`。96个生产输入已冻结，TestFlight 0.1.0（20）已VALID/IN_BETA_TESTING并关联原组；不重跑未改动Rust生产代码的全仓与Mac构建，沿用v99证据及其音频测试限制。
- Mac仍运行v92；v99配套包未切换，既有确认待用户答复。物理iPhone/双Mac/Tailscale与完整界面验收未完成，APNs按用户决定延期，Goal保持active。

构建20发布回读：`ebef2af5-e1b6-4dc0-b14c-c8315f3b1ca0`，VALID / IN_BETA_TESTING，原组`1e9af1ec-bf35-4a49-b7fd-01169c421384`；IPA 5479429字节，SHA256 `0149fd177d4f016307daabf3b16af297fadecb013f077d49c51cc813cc0a3dcf`。中文测试说明已保存并回读一致，无新增测试邀请。


## v101：方向编辑与优先事項（进行中，2026-09-10）

前一Goal回合分类：progress。v100已有正式代码、5条原生UI/CUA及TestFlight20回读证据，不是等待或重复状态。

本轮差距：原型P0要求优先事项及目标/取舍；当前“调整方向”却打开计划历史列表、空白输入，协议只有方向文字。恢复独立黑色编辑面板并预填当前方向，计划历史移为次级入口。优先事项通过同一持久请求绑定来源计划/待办索引，不将偏好拼入用户原文；模型必须保留准确同名同卡事项，后端确定性前移，遗漏/歧义失败。生成仍是建议，实际任务由既有Todo确认送入原卡，不放宽审批、不抢占运行任务。

新增保存保护：依据旧计划生成期间若另一端发布了新方向，SQLite事务内拒绝迟到结果；旧建议和决定不被覆盖。沿用gpui-async后台查库/模型/保存，前台只做窗口与原卡身份检查。无DB新表或迁移，无新增依赖。未选优先项仍用旧请求格式；带优先项需要配套新Mac，旧版严格解析将拒绝新字段，不会忽略偏好后假称成功。

当前证据：`target/norma-ios-evidence/v101/`下优先项测试1、存储2、Swift计划3通过，app check通过；原生UI红测已复现未预填与缺少优先项，正式清理通过；黑色方向面板已实现并可编译，首次绿色复验未通过：能预填、选项及收键盘，但提交后结果确认/返回失败；测试输入还出现光标位置导致旧文字未删净。正式测试配对清理通过。尚未发布或构建配套Mac。后续须补模型输入/网络UI、授权/窗口/并发、大字号/CUA与范围回归，再更新安装包和现有内测组。


### v101 当前待办核对

- 收尾方向编辑：修复提交结果确认及返回；调整 UI 测试的全文替换操作。检查后台页面重复订阅是否耗尽现有 8 条连接预算，目前仅为待验证假设，不能直接扩大连接上限代替诊断。
- 补方向的模型输入/返回、授权变化/窗口变化/并发发布、大字号和 Computer Use 验收；再进行范围回归及新包发布。
- 切换配套 Mac 修复版仍待确认；当前桌面 v92，因此 Codex 就绪/Claude 首条提交修复尚未在运行实例生效。
- 完整物理 iPhone/双 Mac、LAN/Tailscale 低延迟/空闲开销/切网恢复、真实云规划质量和界面联动验收仍未完成。
- APNs 服务按用户决定延期；当前 TestFlight 0.1.0（20），本轮开发代码尚未发布。全仓既有 macOS 音频调度失败仍保留，不能宣称全仓全部通过。


### v101 验证与交付准备

前一 Goal 回合分类：progress（黑色方向编辑器已落地、红测与提交失败证据改变了下一步）；本回合继续取得实现与验收进展。

方向编辑器已预填当前目标、以来源计划 ID/待办索引传递优先项，支持原生选择、300字目标/取舍、收键盘、历史入口以及回执查询。选择或编辑不提交，保存成功才自动关闭。另一端发布/窗口变化会阻止旧操作，输入保留；原审批和Agent执行仍各自确认。首次计划加载完毕前暂不开放文本编辑，避免异步预填覆盖用户输入。Apple Design落地为一致的黑色面板、系统字号和原生菜单、44pt以上可触目标、辅助字号可滚动，以及明确的提交/拒绝反馈。

实际诊断：测试Mac已保存建议，但旧/新页面重复订阅占满8条连接，导致手机回查被拒绝。暂停隐藏标签页、方向面板背后的计划/Todo与验收订阅后，正常提交流程可收到回执并返回；没有增大服务连接上限或延长超时。保留3轮连接计数原始日志；快速切换仍有短暂容量拒绝，下一轮继续处理一般连接复用/释放，不能把本轮正常路径通过当作完整性能达标。

验证：模型协议4通过/1个真实模型检查忽略，Swift58通过；方向正常/AX5窗口变化/并发发布3项原生UI通过。正式GPUI计划入口5通过/1个原生探针忽略，包含后台读取来源和真正SQLite并发拒绝。全仓1792通过/96忽略，仍显式排除既有macOS音频调度失败；之后添加的2条优先事项E2E另跑通过。跨项目导航通过；三主页测试初次仍断言旧“只读入口不可点击”，已按用户确认的入口可查看/提交需控制权限修正，完整三页重跑通过。

Computer Use已实际打开面板、核对预填目标、提交一次并看到回执和返回；截图：`target/norma-ios-evidence/v101/direction-editor-cua.png`与`direction-return-cua.png`。菜单选项点击工具报invalid element ID、坐标报noWindowsAvailable、粘贴超时；首个桥因工具排查超过有界时长，不能记通过。随后单独提交CUA及正式cleanup通过。原生XCTest已验证菜单选项和文本替换，不能将其说成CUA菜单点击通过。

Mac v101已构建验签且未启动，当前仍v92 PID37820；既有切换确认未答复，不终止会话。TestFlight21归档/导出/签名及Apple校验通过，96生产输入已核对，上传进行中；IPA SHA256 `d3e537a627ae34a36b11d822b1afcb2ce74d3721a6ac5703feb9b6c5e85292bb`。详见 `target/norma-ios-evidence/v101/summary.json`，Goal保持active。


v101发布完成：TestFlight **0.1.0（21）**，Build ID `f7ba7443-1929-4ed9-a6ae-5c9181311682`；Apple回读VALID / IN_BETA_TESTING，原内测组关联及中文说明一致。96生产输入和最终IPA严格签名核验通过；没有新增邀请、外部测试或公开发布。Mac v101仍未切换，v92会话保留；快速切页连接容量、完整真机/跨网/模型质量验收继续推进，APNs延期，Goal保持active。


## v102 · 订阅关闭与连接容量（进行中）

上一Goal回合分类：no progress（仅应用户要求核对并报告剩余任务，没有实现或验收推进）。本回合继续安全可执行的连接性能修复；不重启当前v92，不重复询问既有切换确认。

根因复现：正式TLS/WSS开满8条计划订阅后，客户端Close在250ms内没有回执，首轮稳定失败。独立Rust客户端、单回环地址也能复现，说明无需SwiftUI重复订阅或多地址竞争即可触发。生产watch只发送内容/心跳，等待项目Condvar，没有消费Close/Ping，旧连接直到后续写失败才释放。

修复：复用现有共享Tokio运行时，将共享变更信号改为Notify，先登记再检查revision，避免丢通知；五条watch循环复用一个WatchWait，等待项目变化、2s心跳或socket可读。TLS/WS缓冲中的Close/Ping及时消费，保持3s写期限、认证/撤销与8连接上限；每watch一个readiness注册，不增加每连接线程/依赖，无高频socket轮询。原50ms合并上限仍保持。普通短请求协议与手机生产代码未改。

当前证据：原始红测0.46s首轮Close超时；单计划绿色0.40s；六类订阅（项目/聊天/终端/审批/计划/成果）满8连接，第9条拒绝，Ping回准确Pong，8次Close与8次TCP异常断开后立即替换通过，整体0.85s。首次扩大测试时遗漏前台快照fixture导致项目不可用，已补合成内容来源，保留失败日志，不放宽生产校验。Close回执本机样本36–103μs，仅代表本机WSS控制帧，不是手机/LAN/Tailscale时延。

范围回归及隔离iOS双地址原生取消/重建与方向提交正在验证，证据目录`target/norma-ios-evidence/v102/`。尚未构建配套v102 Mac；TestFlight仍0.1.0（21），APNs延期，Goal保持active。

### v102 原生与页面验证补充

- 最终Rust回归补充TLS内分片Ping：先发送7/9字节并等待80ms，无误关闭/提前Pong；补齐2字节后返回准确负载。订阅内额外业务帧明确invalid_request、不派发命令；六类订阅、8容量与正常/异常关闭仍通过。
- 首轮原生压力测试将8条探针放入已登记电脑的App测试host，未隔离正常页面自身订阅，出现connectionFailed；该失败保留，不能用它代表总计8条的负载。改用既有PairingProbe独立Keychain与正式配对，不登记App电脑列表，再测总计8条。
- 隔离iOS原生WSS实际跑完24次取消/替换（Rust宿主明确断言完成标记），同时保留2条可用地址与1条TLS停滞地址；无重试写指令，P50=23.52ms、P95=26.13ms、最大28.32ms。证据`native-isolated-verified.log`、`native-watch-latency.json`。这只代表模拟器回环，不是物理LAN/Tailscale。
- 实际方向页面单地址/双地址两轮均通过：预填、只读禁止、控制授权、选优先项、输入/收键盘、一次提交、准确排序和自动返回；正式配对清理通过。单地址临时诊断共43次连接尝试，峰值已有6条、容量拒绝0；v101同流程仍记录2–3次拒绝。临时DEBUG-v102日志已移除，不携带令牌或正文。
- SwiftUI/NormaCore生产代码未变，TestFlight21的96个生产输入逐一hash仍相同；本轮服务端修复无需制造内容不变的TestFlight22。App页面测试与原生探针改动只进入测试产物。
- 共享变化通知1条、订阅范围回归8条通过；原PTY终端E2E1条通过（2条真实Provider测试本轮忽略）。全仓回归与最终Mac Release仍在运行，音频已知失败显式排除，最终结果以汇总为准。

### v102 最终验证状态

全仓串行1795通过、96忽略、0新增失败，仍显式排除既有`test_macos_default_sink_schedules_matcha_pcm`，不能宣称全仓所有测试通过。Norma119通过/1忽略；共享信号1项、订阅范围8项、最终满额/分片控制帧1项、正式原PTY终端1项通过。最终Release编译成功，v102本地验证bundle严格验签通过，未启动；PID37820仍为v92。

临时诊断已清除，行数/noallow/存储/性能/diff护栏通过；`target/norma-ios-evidence/v102/summary.json`汇总结果和全部保留的失败尝试。前次原生菜单Computer Use限制、物理iPhone/双Mac/LAN/Tailscale、空闲开销/耗电和真实云规划质量仍未完成，APNs继续延期；Goal保持active。本轮分类progress（正式修复、红绿证据、原生/页面验证与配套验证包已完成）。

## v103 · 空闲开销与真实验收条件（2026-09-10）

前一Goal回合分类progress：v102服务端正式修复、原生24次重建、两种地址页面流程和验证包均有证据。本回合补空闲CPU/系统流量，未改生产逻辑，不创建重复Mac/iOS包。

复用既有大聊天原生验收，先修正历史证据固定写v61的问题：每次独立创建新的绝对证据目录，可显式指定NORMA_MOBILE_EVIDENCE_DIR；已存在目录拒绝覆盖，检查放在启动原生子进程前。v61原证据hash完整保留。测量脚本原本只统计listener/peer，v102新增等待由共享Tokio运行时承接，因此TDD补实际选择器回归：旧选择漏掉runtime时失败，修正后保留listener/peer/runtime、排除fixture，并继续以getrusage独立校验libproc纳秒单位。

先后两次实际iOS模拟器测量通过；最终含共享运行时的服务线程占用为单核0.5223%，CPU窗口6.03s，共享运行时只占56μs。nettop两次6s间隔样本发送1158字节、接收0字节；客户端收到3次心跳、0次正文更新，完整中文与工具输出仍逐字保留。客户端测试host约0.7681%单核CPU，含100ms测试协调轮询，不能代表完整SwiftUI或电池功耗。原生20次首个完整WSS分片/全文数据在summary.json保留全部样本，不是模型首token或物理跨网时延。

当前条件重新核对：验证App数据库只有3条设置，没有保存规划服务、模型或凭据引用；独立真实云测试环境也未提供配置，已异步询问用户使用哪个服务/模型，凭据仅要求在电脑本地填写。没有读取/复制其他服务凭据，也没有替换成CLI或合成推理冒充云规划验收。iPhone11由devicectl报告unavailable；Computer Use读取Simulator仍报Mac锁屏。Tailscale自身运行且另有在线Mac，但在线不代表该机已有Norma配对和授权，物理双机验收仍保留。既有Mac切换确认未答复，v92会话未中断。

验证：测量脚本自检红→绿、两轮真实iOS大页/空闲/授权流程通过，普通大页WSS回归1通过/2忽略；行数/noallow/diff护栏通过。iOS96个生产输入与TestFlight21一致，v102修改的9个Mac生产文件hash不变；沿用v102全仓1795通过/96忽略及明确排除的音频失败，不为仅测试路径/统计修改重跑整仓。证据：target/norma-ios-evidence/v103/summary.json。Goal保持active，本回合分类progress；真机、完整界面、云规划和切换确认未完成，APNs继续延期。

## v104 · 验收阻塞复核（2026-09-10 18:45 CST）

上一回合仅应用户要求核对并报告剩余任务，分类no progress；本回合再次查验外部条件，分类no progress，连续计数2。文档记录不算开发进展，当前没有可继续等待的构建、测试或上传句柄。

- Computer Use重新选择Simulator，仍明确返回Mac锁屏且自动解锁失败；未进行真实页面点击验收。
- devicectl仍报告iPhone 11（339891E5-A727-5CE0-864C-83C12494B81F）unavailable。
- PID37820仍运行v92验证App。配套v102未启动，既有切换确认仍无答复，不中断现有Agent会话。
- 只读检查验证App设置库：3条设置，无agent.provider.*或voice.openai.*配置；真实规划测试的服务地址、模型及凭据环境变量也均未配置，既有配置问题仍待答复。

剩余真实界面、物理多机/切网/耗电及云模型质量验收仍依赖上述条件恢复。未扩大范围、重复相同测试或发布相同iOS包；TestFlight仍0.1.0（21），APNs继续按用户决定延期。本回合未达到连续三回合阻塞门槛，Goal保持active，不标记完成。

## v105 · 连续三回合阻塞审计（2026-09-10 18:46 CST）

上一回合与本回合均为no progress，连同此前剩余任务核对共连续三回合，没有verified wait。再次实际调用Computer Use仍返回Mac锁屏；devicectl仍报告同一iPhone unavailable；PID37820仍运行v92；只读数据库与环境检查仍无规划模型配置。既有模型配置问题与Mac切换确认均没有新答复，没有活动测试/构建/上传句柄可继续等待。

当前剩余工作不能以模拟数据、重复测试或重复发包替代真实环境验收，已调用update_goal将状态设为blocked。未标记任何G2–G5验收条件完成，未中断用户会话。解锁并接入设备、补齐本地规划模型配置或明确允许切换配套Mac后，可恢复对应验收；第二台Mac仍需建立实际Norma配对与授权。TestFlight保持0.1.0（21），APNs按用户决定延期。

## v106 · Codex紧凑首屏与桌面回看（2026-09-10）

用户在明确知道切换会结束Agent会话后要求继续修复。本轮先一致性备份，正常退出v92；PID37820及29个后代均已退出，再启动v102 PID29278。前后2条mobile_devices记录逐字段摘要一致；用户随后确认已打开dph-plugin，手机服务已开启。

真实Codex0.153.4、80×8原卡复验出现新红测：当前屏幕有已加载model和输入符，但CLI紧凑布局直接省略directory，v102仍等待30s后unknown。复用真实失败画面补单测，不扩大等待时间。共享native_prompt_ready不再要求directory，仍验证最新model非loading、当前运行及paste模式；输入符只在原PTY的实际底屏判断，不能从菜单背后的历史取得。中间实现读取桌面可见区会误伤电脑回看历史，补第二条红测后改用已有有界绝对行接口，保持电脑display_offset不变。没有新增IO、轮询、执行器或网络请求，也没有绕过原审批/草稿保护。

Codex就绪测试按职责移至mobile_codex_readiness_tests.rs，保持每个Rust文件≤500行。真实E2E允许小屏缺少model或directory其中一项，覆盖实际CLI紧凑布局；仍要求准确run/session、首条输入回执、真实完成和唯一PTY。最终真实测试10.74s通过；完整Runtime175通过/3忽略、完整E2E240通过/67忽略，App check与Release（2m12s）通过，行数/noallow/性能/diff护栏通过。不把本轮范围回归称为全仓所有测试通过。

最终本地验证包target/norma-mobile-desktop/v106/Norma Mobile Validation.app严格验签通过，备份完成但尚未启动。Computer Use在项目窗口持续noWindowsAvailable/timeoutReached，已请用户用⌘Q正常退出当前v102；没有通过强杀跳过持久化。96个iOS生产输入与TestFlight21完全一致，无重复上传。证据target/norma-ios-evidence/v106/summary.json，保留初始真实失败、两条单测红灯与最终成功日志。本轮分类progress；实际用户原卡/物理iPhone与整个Goal尚未完成验收，APNs延期。


## v107 · 重新打包桌面并发布TestFlight22（2026-09-10）

用户明确要求重新打包Norma OS桌面并发布iOS，因此本轮重新生成构建22；iOS业务代码与21一致，只更新两份构建版本配置。96项输入hash、归档、导出、严格签名及Apple校验通过。Apple上传成功，Build ID `f1e19ac7-ccd2-410f-84c8-758183e8fa15`；最终回读VALID / IN_BETA_TESTING、未过期、原内测组关联及中文说明一致，未新增邀请/外部测试/公开发布。

桌面通过仓库release入口重新打包：`build/Norma OS_0.1.71_aarch64.dmg`，版本0.1.71/build20260910.115829，192791700字节，SHA256 `6f30c9ebae558dc5401ac294ad5242c054091118a2b195784ec5833dc314c8ce`。Developer ID签名、未公证，严格验签、Finder布局、hdiutil与checksum通过。打包前保留旧DMG；复用既有已验签App中的Sparkle公钥，不生成或输出私钥。

新桌面包包含v106 Codex修复，5项修复输入hash与已通过的Runtime175/E2E240及真实Codex首条输入测试保持一致。IPA为`target/norma-ios-testflight/v107/export-final/Norma.ipa`，5553296字节，SHA256 `a49dbba1dedeb3bf566998931d4964aa0b3888ccab2d532649c6679b1cf13962`。证据汇总`target/norma-ios-evidence/v107/summary.json`。

本轮打包发布请求已完成；未替换运行中的验证实例，未迁移或覆盖数据库。Codex修复需配套更新桌面后验收；物理iPhone、跨网多机、完整界面及云模型质量验收仍保留，APNs仍按用户决定延期，不将整体Goal标记完成。


## v108 · 桌面手机连接入口与英文文案（2026-09-11）

按用户反馈移除欢迎页右上角连接按钮及其独立弹窗，保留项目内Settings → Mobile Connection。该页标题、状态、配对引导、权限、通知配置和对应错误文案统一为英文，复用现有原生控件与设置滚动区。Settings原本仅在打开项目后可用，本轮不扩大该范围。

已有三条设置E2E更新后通过，覆盖欢迎页无入口、设置启用/通知配置与960×480小窗口滚动。期间另一提交恢复了旧页面；本轮保留该提交，在最新代码上恢复14项修改，保存补丁并冻结输入，重新完成全E2E240通过/67忽略、App check、Release及行数/no-allow/diff护栏。证据：`target/norma-ios-evidence/v108/summary.json`。

Computer Use点击隔离桌面返回noWindowsAvailable，真实界面验收未完成。隔离进程及其子进程已退出，用户正式Norma未替换；本轮未重新打DMG、未上传iOS，TestFlight仍22，APNs继续延期。


## v109 · 终端自适应与聊天排版（2026-09-11）

终端取消双向滚动和固定桌面行宽，保留正文、历史和原卡输入绑定；手机端自然换行，不resize电脑PTY。聊天复用ProviderMark与黑色表面，增加正文行距、用户气泡缩进、工具图标/状态层级及紧凑输入区；原生AttributedString处理Agent内联Markdown，用户输入保留原文。动态字号和工具完整展开保留，发送图标固定在44pt按钮内。

验证：旧实现红测捕获5项实际格式/宽度错误；修改后6项原生UI通过，覆盖长中文/路径、旋转、AX5、工具完整输出/状态更新、流式内容、发送清空与自动跟随。Swift58、Rust E2E240通过/67忽略；Release归档/导出、IPA严格签名、production推送环境及Apple校验通过。大字号工具测试按LazyVStack实际回看流程修正定位，未降低完整边界/触控断言。Computer Use能打开Norma，但内容点击持续noWindowsAvailable；不计为完整CUA通过，已核对XCTest实际截图。临时模拟器仅配对隔离TLS服务，正式忘记后关闭；原模拟器已有真实电脑配对，未清除。

构建0.1.0（23）已上传（Delivery e021ba89-33e7-4fc4-afde-f9ff50b5e975）；随后用户明确取消自动发布。未继续加密申报或关联原内测组，API回读原组无23。此后仅按用户明确发布指令执行。IPA：`target/norma-ios-testflight/v109/export/Norma.ipa`；完整证据：`target/norma-ios-evidence/v109/summary.json`。v108桌面14项输入保持一致，运行中的Mac应用未替换。


## v110 · 移除项目脉络（2026-09-11）

按用户截图移除项目页底部整块项目脉络，包含方向、汇总、跳转按钮及记录与成果；同时删除该区域专属的计划订阅和无用回调，原卡片选项仍能查询未确认操作。回归发现最大辅助字号输入时聊天空间不足，键盘开启时输入区改为单行滚动，五行原文与动态字号均保留。

本地编译与5项原生UI通过，覆盖默认/AX5页面与聊天输入、项目切换、指挥台实时计划、创建卡片和未确认操作保留；两轮真实隔离TLS配对与正式忘记清理通过。Computer Use打开真实Norma并确认目标区块已移除，截图`target/norma-ios-evidence/v110/project-page-cua.png`。未上传或发布TestFlight；完整证据`target/norma-ios-evidence/v110/summary.json`。

## v111 · 紧凑项目标题（2026-09-11）

项目名称直接作为切换入口，与创建按钮同排；删除独立“项目”标签、大标题和重复电脑/同步行及无用同步时间状态。沿用黑色配色、原生项目选择面板和按压反馈；Apple Design 落实为内容优先、20pt semibold 动态字号、44pt触控区域和完整VoiceOver项目名，辅助字号允许标题两行。普通字号标题行与下方筛选间距不超过56pt，不再保留旧区块空位。

本地编译通过，4项原生UI流程最终通过：项目/电脑切换、创建入口及只读保护、三主页层级、普通/最大辅助字号标题和项目范围。首轮辅助字号在未改动的指挥台菜单点击后仍留在菜单，原代码复跑通过，首次失败记录保留，不将其算作首轮全绿。Computer Use 已实际打开 Norma、点击项目名并切换至第二项目，确认新标题与关闭项目状态；证据位于 `target/norma-ios-evidence/v111/`。本轮未上传或发布 TestFlight。

## v112 · 玻璃筛选与底栏（2026-09-11）

任务状态筛选和底部三入口共用玻璃选中样式：iOS 26 使用 SwiftUI 原生 `glassEffect`，旧系统使用 Material。保留黑色、居中的 N 指挥台和现有尺寸；选中胶囊使用匹配几何与0.28s响应弹簧，动画只作用于选择控件。底栏移至共享根导航，三主页不再各自创建底栏，进入聊天时仍隐藏；不新增网络请求、定时器或依赖。

依据 Apple Design、Emil Design Engineering 与 review-animations：材质表现层级，移动表达选择位置；保留即时按压与选中语义，减少动态效果时改为0.12s淡化，降低透明度时使用不透明背景。真实录屏抽帧确认两处选中胶囊连续移动；辅助字号审计通过。iOS 17–25材质回退和系统降低透明度/减少动态效果开关本轮仅代码检查，未作运行时验收。

本地编译、4项原生UI流程最终通过：项目/电脑切换与只读创建、三主页层级、辅助字号项目范围、任务实时筛选及聊天返回。首轮保留两类失败证据：已知辅助字号指挥台菜单点击复跑通过；任务测试原先因第二项目已有不可读提示而提前断言，改为等待第一项目的WSS失效更新后通过。隔离TLS配对及正式忘记清理通过。Computer Use 实际切换四种筛选和三个Tab，选中态与结果一致；录屏、抽帧和简短预览位于 `target/norma-ios-evidence/v112/`。本轮未上传或发布 TestFlight。

## v113 · 移除逐项目错误提示（2026-09-11）

移除任务页底部“项目名：项目尚未在电脑上打开”等橙色逐项提示，保留统一的“部分任务暂时无法读取”状态和自动恢复订阅。遵循 Apple Design 的清晰层级，避免重复错误信息挤占任务内容。

本地编译、任务筛选/聊天返回/读取失败与恢复的聚焦UI流程通过；隔离TLS配对与正式忘记清理通过。Computer Use 实际打开 Norma 任务页，确认真实任务及统一状态仍可见、逐项目提示已移除，证据位于 `target/norma-ios-evidence/v113/`。本轮未上传或发布 TestFlight。

## v114 · 离线移除电脑（2026-09-11）

移除原先“先联网关闭提醒才允许忘记”的前置条件。确认后直接删除手机Keychain与电脑元数据，停止通知续登记；仅在内存中临时保留一次清理请求需要的凭据，在线时后台尝试关闭提醒，离线不保留凭据重试。已有提醒请求先完成再清理，避免重新开启；电脑端授权仍需在Mac撤销，确认框明确说明。

按Apple Design的直接操作与可取消原则，增加每行44pt菜单入口，左滑操作明确为红色垃圾桶，使用有明确取消按钮的原生确认框。删除当前电脑后自动选择剩余电脑；此自动切换不再用连接错误弹窗打断管理列表，主动连接的错误提示保留。

本地编译、Swift核心58项、两台离线电脑的取消/移除/重启UI、Keychain与通知续登记清理检查通过。真实隔离TLS验证本地凭据删除后仍能关闭在线Mac提醒；测试配对正常撤销。回归曾捕获自动切换弹错和推送身份校验顺序问题，已修复并复测。Computer Use实际通过菜单删除离线A、确认B仍在列表、再删除最后一台B至空态；截图位于 `target/norma-ios-evidence/v114/`。坐标拖动验收返回noWindowsAvailable，左滑交互由XCTest验证。本轮未上传或发布TestFlight，未修改用户真实配对。

## v115 · 原生键盘收起交互（2026-09-11）

原先只有局部收起按钮和滚动手势，点击导航栏、表单说明等外部区域不会结束编辑。现在由当前App窗口的一个原生轻点识别器统一收起键盘，覆盖页面和Sheet；输入框及其子视图不触发收起，切换字段和移动光标继续可用。识别器不取消按钮触控，并为长按文字选择让步，视图离开窗口时移除。

按Apple Design与Emil Design Engineering的直接操作、即时反馈原则，沿用系统键盘动画与交互下滑；聊天、终端和验证码页保留44pt明确收起入口。地址Return进入验证码，数字键盘也可随时关闭。收起不更改草稿、不执行发送。

配对外部轻点测试先在旧实现失败，修复后通过；4项聚焦UI回归覆盖配对字段切换/前导零、聊天和终端按钮、标题外部轻点、工具卡切换、长按复制、下滑与草稿保留。首轮工具测试因隔离服务未启用决策数据失败，启用原有fixture后完整通过，未以跳过代替验收。Computer Use实际触控标题及工具卡，确认键盘消失、工具状态切换、原文保留，并验证可继续输入。文字选择与拖动由XCTest验证，物理iPhone仍待用户验收。

两次隔离TLS生命周期及正式忘记电脑清理均通过，专用模拟器已关闭，未操作用户真实配对。证据在 `target/norma-ios-evidence/v115/`；本轮未上传或发布TestFlight。


## v116 · 卡片聊天发送图片（2026-09-11）

按 Apple Design 的原生选择、直接预览与内容优先原则，输入框旁增加44pt“添加图片”，使用系统 PhotosPicker；每条最多4张，可预览、缩放和移除，也可只发图片。照片在后台按方向缩小到2048px、重新编码为JPEG并移除EXIF/位置数据，每张最多2MiB。草稿图片独立落盘，编辑文字不重写图像；只有准确版本的成功回执才清理，新版本/未知回执保留。图片重新选择使用独立本地文件身份，避免迟到写入误删相同摘要的新图。

Mac增加独立鉴权WSS图片传输，每片64KiB，保留256KiB帧上限；上传需当前项目control权限，读取允许read，逐片重验授权。校验尺寸、格式、SHA256后原子保存，拒绝越界/错位/坏摘要，取消不落半文件；全机图片存储最多256MiB/2048目录文件项。发送仍复用原有卡片、PTY、幂等命令与队列，未创建额外Agent执行器。聊天和消息队列隐藏附件协议正文，显示原文与可重新从电脑读取的图片。

真实Claude曾把裸图片路径转换成`[Image #1]`并删除路径，导致严格回执与历史预览失败。改为JSON数组表达文件引用，保留原有逐字回执校验，未放宽为“任意Hook即成功”；纯图片在Codex去首尾空白后也能恢复缩略图。真实Codex读取像素中的`TEST-4829`通过；Claude准确发送成功且Read工具返回image内容，但当前配置模型`ark-code-latest`报`400 Model only support text input`，因此Claude端到端视觉回答未通过，需要支持图片的模型。未改用户模型配置。

Swift Core 61项与完整E2E 241项通过/69忽略，App检查和iOS模拟器构建通过。图片网络回归覆盖只读拒绝、超限、错位、坏摘要、中断无半文件、精确回读、跨项目及撤销；前期Computer Use实际选图/预览及XCTest纯图片发送通过。最终纯图片UI发送/草稿清空/历史回读与全图打开通过，Computer Use再次打开已发送图片验证；隔离测试电脑已正式忘记，服务正常退出，专用模拟器已关闭。初次草稿seed因fixture缺少NORMA_DECISION_UI未包含card7失败，正确启动后复测通过。完整结果见本轮证据`target/norma-ios-evidence/v116/summary.json`。隔离测试不替代物理iPhone/LAN/Tailscale；用户当前运行的Norma未重启，使用图片传输需配套更新Mac和iOS。本轮未发布TestFlight。


## v117 · 运行卡片动效性能（2026-09-11）

本轮只修改iOS动效。项目卡片边框和Claude图标原先各用30Hz TimelineView重建SwiftUI绘制内容；多卡同时运行时形成持续CPU负载。现在用Core Animation的transform动画播放静态圆锥渐变和品牌图标，固定圆角遮罩只在尺寸变化时更新。边框保留3秒循环、Claude保留8秒旋转；Codex改为轻缓的缩放/±4°摆动。标题、任务进度更新不会重启动画，后台、任务结束和离屏都会移除循环；减少动态效果沿用静态图标和边框，不改变状态文字。

设计和验收参考Apple Design、Emil Design Engineering及review-animations；保留有目的的运行状态反馈，不增加手势延迟、模糊或布局动画。此次为明确卡顿的局部修复，不执行系统性动效重构。

| 原行为 | 当前实现 | 原因 |
|---|---|---|
| 每卡30Hz刷新边框渐变 | 固定遮罩+系统旋转合成层 | 消除SwiftUI逐帧重绘与主线程调度 |
| Claude逐帧旋转、其他图标phaseAnimator | 系统transform循环 | 共用低开销生命周期，保持品牌图形 |
| 视图刷新可能打断动画 | 已有动画不重新add，保留beginTime | 状态内容刷新不跳回起点 |
| 减少动态效果/后台及离屏 | 停止循环，保留静态状态 | 降低耗电并保留可访问性反馈 |

审查结论：本地范围Approve。6张实际AgentSnapshotCard的模拟器测试中，6秒区间的进程CPU由1.896699秒降至0.007534秒，另一无CUA采样为0.008578秒；带Computer Use观察的一次采样0.918973秒单独保留，不混入干净对照。这是测试host进程CPU，未统计渲染服务GPU，也不是物理iPhone帧率/耗电承诺。

原实现聚焦测试失败，新实现2项本地测试通过：检查真实presentation角度推进、相同内容更新保留起始时间、后台/完成/离屏停止、回前台恢复12个装饰动画，以及两种品牌图标仍可读取。Computer Use实际观察原生App中的6卡窗口，并保存两帧边框/图标变化。系统“减少动态效果”开关未单独操作验证，其关闭动效条件与原实现一致。测试窗口已回收，没有建立测试配对或运行Agent；证据见`target/norma-ios-evidence/v117/summary.json`。未发布TestFlight。

## v118 · 按用户要求发布TestFlight24（2026-09-11）

用户明确要求发布后，查询Apple最新构建为23，冻结103项iOS生产输入并重新归档为0.1.0（24）。Release归档/导出、严格分发签名、production entitlement、Apple校验及单次上传通过；上传后源码与IPA摘要保持一致。保留此前停止分发的23，不将其关联原组。

Build ID `42c0cb94-7274-48bd-a844-d31fb3b32ca3`，Apple回读VALID / IN_BETA_TESTING、未过期，原「Norma 内测」组关联和中文测试说明均核实。IPA `target/norma-ios-testflight/v118/export/Norma.ipa`，5662669字节，SHA256 `3540b664ff3706fcc29e05bb40a83128122bf74381ba98f97bc37ddfbd646520`。证据：`target/norma-ios-evidence/v118/summary.json`。

本次包含v109–v117的iOS界面、离线移除、键盘、图片发送与动画优化，复用已完成的范围验证。未新增邀请、外部测试或公开发布；未重启/打包Mac。内测说明明确图片功能需配套新版桌面、当前Claude模型仅支持文字，真实iPhone/LAN/Tailscale验收仍由用户继续；APNs延期，不将本轮发布等同于整个Goal验收完成。

## v119 · 打包配套桌面（2026-09-11）

按用户要求通过`./scripts/release-local.sh`重新编译、签名和打包当前桌面源码，包含与TestFlight24配套的图片传输。版本保持0.1.71，新构建20260911.085840；Cargo/Packager版本配置未改。Developer ID签名、未公证；严格签名、DMG完整性、Finder布局和相邻SHA256通过，1467项构建输入保持一致。

产物`build/Norma OS_0.1.71_aarch64.dmg`，192861255字节，SHA256 `0b9db165abdb49dcf62f7fa8cd775b4bae00969f43efe3eb13dbc953094a2877`。首次create-dmg在旧安装镜像仍挂载时选错设备，卸载尝试失败；核对实际镜像身份后仅正常弹出旧Norma安装卷，再走同一正式打包入口成功。旧DMG和失败日志已保留。证据`target/norma-ios-evidence/v119/summary.json`；未安装或启动新包、未公开发布，既有Claude视觉模型与真机验收限制保留。

## v121 · 统一iOS加载居中（2026-09-11）

项目/任务/对话/会话记录/成果/变更/草稿/计划/通知定位的首次读取统一放在List或ScrollView之外，按可见内容区域居中；追加读取和局部核验水平居中，按钮内spinner和图片上传进度保留原生位置。复用WorkspaceStatusView文件中的共享加载组件，避免各页重复间距；保留导航点击和系统安全区，符合Apple一致反馈与内容优先原则。无网络、存储或审批逻辑变化。

聚焦测试先复现项目spinner y168偏离内容中心y451，修改后3项原生布局测试通过，覆盖项目、标题下的任务滚动区、最大辅助字号列表及加载完成移除。Computer Use观察居中项目页并打开创建面板，未读项目的实际创建仍禁用；临时窗口自动回收。任务页Computer Use错过短暂展示窗口，不计为人工界面通过。证据`target/norma-ios-evidence/v121/summary.json`与`loading-centered-cua.png`；未发布TestFlight，专用模拟器验收后关闭。

## v122 · 终端竖屏重排与完整按键（2026-09-11）

原PTY手机投影保留软折行标记及行尾空格；iOS终端与聊天实时预览只连接明确的自动折行，让系统按手机宽度排版，保留硬换行/空行。增量事件包含完整页的标记，旧协议保持原换行，不改变共享电脑PTY尺寸。底部改原生自适应网格与键盘符号，普通竖屏两行、宽屏单行、辅助字号三列，保留44pt触控区域和VoiceOver名称。Apple设计落地：按可用宽度响应布局、系统图标/动态字号、直接可见的控制键。

原PTY红测准确复现缺少折行标记；536项Rust、62项Swift测试及原生竖屏/横屏/AX5、长中英文、9键边界、键盘展开/收起测试通过。Computer Use可打开真实App并读取项目画面，但卡片点击报noWindowsAvailable；重连激活仍失败，未声称完成CUA终端点击。截图已复核；隔离服务首次遗漏NORMA_DECISION_UI，清理后按正确配置重建，最终正式忘记测试配对。证据`target/norma-ios-evidence/v122/summary.json`。正文重排需配套新版Mac，旧Mac不猜测换行；未替换运行中的桌面、未发布TestFlight，真机待后续验收。

## v123 · 底部导航距屏幕底边8pt（2026-09-11）

共享TabView允许导航底栏进入container底部安全区，并在屏幕底边保留8pt；相较v120的34pt安全区间距，再下移约26pt。仍尊重键盘安全区，不修改独立对话页。原生模拟器构建通过，Computer Use完成项目→指挥台→任务→项目点击与选中状态核验；三页底栏位置一致。证据`target/norma-ios-evidence/v123/summary.json`、`bottom-navigation-cua.png`。未发布TestFlight。

## v124 · TestFlight 0.1.0（25）发布（2026-09-11）

2026-09-11T18:22:31+08:00确认构建25已由Apple处理为VALID / IN_BETA_TESTING，原「Norma 内测」组关联与中文说明回读一致。Build/Delivery ID `796e428a-34b5-4a41-9577-027f79530f34`。本轮包含v120–v123的加载居中、终端软折行/竖屏按键、底栏距底边8pt。沿用已有加密实现和不向法国分发的确认，未新增邀请、外部测试或公开App Store发布。

Release归档、导出、严格签名、Apple校验/上传通过；103项发布输入及IPA上传后hash一致。IPA：`target/norma-ios-testflight/v124/export/Norma.ipa`，5679727字节，SHA256 `5bee930651634070b9fa2e76e10539dfd6f192a565354dc202cabd5e4d063400`。复用v121加载布局、v122 Rust536/Swift62及原生竖横屏/AX5/键盘、v123 Computer Use三页导航验证；未把v122工具点击故障当作CUA通过。

测试说明已注明完整终端软折行重排需配套新版Mac；旧Mac仍按原行显示，本次未打包/替换运行中的桌面。APNs仍延期，真机/LAN/Tailscale由用户更新后验收。完整证据`target/norma-ios-evidence/v124/summary.json`及`target/norma-ios-testflight/v124/`。

## v125 · iPhone锁屏恢复底栏位置调查（2026-09-11）

用户确认锁屏对象为iPhone／模拟器。新增聚焦原生UI测试，逐页执行Home→activate，恢复后不点击先检查选中态与底边坐标，避免切Tab重排掩盖问题。空配对基线及真实隔离TLS配对项目均通过：项目、指挥台、任务按钮底边距屏幕12pt（8pt外距+4pt玻璃内边距）；项目目录清空再加载后仍保持正确。`navigation-red.log`实际为通过的基线，不是红测。

真正Sleep/Wake锁屏尚未执行：Computer Use两次返回Mac锁定，已请求用户解锁宿主Mac。根因未确认，本轮无生产代码修复，不宣称已解决。隔离配对、正式忘记与服务回收通过，用户桌面会话未操作。未发布TestFlight，仍为0.1.0（25）。证据：`target/norma-ios-evidence/v125/summary.json`；解锁后继续真实锁屏前后位置及玻璃选中态验收。

## v126 · 精简待决策页重复信息（2026-09-11）

删除「电脑状态 · 已同步」折叠行及单独的「N项待确认」行，移除对应额外顶部留白。数量统一由顶部分段显示，连接未知仍显示「待核验」并提供VoiceOver值；不改变审批权限、卡片、跳过与任务详情行为。按Apple设计原则减少重复信息，让决策内容紧接分段。

原生构建、原任务打开/目标替换拒绝、未知数量与跨项目收件箱两项流程通过；两次隔离配对及正式忘记/服务回收通过。初次fixture启动因配对前标记失败，随后一轮因第二测试项目未打开导致收件箱检查失败，修正测试环境后全流程通过；生产超时/断言未放宽。Computer Use仍报告Mac锁定，未宣称真实点击验收完成。未发布TestFlight，v125锁屏恢复调查仍待继续。证据：`target/norma-ios-evidence/v126/summary.json`。

## v127 · TestFlight 0.1.0（26）发布（2026-09-11）

2026-09-11T19:39:31+08:00确认构建26已VALID / IN_BETA_TESTING，现有「Norma 内测」组及中文说明回读一致。Build/Delivery ID `15961c37-e419-4cb3-9a05-3136cee85d23`。本轮仅包含v126决策页重复电脑状态/数量的精简、留白与分段读数改动，另更新构建号；相对构建25的103项发布输入仅CommandCenterView与两份版本配置变化。复用v126已通过的原生流程、隔离TLS/WSS配对及清理证据。

Release归档、导出、严格签名、Apple校验和单次上传通过；上传后输入与IPA哈希一致。IPA：`target/norma-ios-testflight/v127/export/Norma.ipa`，5660385字节，SHA256 `1511e4f8cca7ceed708777436b442363ebe8479453027e1be9eb659eef072c94`。沿用既有加密实现及不在法国分发范围，没有新增邀请或公开发布。测试说明已明确锁屏恢复底栏问题仍未修复；Computer Use因Mac锁定未完成，物理iPhone由用户验收。未重启或替换桌面，APNs仍延期。证据：`target/norma-ios-evidence/v127/summary.json`。


## v128 · 键盘退出后主页底栏偏移修复（2026-09-11）

用户确认触发操作为键盘打开时锁屏或切后台。原生回归两次稳定复现：带聊天键盘侧滑返回后，按钮底边距屏幕从12pt增为60pt，等待3秒仍有48pt额外空白。将底栏移出TabView、隐藏bottomBar、替换TabView均不能解决，已全部还原。普通后台旧版基线通过，未将其声称为红测；旧版真正锁屏没有同条件红测证据。

最终生产仅修改RootView一处安全区范围：主页Tab内容由只忽略container改为忽略全部底部安全区，避免响应退出键盘后的残留占位。NavigationStack及推入的聊天、终端、Sheet保持原生键盘避让。恢复原生TabView，不增加状态重建、键盘通知控制器或负偏移。相对v127的103个生产输入仅RootView变化；还原这一行和注释即可获得v127相同SHA256。Apple Design落地为稳定8pt外距、一致玻璃导航和原生输入行为。

先红后绿的键盘返回回归通过；最终4项原生UI回归通过，覆盖等待实际runningBackground再恢复、侧滑/按钮返回、三主Tab、验证码Sheet、终端横竖屏与辅助字号，并明确断言输入不被键盘覆盖。额外系统锁屏验收通过：CUA观察真实聊天键盘，点击Simulator Device→Lock并保存锁屏画面；XCTest确认App处于后台、activate唤醒并返回项目，未切Tab前测得按钮底距12pt（8pt外距+4pt轨道内距）。两次60秒操作窗口超时后，仅将测试远程操作预算改为180秒；生产等待与位置断言未放宽。

Computer Use部分App坐标点击仍报noWindowsAvailable，因此本轮是CUA系统锁屏与原生XCTest恢复测量配合，不冒充纯CUA完整点击或物理iPhone/FaceID验收。两轮隔离TLS配对、正式忘记和服务回收均通过，未发送Agent命令、未操作用户桌面会话或真实配对。未发布TestFlight，构建号仍为26，线上不包含本修复。证据：`target/norma-ios-evidence/v128/summary.json`；锁屏恢复截图：`target/norma-ios-evidence/v128/after-keyboard-lock.png`。

## v129 · 就绪Claude卡明确拒绝后恢复发送（2026-09-11）

已复现一个确定路径：ChatComposer收到经过校验的rejected终态后仍保留pending，按钮持续禁用，形成“卡片就绪但无法再发送”。现在仅移除该已拒绝操作，保留原文和附件草稿；复用现有回执错误映射，在输入框上方直接解释电脑待处理输入等原因。outcome_unknown仍保留pending，重连和重启只查询，不自动重发。Apple Design落地为就近反馈和原生输入，不增加页面或入口。未证实这就是用户此次手机问题的唯一根因。

聚焦原生回归先红后绿：卡7明确显示Claude/就绪；第一次被拒绝后恢复按钮且保留原文；第二次未知后重启仍保持保护；仅在测试已核实fixture从未执行、通过正式确认清除记录后才主动重试，最终仅收到一次准确原文，成功才清空草稿。最终1项通过，耗时24.313秒。第一次修复测试的原文失败来自红测草稿被测试重复输入，修正测试并重建隔离服务后完整通过。完整E2E为241通过、0失败、69忽略，格式、行数、禁止allow及diff检查通过。

已复核原生测试截图：提示位于输入框上方、原文完整、发送按钮恢复。Computer Use可读取真实项目画面，但点击Claude卡仍报-10005 noWindowsAvailable，不能计为完整CUA点击验收。两轮隔离TLS服务及正式忘记测试电脑均通过，服务退出0，专用模拟器已关闭；没有启动真实Provider、操作用户配对或重启桌面。证据：`target/norma-ios-evidence/v129/summary.json`。未发布TestFlight，构建仍为26；用户实际发送表现、Claude硬换行/框线终端排版及原Goal真机/跨网/APNs未验收项继续保留。

## v130 · TestFlight 0.1.0（27）发布（2026-09-11）

2026-09-11T21:18:40+08:00确认构建27已VALID / IN_BETA_TESTING、未过期，原「Norma 内测」组及中文说明回读一致。Build/Delivery ID `6c7a25de-f9f9-4a46-8dfa-f0fc39079334`。按用户明确要求发布v128键盘退出/锁屏恢复底栏位置与v129就绪卡明确拒绝后恢复发送；草稿保留及未知结果防重发保护维持。相对构建26仅RootView、ChatComposer、CardCommandSheet与两份版本配置变化，103项生产输入冻结。

Release归档、导出、严格分发签名、Apple校验及单次上传通过，上传后源码和IPA哈希一致。IPA：`target/norma-ios-testflight/v130/export/Norma.ipa`，5663876字节，SHA256 `fc6527687616f474d354abee35f2f67d47245543a697f109827c2483ec443a74`。复用v128四项原生回归与CUA系统锁屏配合XCTest恢复测量、v129原生拒绝/未知/主动重试回归及完整E2E241通过、0失败、69忽略的证据；本轮仅发布，没有扩大为真机验收通过。

沿用已确认的加密实现及不在法国分发范围，没有新增邀请、外部测试或公开App Store发布。中文测试说明明确Claude自绘硬换行/框线手机排版仍待优化，真实iPhone及LAN/Tailscale由用户继续验收，APNs仍延期；未重启或打包桌面。证据：`target/norma-ios-evidence/v130/summary.json`及`target/norma-ios-testflight/v130/`。

## v131 · 移除任务列表冗余读取提示（2026-09-11）

按用户要求删除“部分任务暂时无法读取”及其说明；保留不可读项目状态，防止误显示“当前没有任务”或“没有匹配的任务”。仅改任务列表显示分支，订阅、重连、权限及筛选不变；按Apple Design内容优先原则减少冗余信息。

更新已有原生流程断言，原实现两条断言失败，移除后完整流程34.595秒通过；覆盖筛选、搜索、原对话返回、只读保护、不可读清除旧数据及恢复自动显示。Computer Use成功打开App并点击任务Tab，读取不可读与恢复后的真实画面；追加筛选点击报noWindowsAvailable，其行为由原生回归覆盖。原生构建与diff检查通过；隔离TLS配对、正式忘记及服务回收完成，专用模拟器已关闭，未操作用户电脑或配对。

证据：`target/norma-ios-evidence/v131/summary.json`，截图`tasks-unavailable-cua.jpg`与`tasks-restored-cua.jpg`。未发布TestFlight，线上仍为0.1.0（27），本轮显示修改仅在本地。

## v132 · 项目切换合并任务搜索（2026-09-12）

任务标题下不再单独占一行显示项目选择；原生Menu/Picker位于原胶囊搜索框左侧，右侧搜索任务或Agent，原有筛选/清除及未知状态判断保持。按Apple Design内容优先、动态字号和可访问性原则实现：普通字号单行，辅助字号允许项目名两行并随内容增高；选择器保留44pt触控区域及完整读屏值。未新增依赖或改变数据权限。

旧布局先红（选择器与搜索中线相差56pt）。最终普通/AX5项目切换和文字裁切/触控/描述审计45.806秒通过；原型三页布局20.667秒、任务搜索/清除/状态更新/只读/对话返回/不可读恢复35.806秒通过。Computer Use已实际打开任务页、切换第二项目及恢复所有、输入Codex并清除，结果与筛选一致；原生构建与diff检查通过。此前未修改的指挥台菜单测试失败未计作本轮全量通过。

两次过期隔离fixture均通过正式身份核对清理；最终正式配对、UI忘记及服务回收退出0，专用模拟器已关闭。没有操作用户Mac会话或真实配对。证据：`target/norma-ios-evidence/v132/summary.json`；标准及辅助字号截图`tasks-standard.png`、`tasks-accessible.png`，CUA截图`tasks-final-cua.jpg`、`tasks-search-cua.jpg`。未发布，构建号仍为27；TestFlight暂不含v131/v132。

## v133 · Claude聊天持续加载排查（2026-09-12，未复现）

只读检查：现有WSS增量/撤销测试4.10秒通过；调用实际Claude解析和手机投影，当前已有回复的会话1毫秒读取3条，has_more=false；最近新建的node27/32记录文件尚未生成，读取立即返回NotFound。Computer Use查看了真实桌面已有回复Claude和新建Alan，但没有用户手机的持续加载复现证据，不能把文件尚未生成直接认定为根因。

已询问具体故障卡片和同一卡片终端是否正常，等待确认后继续针对准确路径修复。未修改生产代码、未发Agent命令、未重启桌面或发布TestFlight；临时只读诊断代码移至证据目录。证据：`target/norma-ios-evidence/v133/summary.json`。v132已完成状态保留，整体Goal未标记完成。


## v133 · Claude 空实时输出持续加载修复与发布授权（2026-09-12）

用户已明确授权“完成后发布一下 TestFlight”。已复现的路径：原生聊天实时通道返回24行空屏后，`LiveAgentOutput`仍以正文是否为空判断是否连接，因此持续显示加载；收到有效页和收到正文是不同状态。增加一个已收页状态，首次有效空屏后显示等待Agent输出，后续正文和完成历史继续自动更新。没有改动协议、权限或用户桌面。

`empty-output-red2.log`先确认红灯，`claude-loading-green.log`3项原生回归通过；62项Swift Core通过、相关WSS/权限/连接回收检查通过；Computer Use实际点击Claude卡并观察空屏→正文，截图已保存。历史不可读测试同时确认有限重试约7秒结束并可手动恢复，不能把尚无历史文件认定为唯一根因。用户尚未确认实际故障卡片及同卡终端情况，真实iPhone仍待验收。

完整桌面E2E初次受本地HTTP代理502等影响，进程内去除代理并串行后214通过、3项会话恢复失败、60忽略；未扩大为完整通过。此次发布输入相对构建27仅LiveAgentOutput、TasksView、WorkspaceStyle和两份版本配置，保留v131/v132成果。103项输入冻结，构建28Release归档/导出及严格签名通过，等待Apple校验与上传。正式配对/忘记/fixture回收通过，专用模拟器已关闭，真实Mac及配对未变。详细证据见`target/norma-ios-evidence/v133/summary.json`。

2026-09-12T11:30:11+08:00：TestFlight 0.1.0（28）已发布。Apple回读VALID / IN_BETA_TESTING，Build ID `684f833d-f604-4026-ba34-fa661d3db5c3`，原内测组与中文说明确认一致；归档、导出、分发签名、Apple校验/上传及103项生产输入/IPA上传后哈希检查通过。未新增邀请、未公开发布、未重启桌面。用户实际Claude故障唯一根因仍未确认，完整Goal的真机与桌面未验收项保持未完成。
