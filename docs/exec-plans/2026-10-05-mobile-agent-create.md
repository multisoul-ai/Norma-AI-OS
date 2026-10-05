# 手机快捷创建智能体：实施与验收

日期：2026-10-05。分支：`dev`。范围：本地 Mac、iOS App 与 App Intent；不部署云服务。

## 实现边界

- 群聊顶部 `＋`、工作空间设置 `新建智能体` 共用原生创建 Sheet；保留添加已有 Agent。
- 仅选 Codex / Claude / Cursor 与真实项目区域；单项目自动选中，无项目引导关联已有项目。名称由 Mac 现有名称注册表分配。
- App Intent「创建 Norma 智能体」使用相同创建服务，参数持有 Mac / 工作空间 / 区域身份。`openAppWhenRun=false`，要求系统设备认证；没有独立扩展。
- 新建不产生聊天消息或任务。真实定义、真实画布卡片、进程启动状态和目录由 Mac 管理；手机凭回执显示结果。
- 创建已成功但目录更新失败时仍保留成功，显示成员同步中，不诱导再次创建。提交后「关闭」仅关闭页面，不撤销已提交操作。

## 代码与依赖

| 位置 | 职责 |
| --- | --- |
| `crates/norma/src/mobile/agent_creation.rs` | 请求身份、类型、版本与禁止附带任务的校验 |
| `crates/storage/src/agent_creation_store.rs` | 持久操作账本、原始参数冲突、唯一 claim、定义/卡片绑定、重启核查 |
| `crates/app/src/mobile/workspace_chat.rs` | 直连与中继共用创建/回执业务入口，精确授权与目录 |
| `crates/app/src/mobile/workspace_live/creation.rs` | 精确区域加载、现有 quick Agent 定义、真实卡片与后台启动、局部失败清理 |
| `ios/Sources/NormaCore/AgentCreation.swift` | UI / Intent 共用 actor；持久 pending，未知结果查询原操作，离线不排队 |
| `ios/Sources/NormaCore/AgentCreationAPI.swift` | `workspace_agent_create` / `workspace_agent_creation_receipt` 的双连接适配 |
| `ios/App/AgentCreationEnvironment.swift` | 页面外可用的凭据、目录和创建服务生命周期 |
| `ios/App/AgentCreationSheet.swift` | 两项选择、进度/禁用/未知查询/成功与目录同步反馈 |
| `ios/App/CreateNormaAgentIntent.swift` | 真实目录 AppEntity、参数依赖、快捷指令和 Siri 入口 |

能力标记：`workspace_agent_create_v1`。旧 Mac 缺少能力时提示更新；直连不要求云端模型授权。

依赖沿用：iOS UI → NormaCore → 直连/中继 → Mac 手机接口 → workspace / runtime / storage；域层不依赖 GPUI。文件、SQLite、目录检查与进程准备在后台；GPUI 只更新状态和卡片。

## 可靠性

1. 实时预检目录与能力后，将稳定 `operation_id` 和原参数先保存到手机。
2. Mac 持久登记并唯一领取操作，在下一项副作用前绑定真实定义和卡片身份。
3. 换连接或响应丢失先查原回执；未确认不得换新操作重复创建。
4. Mac 重启将进行中的创建标为结果待核查；只允许比对原区域、定义、卡片后完成回执，不自动重放。
5. 普通离线保留选项但不排队；明确拒绝解除 pending；失败仅清理本次定义与卡片。
6. 准备前重查授权和配置；撤权保留 `unauthorized`，不误报为工具启动失败。

## 设计落地

遵循 `apple-design` 与现行 iOS 样式规范：浅底、系统字号和控件、48pt 左右的原生主操作、系统导航和可拖动 Sheet，玻璃仅用于功能层。medium Sheet 必须同时看到类型与完整项目行；大字号可展开/滚动。返回保留聊天草稿和原生阅读位置；进度反馈即时，减少动态效果时取消 spring。

## 验收矩阵

