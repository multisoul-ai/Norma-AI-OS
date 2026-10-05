# iOS Agent 实时终端详情

日期：2026-10-05。实现位于当前工作区，尚未提交或发布。

## 实现范围

- 群聊执行卡及成果卡打开原生大尺寸 Sheet，默认进入「终端」，可切换「结果」并追问原任务。运行结果由同操作的后续事件更新。
- 终端为只读 UIKit 双轴滚动视图，CoreText 按 Mac 的固定网格绘制字符、颜色、样式及光标。手机阅读不调整 Mac 的 PTY 尺寸、滚动位置或输入权。
- 上滑暂停跟随并冻结已读页；提供历史分页、回到实时、10–24pt 字号与复制反馈。Sheet 内切换标签保留终端阅读位置；返回群聊保留原滚动位置与草稿。
- 独立 `WorkspaceTerminalClient` 连接：LAN 每 250ms、中继每秒查询变化。无变化响应仅含版本；关闭详情、离开终端标签及进入后台取消刷新。短断线保留最后画面并重连。
- 原事件的 Mac、工作空间、设备、操作、任务引用、区域、Agent 定义、节点、运行和会话共同限定目标。迟到会话只能补齐原运行；会话变化停止读取，不能自动切换到当前同名卡片。

## 协议及桌面边界

新增只读能力 `workspace_terminal_v1` 及 `WorkspaceTerminalRequest/Update`。LAN 与端到端加密中继复用业务授权入口。读取已保留的 Runtime，不加载项目、不启动新的 PTY；捕获后再次核对授权。

复用已经解析的 ANSI 网格及桌面颜色解析逻辑。每页最多 120 行、最多 512 列、总历史最多 100000 行。后台计算样式、JSON 与版本，不在 GPUI 前台进行编码或 hash。

大画面按行分片，正文不超过 200KiB，维持现有 256KiB 帧上限。iOS 校验总行数、连续 offset、完整元数据和快照身份，收到完整画面后一次应用。内存分片缓存最多 8 份、30 秒过期，并绑定设备和完整目标身份；续传先校验当前原会话。

历史版本第一部分锚定分页边界前最多 8 行及网格尺寸、主屏/备用屏状态，第二部分标识整个画面。进度原位更新和追加输出不影响未移动的旧边界；清空或移动边界时拒绝旧分页。重复内容不能证明全历史的世代一致性。没有永久终端录像或任务输出归档。

## Apple 设计原则落地

依据 `apple-design` 与 `norma-ios-style-guide.md`：原生 Sheet、NavigationStack 与分段选择器提供系统转场和明确层级；触按样式、复制成功及连接状态提供即时反馈；横屏压缩标题以保留至少 100pt 网格阅读区。按钮保留 44pt 点击区域，字号控制有明确边界，VoiceOver 正文去掉中文宽字 spacer，网格可读取文字且不会获得键盘输入焦点。既有 Reduce Motion 和原生材质策略继续适用。

## 验收映射

| 验收项 | 自动化证据 |
| --- | --- |
| 完整身份隔离、同名卡片、只读授权、撤权 | `crates/app/tests/unit/workspace_terminal_tests.rs`、`workspace_terminal_cache_tests.rs` |
| 缺失绑定、同运行迟到会话、拒绝替代会话 | `ios/Tests/NormaCoreTests/WorkspaceExecutionDetailTests.swift` |
| 颜色、中文宽字、光标、原位进度、只读尺寸和滚动 | `workspace_terminal_capture_tests.rs`、`WorkspaceTerminalGridTests.swift` |
| 主屏/备用屏、历史分页、追加输出、边界失效 | `workspace_terminal_capture_tests.rs` |
| 分片大小、完整组装、缺片及混片拒绝 | `crates/norma/tests/workspace_terminal.rs`、`WorkspaceTerminalTests.swift` |
| LAN 连接关闭重连、假运行拒绝、群聊连接独立 | `WorkspaceTerminalLocalTests.testLiveTerminalReconnectAndOriginalSessionIsolation` |
| 真实加密中继 fallback、关闭重连、每秒预算 | `WorkspaceTerminalLocalTests.testEncryptedRelayTerminalFallbackAndReconnect` |
| 会话失效说明不因回实时消失 | `WorkspaceTerminalModelTests.testReturnToLivePreservesStoppedConnectionNotice`，已观察 red 后修复并通过 |
| 标签、复制、暂停、旋转、原卡位置和草稿 | `WorkspaceTerminalFlowTests.testTerminalSheetReadingAndChatState` |
| 长消息与旋转后恢复原卡视口位置 | `WorkspaceTerminalGridTests.testChatReadingAnchorSurvivesMessageRelayout`，先观察原卡偏移失败，再修复并通过 |
| 长消息下直接打开详情 | `WorkspaceTerminalFlowTests.testOpenExecutionSheetWithLongMessages` |
| 旧成果会话已失效但原正文可读 | `WorkspaceTerminalFlowTests.testOldResultRetainsOriginalSessionAndReadableBody` |
| 真实长历史只读分页 | `WorkspaceTerminalLocalTests.testLiveHistoryPagingPreservesOriginalSession`，使用明确隔离的 PTY 输出 fixture |