| 行为 | 证据入口 |
| --- | --- |
| 精确身份、允许的三种类型、禁止附带任务 | `crates/norma/tests/agent_creation.rs` |
| 相同操作只 claim 一次、参数冲突、重启不重放、未知按原卡片恢复 | `crates/storage/tests/agent_creation.rs` |
| 当前与未来项目授权、准备前配置冲突/撤权、不得启动第二次 | `crates/app/tests/unit/workspace_agent_creation_tests.rs` |
| 首次旧库并发打开不会重复 ALTER 时间字段 | `crates/storage/tests/workspace_chat_time.rs` / `mobile_commands.rs` |
| 未知恢复、直连转中继、离线/旧版本不排队、明确拒绝后可再次运行、草稿 | `ios/Tests/NormaCoreTests/AgentCreationTests.swift` |
| 两次独立 Intent 在聊天页未打开时各创建一个真实卡片，无任务 | `ios/DeviceTests/AgentCreationLocalAppTests.swift` |
| 真实直连创建后中继重提，仅新增一张卡片，回执一致 | 同上 `testRelayRetryCreatesOneRealCardWithoutTask` |
| 真实退出与重启 Mac 后，原操作仍返回原定义/卡片，不增成员、不派任务 | 同上 `testRestartRetryKeepsOriginalCardWithoutCreatingAgain` |
| medium 项目可见、双入口、取消保留草稿、保留添加已有 Agent | `ios/UITests/WorkspaceGroupChatFlowTests.swift` |
| App 提交、成功、概览更新、新成员可选、阅读位置与草稿 | 同上 `testAppCreatesRealMemberAndPreservesReadingPosition` |
| 手机创建 → Mac 卡片 → @新 Agent → 手机真实成果 | Computer Use 与 Mac 持久创建/任务回执 |
| 风格、状态和交互 | 独立只读 `workspace_chat_ui_acceptance` 复核 |

## 本地环境与记录

- iOS：iPhone 17 Pro / iOS 26.3，模拟器 `190AECB5-A945-41EF-B6D6-3BC08EC4E76B`。
- Mac：独立 `Norma Agent Creation Acceptance.app` 与 `/private/tmp/norma-agent-create-20261005/mac-data`，不写用户正式数据。
- 本地加密中继端口 `58415`。仅使用本地连接，不发布云服务。
- 日志、xcresult、截图和无凭据回执保存在 `target/mobile-agent-create-2026-10-05/`。
- `before.tar.gz` 为本任务开始时的保护副本。工作区含其它会话改动，未 reset、全仓格式化或提交。

### 当前测试结果

本地实现与下列验收已完成。独立 UI 结论限定为浅色、单项目 Codex；系统快捷指令配置及其 Siri/锁屏体验仍需真机验收。Claude / Cursor 的协议与类型支持已实现，本轮真实执行闭环使用 Codex。

| 验证 | 结果 | 证据（均位于 `target/mobile-agent-create-2026-10-05/`） |
| --- | --- | --- |
| 创建请求的精确身份、允许类型、禁止附带任务 | 通过 | `protocol-green.log`、`norma-all.log` |
| 持久 claim、重复请求、参数冲突、重启与恢复 | 通过；storage 全部 173 个测试、38 个 suite | `recovery-green.log`、`storage-final-green.log` |
| 准备/启动前权限与配置复查 | 3 个聚焦测试通过 | `revocation-green.log`（对应失败证据 `revocation-red.log`） |
| Swift 核心 | XCTest 107 个、2 个条件跳过、0 失败；Swift Testing 23 个通过 | `swift-final.log` |
| 聊天页面未打开时，两次独立 Intent 各创建真实卡片，未提交任务 | 通过 | `intent-green.log`、`native-isolated.log` |
| 直连创建后使用中继重提原操作 | 通过，只新增一张卡片 | `sheet-relay-green.log` 中 `testRelayRetryCreatesOneRealCardWithoutTask` |
| medium Sheet 项目可见、双入口、取消保留草稿 | 通过 | `ui-green.log`；`native-create-reading.log` 中 `testQuickCreateSheetPreservesChatDraft` |
| App 真实提交、成员刷新、新成员可选、草稿与旧消息阅读位置保持 | 通过；旧消息位置误差不超过 2pt | `app-create-final.log`（完整 `TEST SUCCEEDED`） |
| 重开群聊显示最新成果 | 通过 | `native-final-green.log` 中 `testOpeningHistoryShowsLatestResultWithoutScrolling` |
| 真实重启 Mac 后原操作重提 | 通过，原定义7/卡片7保持，未增加成员或任务 | `restart-prepare.log`、`restart-native-green.log`（完整 `TEST SUCCEEDED`） |
| Mac 检查及构建 | `cargo check -p app`、隔离 debug App、release build 通过 | `cargo-check-final.log`、`mac-build-final.log`、`release-final.log` |
| 行数、禁止 allow、存储边界、性能护栏 | 2026-10-05 最终复查全部退出码 0 | `final-guardrails.json` |

部分文件名包含 green，但记录的是混合运行：`sheet-relay-green.log`、`native-create-reading.log`、`native-final-green.log` 中的其它失败由后续修复与指定用例通过替代，不能把整个文件宣称全绿。`rust-core-final.log` 的旧库迁移失败由 `storage-final-green.log` 全量通过替代；`scope-green.log` 早期编译错误由最终 app 检查与聚焦授权测试替代。保留失败记录用于追溯。

### Computer Use 真实闭环

1. 在隔离 iPhone 的「移动端体验」群聊顶部打开 `＋`，medium Sheet 自动选择唯一项目 `acceptance-project`，选择 Codex 后提交。
2. Mac 创建 **Felix**，返回定义8、卡片8、区域 `be5eac92-6716-43e6-99d3-5004465a6fbe`；概览从7增至8位执行 Agent。创建本身未派工。
3. 手机通过 `@` 选中 Felix，明确切为「请执行」，发送读取 `proof.txt` 的只读任务。
4. Mac 实际 Felix 卡片显示 Done；手机收到「已接收 → 开始执行 → 已完成」，成果为 `Norma mobile chat ok.`。
5. 打开成果详情并切到「结果」核对正文；账本与真实 Codex `task_complete` 均保留完整文件路径 `/private/tmp/norma-workspace-chat-20261005/acceptance-project/proof.txt`。手机以 Markdown 链接标签 `proof.txt` 展示路径。

| 证据 | 文件 |
| --- | --- |
| medium 原生 Sheet | `create-sheet-cua.jpg` |
| 创建成功与成员同步反馈 | `create-success-cua.jpg` |
| Felix 的明确选中状态 | `create-member-selected-cua.jpg` |
| 派发前目标、执行模式与原文 | `create-task-before-send-cua.jpg` |
| Mac 真实 Felix 卡片及 Done | `create-mac-card-cua.jpg` |
| 手机回执与已完成成果 | `create-task-complete-cua.jpg` |
| 成果详情中的可读正文 | `create-task-result-cua.jpg` |
| 不含凭据的创建、任务与事件账本 | `creation-task-receipts.json` |
| 真实 Codex 会话完成事件 | `creation-codex-output.json` |
| 独立只读 UI 验收结论 | `ui-acceptance.md` |

身份核查：创建操作 `67F11780-8795-49F4-9E06-881FA551CF75` 和任务操作 `3C924C32-B613-449E-BB51-CBEC42934751` 共享定义8、卡片8、原区域及 run `a8cd0d5d-debb-4e1d-ab59-a0d7abe12522`；任务绑定真实 session `01a10aed-5317-7f31-af95-8e6ccdcbb894` 与 task_ref `18db94239327abe8-1:1`。

独立只读 Agent `workspace_chat_ui_acceptance` 复核7张 CUA 图和无凭据回执后给出 **PASS：限定 iOS 浅色、单项目 Codex 的新建 → @派工 → 成果回传流程**。未把多项目、Dynamic Type、VoiceOver、Siri 或锁屏体验计为通过。

### 环境观察与限制

- 本地 Mac 重启后手机服务默认关闭；本轮通过 Mac 界面重新开启。启用后的临时直连端口改变，验收 fixture 更新 endpoint，再由正式目录读取测试更新手机副本；没有更换凭据或证书。
- 创建回执的卡片状态为 `unknown`，手机显示「状态待核查」；没有伪造可用状态，实际能力由上述派工闭环证明。
- Mac 终端在真实成果之后出现 `hook exited with code 127` 告警，根因未定位。真实会话 `task_complete` 和手机任务账本均成功且内容一致；保留告警截图，不将其描述为无告警验收。
- 磁盘空间一度导致 Computer Use 无法保存截图；清理约2GiB、一天前且未加锁的 Rust 增量缓存后恢复。保护副本、源码、用户数据及关键测试证据均保留。

## 真机保留项

Siri 的系统调用体验、锁屏认证、真实扫码与蜂窝网络连接切换需要用户真机验收。原生模拟器直接调用 Intent 与本地中继集成通过，不能代替这些系统/网络场景。