已通过 Rust norma 全 crate（133 项），app 捕获/授权/缓存聚焦测试（8 项），terminal（56 项、2 ignored），runtime（212 项、3 ignored），`cargo check -p app` 及 release 构建。Swift 核心通过 107 项 XCTest（含 2 skipped）及 23 项 Swift Testing；原生 Grid 4 项、Model 1 项与真实加密中继连接测试通过。未运行禁用的 `crates/e2e`。

主原生 UI 流程先以固定 25 字符草稿通过（35.695 秒），再显式构造 883 字符草稿通过（37.356 秒）：历史分页、复制、暂停、两标签、横屏网格至少 100pt、返回原卡位置误差不超过 3pt、草稿全文保留。883 = 25 + 107 × 8 + 2；测试只再追加 8 字符，最终比较完整 891 字符。真实历史接口探针通过（0.249 秒）；旧成果会话隔离与长消息直接开详情也通过。

独立模拟器最后复验通过：全部 3 项原生 UI（主流程 36.407 秒）、4 项 Grid、1 项 Model，以及真实历史分页（0.348 秒）。已保存无凭据的执行结果到 [`native-test-results.txt`](ios-terminal-acceptance-2026-10-05/native-test-results.txt)。

已通过文件行数、禁止 allow、指南行数、性能及 storage 边界护栏。最终新增的 Swift 验收用例继续单独运行，不重复未受影响的 Rust 测试。

## 真实 App 证据及限制

验收只操作 `/private/tmp/norma-workspace-chat-20261005/Norma Workspace Acceptance.app` 与隔离模拟器；没有修改生产 Norma 数据或权限。Computer Use 已操作默认终端、结果正文、复制反馈、字号变化、横竖屏和返回群聊。最终截图保存于 `docs/exec-plans/ios-terminal-acceptance-2026-10-05/`。

- `chat-before.png`、`chat-returned.png`：旋转及关闭详情后，原执行卡位置与草稿保留。
- `terminal-history.png`：主屏历史第一页中文日志、暂停跟随与回实时入口。
- `terminal-recovered.png`：当前原会话的中文网格及绿色进度。
- `terminal-landscape.png`：横屏紧凑标题、网格与工具栏。
- `result-running-wait.png`：执行期间的结果等待状态与追问入口。
- `terminal-reconnecting.png`：短断线的最后画面与重连提示。

直接请求 Agent 生成长回复的准备用例仍受 CLI hooks 审查与会话占用阻塞；该用例明确 skipped，没有宣称真实 Agent 已生成这段新回复。展开旧 Codex 工具输出进入备用屏，不等于 ANSI scrollback，首次历史探针据此失败。随后向已验证属于隔离 Bella 的 PTY 输出受控 ANSI fixture，供主屏历史及实时刷新验收；不向 Agent 输入命令、不新增授权。

复验曾发现键盘交互收起时群聊文本布局持续占用 CPU；采用即时收起，并修正验收测试反复删除/恢复草稿造成文本累积的问题。可控 883 字符回归已通过；没有保留未能改善问题的 Text fixedSize 改动。返回群聊以原卡视口锚点恢复，避免长消息和旋转重排后沿用旧 offset 造成跳动。

Computer Use 已读取主屏中文日志 001 和 180，横向阅读固定列宽，回到实时并复制输出；受控 ANSI 进度在同一行从红色 25% 变为绿色 75%。当前执行卡为 cursor 24；cursor 19 的旧成果使用已失效的原会话，不能替代绑定。

初次日志和截图曾写入 `target/`，随后被其他构建流程清理；本轮补验收证据改存 `docs/exec-plans/ios-terminal-acceptance-2026-10-05/`。原模拟器也被其他流程反复重新安装 App，导致无签名构建的 Keychain 访问失败；正式签名的配对恢复测试通过，并改用独立 `Norma Live Terminal Acceptance` 模拟器复验。凭据始终保存在隔离 fixture 内，没有写入文档或日志。

短断线通过暂停已核对身份的隔离 Mac 服务模拟，设置 45 秒自动恢复保障：Computer Use 观察到最后网格保留及重连说明；服务恢复后说明消失并自动回到实时。没有关闭生产服务或改变网络/授权设置。加密中继关闭重连另有正式 native 客户端证据。

Computer Use 将 App 切到后台后，独立终端 TCP 连接从 1 条降为 0；返回前台后恢复为 1 条，原详情仍可阅读。无正文的本地连接记录保存到 [`connection-lifecycle.txt`](ios-terminal-acceptance-2026-10-05/connection-lifecycle.txt)。实体 iPhone 未验收。
