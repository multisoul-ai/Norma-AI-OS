# Norma iOS

原生 SwiftUI 客户端，最低 iOS 17；使用系统 NavigationStack、TabView、Sheet 和 VisionKit 相机扫码。无 WebView；验证码使用两端共用的 Rust SPAKE2 库。

当前可运行：黑色界面、居中指挥台导航、多电脑元数据管理、扫码引导、配对核对码、Keychain 独立手机凭据、TLS 1.3 / 证书 pin、真实授权回查与本地忘记电脑；项目目录读取真实 Mac 快照，极简 Agent 卡片通过持续 WSS 接收增量更新，支持 Codex / Claude 品牌图标和运行动效。已接入创建空卡、删除确认与操作结果查询；完整对话读取现已接入；已接入首条消息与同会话续发，以及按会话保存的本地草稿；原生中断和可靠消息队列已接入；手机审批与跨电脑指挥台已接入；计划生成、Todo确认/拒绝和真实发送关联已接入；Todo准确回合完成/失败/中断已接入，明确未发送后的重新安排已接入，项目变更预览和Mac任务起点已接入，不可变交付、历史成果和验收回执已接入；修改意见回原会话已接入；图片预览已接入；通知设置、APNs回调和点击回源已接入，真实推送配置和锁屏验收待完成。

Mac 通过 Settings → Mobile Connection 管理手机连接，欢迎页不再提供独立连接按钮：显式开启后生成六位验证码和二维码，核对手机申请、选择项目权限、撤销或调整授权。同一 App 数据库的窗口共用服务，关闭最后一个窗口停止连接；默认不监听。完整工作仍按 [Goal](../docs/exec-plans/2026-09-07-norma-ios-goal.md) 推进。

桌面使用系统接口发现局域网 / Tailscale 地址，固定端口 9443；端口占用或无私网地址会明确报错。IP 变化后用验证码或重新扫码，已配对手机会验证原凭据并更新地址，不重复申请权限。已撤销的电脑需在手机上忘记后重新配对。

## 当前功能进度

项目/卡片已支持名称、任务标题、状态和Agent筛选，使用当前授权快照即时过滤，清除或切换电脑不会保留上一台的结果。Todo默认显示仍需跟进的事项，已完成/未采纳进入「全部记录」；失败、停止、未发送和结果未知仍保留，执行完成不等同于成果验收。

功能代码与外部验收单独记录在[实时计划](../docs/exec-plans/2026-09-07-norma-ios-goal.md#功能实现与验收分开跟踪v64)。APNs服务尚未部署，完整真机/界面验收后置；TestFlight 0.1.0 (1)已进入内部测试，当前账号已获邀请，见[发布记录](../docs/exec-plans/2026-09-08-norma-ios-testflight.md)。后文为各版本的历史开发记录。

## 开发

先执行下方命令生成本机配对库，再打开 `ios/Norma.xcodeproj`，选择 Norma scheme 和 iPhone 模拟器，运行即可。真机需要选择自己的开发者 Team；本仓库不写入个人签名信息。工程由 `project.yml` 维护：

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
bash ios/scripts/build-pairing.sh
xcodegen generate --spec ios/project.yml
swift test --package-path ios
xcodebuild -project ios/Norma.xcodeproj -scheme Norma \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro' test
```

模拟器也使用 Xcode 默认本地签名；不要在运行 Keychain 测试时设置 `CODE_SIGNING_ALLOWED=NO`，否则 Security 返回 `errSecMissingEntitlement (-34018)`。编译而不运行时可禁用签名。模拟器不支持相机扫码，会显示明确不可用提示。

## 原生网络检查

以下 Rust fixture 创建临时数据库、随机证书和两个回环监听地址，只在隔离测试中自动批准一个初始只读项目；不改实际设备授权。测试覆盖错误证书无申请、真实核对码、Mac 批准后认证、改走另一地址重新扫码仍保留原权限，以及项目目录 / 两张真实空卡、未授权项目拒绝。随后仅在 fixture 内显式升级 control，验证原生创建第三张空卡、丢弃首次回执后重新读取未决记录并经另一地址回查、同 ID 参数冲突与撤销后认证 / 快照 / 回执均拒绝。再删除刚创建的卡片，换地址查询删除回执并确认另外两张卡保留。二维码不输出到日志。

```sh
swift build --package-path ios --product PairingProbe
NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" \
  cargo test -p e2e --test mobile_swift_pairing -- --ignored --nocapture

# 同一检查在实际 iOS 模拟器 XCTest host 内运行；可覆盖模拟器 UUID。
NORMA_IOS_SIMULATOR=AF8B48D2-975E-4F81-97AB-B5FB6F555618 \
NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" \
  cargo test -p e2e --test mobile_swift_pairing -- --ignored --nocapture
```

`PairingProbe`、`NormaDeviceTests` 和测试自动批准均不链接进手机 App。普通 `xcodebuild test` 无 fixture 时会明确跳过网络集成项，其余单测和 UI 测试正常运行。

## 存储与恢复

设备 token 只进入 Keychain，使用 `WhenUnlockedThisDeviceOnly` 且不参与 iCloud 同步。电脑名称、证书指纹和地址保存在 App 私有元数据文件，原子写入；记录损坏时不静默覆盖。IP/DNS 变化不能改变电脑身份；证书变化必须重新配对。

除首次公开 PAKE 交换外，每次请求先完成带 pin 的 TLS / WebSocket 握手。备用地址只在未发送应用请求时尝试；配对响应丢失后只回查授权，不自动重放 `pair`。项目目录显示最近读取时间；打开项目时建立持续 WSS 订阅，先读完整卡片，随后合并新增、变更和删除；读取失败、进入后台或切换电脑时清除可操作的旧状态，避免把旧电脑结果写入新页面。Mac 项目在手机访问时按需加载，不创建桌面窗口；`open` 表示运行实例可用。开启手机服务后关窗保留运行实例，主动退出 Norma 才停止。仅运行状态使用图标 / 边框动效，减少动态效果时静态呈现，大字号时卡片改为单列。

单项目完整基线或事件超过 500 KiB 明确失败，不静默丢卡片。v26 已接项目增量和缺口恢复：每连接独立 stream_id，变化递增序号；缺口重新订阅并读完整基线，不回放任务或审批。大项目分页仍待实现；长对话页已在v27接入无损分段传输。持久化命令回执已用于创建空卡、删除卡片与消息发送，队列和审批命令也已接入；真实双 Mac / iPhone 的跨网络恢复未验收，不能把当前阶段当成完整 G2。


## 卡片管理与回执

项目右上角「+」或底部「创建卡片」直接选择 Codex / Claude 即创建空卡，无需名称表单或再次确认；只有当前项目控制权限允许创建，创建不派发任务；成功后保持看板，结果未知时通过原操作回查。手机发送前把操作身份保存到私有 commands.json，凭据仍留在 Keychain。电脑先预留唯一执行权，再创建、等待现有画布保存完成，最后保存成功回执。

卡片右上角菜单进入删除确认；正在执行任务时明确显示「停止并删除」。Mac 比较卡片、运行实例、会话、回合和状态，拒绝旧快照；启动准备中、状态未同步或存在协调器 / 消息队列任务时暂不关闭。非受控旧卡需要先在电脑上显式重开。删除复用原有关闭 / 布局 / 保存流程，保留已有 Agent 定义、任务记录与 Provider 对话文件；已保存成果可从项目页「项目成果」继续查看和验收，已登记受控会话的完整归档聊天可从项目页「会话记录」打开。

关闭弹窗、断线或重启手机后，通过项目中的「查看操作结果」回查原操作。请求不会自动重放；结果未知时先在电脑核对，再显式清除手机记录。未决文件损坏时拒绝覆盖。同设备操作 ID 改参数会冲突，旧窗口或旧权限版本拒绝执行。

原生 UI 目前已编译和执行现有导航测试，实际创建 / 删除 Sheet 的 Computer Use 操作仍待工具恢复；Mac 已解锁；不能把网络 / headless 检查等同于真实页面验收。

## 桌面交互验收

`bash scripts/run-norma-mobile-desktop-l3.sh` 构建并打开独立的 `Norma Mobile Validation.app`，使用临时数据库和 socket。复用已有克隆预览场景以跳过全局 CLI 安装调整；先取消克隆弹窗，再点首页「连接手机」。脚本打印临时目录，便于收集日志与清理，仅操作该验证 App。

真实 App 启动和关闭此前已检查，但 Computer Use 的坐标点击曾返回 `noWindowsAvailable`；v16 最新尝试返回 Mac 已锁定、自动解锁失败，已请求手动解锁，未完成桌面配对页面的实际操作验收。headless GPUI、Vision QR 解码与 iOS XCTest 通过不能替代这部分；相机真机扫描、双 Mac / Tailscale 和真实卡片页面操作仍未验收。


## 对话读取

点击卡片进入 SwiftUI 原生对话页，显示完整正文与可展开的工具 / 附件。Codex 支持加载更早回合，Claude 按完整 JSONL 记录连续读取；持续WSS接收变化，后台 / 离开停止，错误时可重新读取。权限撤销或运行更换会清空内容，不能把另一会话正文混进当前页。空卡可在手机输入真实任务后启动；从电脑启动的会话也可继续输入。

v24 已接入原生输入框、发送、持久化草稿、手机中断与可靠消息队列。v27对超过500KiB的对话页分段传输，完整后才合并历史和更新游标；项目卡片与聊天现均使用持续事件订阅。Mac 已解锁，实际对话页 Computer Use 验收仍受工具超时影响。

两种真实 Provider 已通过 macOS Swift 和实际 iOS simulator XCTest host → 正式 Rust WSS → headless GPUI 窗口 / Runtime 的历史读取与旧运行拒绝验证。命令与证据见 [连接文档](../docs/design-docs/2026-09-07-norma-ios-connection.md#原生对话读取v19)，不把它等同于真实 iPhone / 双机 / Tailscale 测试。


## 消息发送

聊天底部使用原生多行输入框、系统键盘避让和 44pt 发送按钮。发送前保存原文与准确卡片 / 操作 ID，成功后清空输入；进入后台或断线只停止手机查询，电脑继续已经接受的任务。未知结果保留在「查看发送结果」，不会自动重发。

Codex 以真实 turn/start 回执确认，Claude 以正文核验通过的 PromptSubmit 确认，均返回准确运行 / 会话 / 回合。两种 Agent 已在原生 macOS Swift 和实际 iOS simulator host 中通过首条任务、同会话续发、真实正文、重复请求不增加消息及旧快照拒绝检查。Mac 原生终端保留。 Claude 发送多行 / Tab 前等待终端启用安全粘贴，避免换行提前提交；其原生编辑器会将每个 Tab 转成 4 个空格，手机会提示，其他空白和换行原样保留。Codex 保留原始 Tab。

忙碌时可把下一步想法加入队列，Agent 空闲后依次处理；离开页面后的未发送草稿可恢复。发送正文最多 32 KiB，手机未决文件总计最多 256 KiB；iOS 文件采用完整数据保护。桌面未确认的输入会阻止手机直接发送，避免混合两端草稿。详细范围和重跑命令见 [v20 连接文档](../docs/design-docs/2026-09-07-norma-ios-connection.md#原生消息发送v20)。

## 草稿与恢复（v21）

v115键盘交互：轻点输入框外收起键盘，聊天与表单保留原生下滑收起；点击输入框或切换字段继续编辑。聊天、终端与验证码数字键盘均有44pt收起按钮；收起不清空草稿、不发送内容。地址键盘的“下一项”可进入验证码，工具卡点击与长按文字选择保持可用。

输入立即更新在内存中，并异步写入 App 私有草稿文件；只有写入成功才显示「草稿已保存到此手机」。快速编辑合并为下一份最新快照，离开页面不会取消保存。进入后台会申请系统允许的短暂执行时间收口写入；若保存失败，新内容仍留在内存，旧文件保留，界面提示重试。系统在保存完成前强制结束 App 的场景仍需真机验收，不能把内存更新当作已落盘。

草稿以电脑 / 项目 / 窗口 / 卡片 / 运行 / 会话完整身份绑定。回到同一范围自动恢复；运行变化不会自动迁移或发送旧草稿。对话中可查看先前会话草稿；电脑页右上角「本地草稿」提供全部草稿，即使项目已关闭、卡片已删除或已忘记电脑，仍可复制自己的原文。删除草稿需要明确确认。

发送记录包含本地草稿版本，该版本不进入网络命令。只有准确的成功回执会清除其对应版本；后来重新输入的同文草稿、未知结果和其他电脑的草稿均保留。清除落盘失败时，重复查询也不能假装完成清理。

每条草稿最多 64 KiB，总计最多 128 份 / 4 MiB；超限不截断、不自动淘汰，显示未保存并保留内存文本。iOS 配置完整文件保护，macOS 测试采用 0600。模拟器没有返回文件保护属性，该检查明确跳过并保留真实 iPhone 测试；此结果不能证明实际锁屏保护。Mac 已解锁；Computer Use 的编辑 / 返回 / 切换电脑验收仍待工具恢复。

v21 验证：Swift 13 项通过，Xcode 16 项通过 / 2 项明确跳过（无网络 fixture 与真机文件保护）；两种真实 Provider 的原生 iOS 草稿链路另行通过。模拟器 Release App 位于 `ios/DerivedData/Build/Products/Release-iphonesimulator/Norma.app`；iPhone SDK 编译也已通过，未签名、未进行物理设备运行。详细日志与未完成验收见 [连接证据](../docs/design-docs/2026-09-07-norma-ios-connection-evidence.json)。


## 停止任务（v22）

聊天页右上角可停止当前 Agent 回合；请求先保存在 iPhone，再提交一次。正在停止与已确认停止分开显示，结果也会如实显示任务自然完成或失败。断线后从原操作记录查询，未知结果不自动重试。停止保留原生终端和会话，可继续发送新消息；电脑上已有的未提交草稿仍需先处理。

请求绑定电脑、项目、窗口、卡片、运行、会话和回合，思考 / 工具 / 审批之间的正常阶段变化不会使同回合停止失效。历史停止操作不能重新作用于新回合。两种 Provider 的原生网络检查、构建和未完成页面验收见 [v22 连接文档](../docs/design-docs/2026-09-07-norma-ios-connection.md#原生手机中断v22) 与 [证据记录](../docs/design-docs/2026-09-07-norma-ios-connection-evidence.json)。

## 消息队列（v24）

Agent 忙碌时，发送按钮变为「加入队列」。电脑确认完整原文已持久化后，可继续写下一条；排队不会打断当前任务。聊天页队列入口查看该卡片记录，电脑页「消息队列」保留全部本机原文，即使项目关闭或已忘记电脑也可复制找回。

等待中可以撤回；开始发送后只能查询实际结果。已入队、正在发送、Agent 已收到、取消、结果未知分别显示，未知不会自动重发。重新打开手机只按原操作 ID 查结果；缓存标记为上次状态，明确清除本机记录不会停止电脑上的操作。Mac 窗口、运行或授权失效后不向新会话派发旧消息。

两种真实 Provider 均已通过 macOS 原生 Swift 和 iOS simulator host 的排队、撤回、顺序及去重检查。原生导航测试通过；Computer Use仍因Mac锁定未完成，真机、跨Mac和Tailscale验收继续按Goal推进。重跑命令见 [v24连接文档](../docs/design-docs/2026-09-07-norma-ios-connection.md#后台队列与-swiftui-接入v24)。

## 指挥台与审批（v25）

居中指挥台读取全部已保存电脑的获授权项目；左滑拒绝、右滑允许一次；可暂时跳过，并保留明确按钮。代码选择与按钮区不触发滑动决定，请求内容或授权变化取消当前手势。显示完整工具输入和来源，不会把当前项目的电脑选择器当作决定目标。只读没有控制动作；离线或读取失败的电脑显示需要重新核验。

「查看任务详情」先向对应电脑核对项目、窗口、Agent类型和运行/会话/回合，再进入原聊天页面；查看不发送任务或决定，关闭后保留原决策位置。原生卡片和只读项目也能查看；打开前身份已变化会明确报错。通知定位同样不要求切换卡片模式，控制权限仍独立校验。

决定先保存到手机，再提交一次。右上角「决定记录」保留当时的完整请求，即使项目关闭或电脑已被忘记仍可查看；重连只查原操作，不自动重放。提交成功表示决定已交给本地传输层，实际执行结果应查看任务；请求关闭和结果未知均不显示为工具执行成功。

Codex 已有外部审批 Hook 仍由电脑扩展处理；手机显示外部等待，不提供批准按钮，也不接收 Hook 配置路径或命令。需要验证客户端审批协议时，只在已获用户授权的隔离测试进程中暂不加载外部 Hook：

```sh
bash scripts/check-norma-mobile-approvals.sh macos
bash scripts/check-norma-mobile-approvals.sh ios
```

两种真实 Provider 的拒绝 / 批准、磁盘效果、丢失响应后回查与重复去重均已通过 macOS Swift 和 iOS simulator 验证。SwiftUI 导航测试通过；Computer Use 与真实多电脑 / iPhone / Tailscale、无障碍验收仍按 [Goal](../docs/exec-plans/2026-09-07-norma-ios-goal.md) 推进。


## 项目实时同步（v26）

项目卡片页使用 `MobileClient.projectUpdates`，保留原生 SwiftUI 网格、44pt 系统操作和减少动态效果支持。断线重连使用原生进度反馈；恢复期间清空卡片、来源时间和控制权限，并关闭旧窗口/旧权限的操作 Sheet。持久化未决记录不删除。

后台每次事件重新核验授权与真实 GPUI 状态。前台仅递增原子失效计数，后台最多250ms合并变化；闲时每2秒重新读取，只有这次读取成功才发心跳。心跳不递增序号，也不证明任何任务或审批已执行。订阅占既有八个连接之一；离开页/后台取消。

传输失败、事件缺口或授权版本改变时，用1/2/4秒退避最多三次只读重连；撤销、错误身份和格式错误终止。慢消费者的缓冲满会显式失败，重新连接才能恢复。没有持久化事件日志，离线期间合并掉的瞬态不做完整回放，执行结果仍查询原操作回执。

```sh
swift build --package-path ios --product PairingProbe
NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" \
  cargo test -p e2e --lib e2e_mobile_project_events_native_transport -- --ignored --nocapture --test-threads=1
NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" \
  cargo test -p e2e --lib e2e_mobile_project_events_native_transport -- --ignored --nocapture --test-threads=1
```

上述测试使用真实 WSS、Keychain、正式 GPUI/Runtime 与 SQLite：持续超过5秒、空卡增删、降为只读后断流/恢复、取消后无更新；不启动模型。macOS 12.76秒、iOS simulator 35.51秒通过。项目序号缺口另由Swift测试，换窗口与越权由真实WSS测试验证。v26时聊天与审批聚合尚未切换到事件订阅；聊天已在v28接入。Computer Use 仍受锁屏限制；这些证据不代表物理 iPhone 或双 Mac 验收。


## 长对话完整传输（v27）

保留Provider现有历史分页和完整文本/工具内容。小于500KiB的响应维持原格式，较大的页通过同一WSS连接分段发送；每片最多64KiB原始UTF-8，包含最坏JSON转义仍低于500KiB。一页最多32MiB；Claude仍有8MiB单行/16MiB扫描限额，Codex私有RPC仍有8MiB单响应限制，超限明确报错，不能称为任意大小历史支持。

Swift在后台收齐一次冻结读取后校验请求，才交给聊天页。缺片、offset错误、传输ID混用、取消或断开均不显示半条内容、不推进Provider游标；下次只读请求从原游标重新读取，不重发任务。Mac在传输起止核验准确窗口/run/session，并在每片前重验授权；最后一片前运行被替换时拒绝完成。没有新增缓存服务、数据库表或第三方依赖。

```sh
swift build --package-path ios --product PairingProbe
NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" \
  cargo test -p e2e --test mobile_conversation_transfer native_large_conversation_transport -- --ignored --nocapture
NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" \
  cargo test -p e2e --test mobile_conversation_transfer native_large_conversation_transport -- --ignored --nocapture
```

大响应检查使用明确的正文/工具fixture和真实Rust TLS/WSS、原生Keychain/URLSession：macOS0.75秒、iOS simulator12.15秒通过；另有运行在最后一片前变化的真实WSS检查。两种真实Provider经正式GPUI/Runtime的macOS聊天回归28.20秒通过。大响应fixture不等同于真实Provider产生超长消息或实际页面操作验收；v28已接入聊天事件订阅；真机长内容/无障碍仍待完成。


## 持续聊天与断流恢复（v28）

SwiftUI聊天页使用两条独立的WSS订阅：项目流决定当前卡片状态与控制权限，对话流读取完整历史变化。Codex在最近20回合页变化时发送该完整页；Claude沿JSONL游标继续读完整记录。两者都不是逐token差分；空闲时每2秒复验真实来源和授权，发无正文心跳。

原生unfolding流按消费者节奏读取，不创建可能丢弃Claude记录的应用缓冲。断线或序号缺口先清空历史，最多按1/2/4秒只读重连，每次建立新基线；撤销、错会话终止。后台、离开页与取消会停止订阅；旧的手动翻页不能覆盖恢复后的历史。超过500KiB的大事件复用v27分片，完整校验后才提交内容。创建、发送和决定不会随订阅重连重放。

```sh
swift build --package-path ios --product PairingProbe
NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" \
  cargo test -p e2e --test mobile_conversation_events -- --include-ignored --nocapture
NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" \
  cargo test -p e2e --test mobile_conversation_events native_conversation_watch_recovers_and_revokes -- --ignored --nocapture
```

实际Codex/Claude连续聊天事件在macOS23.13秒、iOS simulator35.29秒通过。原生断流/撤销fixture在macOS与网络检查合计5.80秒、iOS13.91秒通过；iOS大事件完整接收12.48秒通过。Swift19项、DeviceTests21项/2跳过、导航3项通过；Mac/模拟器Release与未签名iPhone SDK编译通过。全仓仍有5项Chrome授权/语音失败，无HEAD基线。

Computer Use仍返回Mac锁定，本轮聊天交互尚无真实操作证据；XCTest导航不能替代聊天页面验收。计划/Todo、授权成果/diff、APNs和真实多Mac/iPhone/Tailscale、无障碍仍按完整Goal继续推进。


## 下一步与计划建议（v29）

指挥台和项目页可进入「下一步」。选择电脑/项目，填写方向，由电脑上的Norma模型生成总结及带执行Agent的Todo建议；每次方向与计划单独保存，历史可分页查看。使用原生SwiftUI列表、输入框和DisclosureGroup，保持黑色紧凑风格。

生成请求先保存在手机，重新连接只查询原结果。生成成功只代表建议已保存，不会派任务；Todo确认/拒绝、执行关联和实际完成状态仍待实现。Mac窗口关闭会取消本地生成，目标卡片被删除后迟到建议不发布。

原生macOS与iOS simulator链路分别1.21秒/8.69秒通过，模型响应为隔离HTTP fixture；Swift21项、DeviceTests23项/2跳过、导航4项通过，Mac/模拟器Release和未签名iPhone SDK编译通过。全仓仍有5项Chrome授权/语音失败，无HEAD基线。Computer Use仍因Mac锁定未完成，不能视为计划页面或真机验收。

重跑方式及输入上限见[连接文档](../docs/design-docs/2026-09-07-norma-ios-connection.md#计划生成与原生下一步页面v29)，后续按[完整Goal](../docs/exec-plans/2026-09-07-norma-ios-goal.md)继续。


## Todo 确认与执行（v30）

计划详情可「安排给Agent」或「不采纳」。原生确认Sheet显示完整指令、电脑/项目与当前执行Agent；会话更换会提示，空卡确认后才启动，忙碌卡片确认后排队。Mac仅执行保存的计划原文，多台手机共享同一决定；创建卡片仍不派任务。

先把决定保存在手机再提交一次，断线只查询。本机尚未发送的任务可撤回；已送达、拒绝、撤回后手机查询记录清理，电脑上的计划/决定保留。结果未知可在电脑核实后显式清除查询记录，不自动重放。页面使用项目事件核验控制权限，每2秒读取共享决定；离开/后台取消读取，连接或权限失效不可用旧确认继续操作。

「Agent已收到」是实际发送结果，不代表完成。只有准确窗口/run/session仍匹配时才能进入所在对话；否则提示到电脑核实。真实任务终态、明确未发送后的重新安排、成果与APNs仍待后续接入。

两种真实Provider的Todo链路macOS23.44秒、iOS simulator42.93秒通过（计划为明确fixture，执行Provider真实）；Swift24项、DeviceTests26项/2跳过、既有导航4项通过。真实Todo页面Computer Use因Mac锁定未完成；不把网络或导航检查等同于真机交互验收。重跑与完整限制见[连接文档](../docs/design-docs/2026-09-07-norma-ios-connection.md#todo-决定与执行关联v30)。

v30构建收尾：App check **5.41秒**、Mac Release **2m17s**、模拟器Release与未签名iPhone SDK build-for-testing通过；行数/no-allow/存储/性能/指南/diff护栏通过，canvas/terminal/runtime依赖树无GPUI。未安装到物理iPhone。日志为`/tmp/norma-ios-todos-{app-check,mac-release,ios-release,device-build,dependencies}-v30.log`。


## Todo 回合结果（v31）

计划详情显示本轮执行完成（待验收）、失败或停止；与“Agent已收到”分别处理。结果来自电脑保存的准确Provider回合，不随卡片后续任务变化。没有终态证据时不推断完成；成果/diff验收仍待实现。

真实Codex/Claude在iOS simulator均通过终态检查（51.89秒）；macOS首次Claude检查失败，补日志后复验通过，原因仍未确定。Swift25项、DeviceTests27项/2跳过、导航4项通过。Computer Use仍因Mac锁定未完成，真实iPhone/双Mac/Tailscale及无障碍继续按[Goal](../docs/exec-plans/2026-09-07-norma-ios-goal.md)推进。测试与构建证据见[v31连接文档](../docs/design-docs/2026-09-07-norma-ios-connection.md#todo-准确回合终态v31)。

v31构建收尾：App check **5.15秒**、Mac Release **2m13s**、模拟器Release与未签名iPhone SDK build-for-testing通过；行数/no-allow/存储/性能/指南/diff护栏通过，canvas/terminal/runtime依赖树无GPUI。未安装到物理iPhone。日志为`/tmp/norma-ios-turns-{app-check,mac-release,ios-release,device-build,dependencies}-v31.log`。


## 重新安排（v32）

上次明确未发送时，Todo显示「重新安排」。再次核对完整指令和当前Agent后生成新请求，旧决定和回执保留；忙碌时仍排队。另一手机已重新安排时，旧确认失效。正在发送、已送达或结果未知都不能用此入口重排；失败后需先核实已有副作用并调整方向。

Swift26项、DeviceTests28项/2跳过、导航4项通过；Storage85项与移动网络14项通过，覆盖两手机竞争和撤回后仅派发一次。真实iOS Codex/Claude验证已送达任务不能重排（39.19秒）。成功重新安排的实际Provider、SwiftUI操作、真实iPhone/双Mac/Tailscale仍待验收；Computer Use仍受Mac锁屏限制。详细证据见[v32连接文档](../docs/design-docs/2026-09-07-norma-ios-connection.md#明确未发送后的重新安排v32)。

v32构建收尾：App check **5.42秒**、Mac Release **2m15s**、模拟器Release与未签名iPhone SDK build-for-testing通过；行数/no-allow/存储/性能/指南/diff护栏通过。本轮未改依赖，域层无GPUI的v31检查保留。未安装到物理iPhone。日志为`/tmp/norma-ios-retry-{app-check,mac-release,ios-release,device-build}-v32.log`。


## 项目变更（v33）

原生SwiftUI项目页/聊天页可查看当前未提交变更，切换HEAD原文、暂存区、工作副本及diff。正文使用原生TextKit，可完整滚动、选择、复制，支持系统动态字号；手机预览不编辑文件或提交代码。600000字节文件已通过正式Mac路由/WSS与原生iOS客户端读取验证。

当前只是项目预览，尚无交付持久化或验收回执；不包含已经提交的内容，不推断某Agent独占改动。二进制图片预览、任务成果验收和APNs仍待完成。单版本1MiB、合计8MiB、128文件，过滤器/子模块等不支持内容会明确报错，不截断后冒充完整结果。

新增原生检查可用`NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" cargo test -p e2e --lib e2e_mobile_review_native_transport -- --ignored`重跑；测试仅操作隔离Git项目/测试配对，不调用模型。Mac锁屏仍阻止Computer Use，模拟器/HostingView检查不代表真实iPhone页面、VoiceOver或双Mac/Tailscale已验收。详细进度见[Goal](../docs/exec-plans/2026-09-07-norma-ios-goal.md)。

v33最终验证：移动网络15通过/3忽略，Swift27通过，DeviceTests30通过/2跳过，原生导航4通过。最新真实iOS预览检查10.18秒通过；App check14.06秒、Mac Release2m15s、模拟器Release和未签名iPhone SDK build-for-testing通过。文件行数、no-allow、存储/性能/指南及diff护栏通过，canvas/terminal/runtime/workspace依赖无GPUI。完整Goal未完成，未安装到物理iPhone；Computer Use与成果验收等缺口按上述记录保留。

## 任务起点（v34）

原生SwiftUI消息流程已连接Mac任务起点存储：直发、Todo和队列在实际发送前保存项目起点字节，并在Provider确认新回合后绑定原始操作。已有用户修改与任务期间commit的内容可在域层准确对比；无可用起点时记录具体失败，不冒充空成果。底层发送预算为40s，手机仍只回查原请求。

真实Codex三条发送路径21.01秒通过；两种Provider原生macOS30.57秒/iOS simulator40.61秒通过，起点按实际发送回执核验。Swift27项、DeviceTests30项通过/2跳过；全仓仍有5项Chrome授权/语音失败，无HEAD基线。详情和重跑命令见[v34连接文档](../docs/design-docs/2026-09-07-norma-ios-connection.md#任务起点与准确发送回执v34)。

本段没有新增成果验收页面；“项目变更”仍为当前未提交预览。不可变交付、验收、图片、通知及真机验收继续按[Goal](../docs/exec-plans/2026-09-07-norma-ios-goal.md)实施。

v34构建收尾：Mac Release2m10s、App check3.80秒、模拟器Release和未签名iPhone SDK build-for-testing通过；移动网络/持久化15通过/3忽略。最终Codex直发/Todo/队列起点检查21.01秒通过。依赖与机械护栏通过，未提交代码、未安装到物理iPhone；完整Goal继续active。

## 任务成果（v35）

手机端继续使用原生SwiftUI。聊天页“查看成果与变更”可进入按回合保存的任务成果，支持历史分页、完整文件正文和“任务起点→本次交付”对比；当前工作树变化不会覆盖旧成果。“整理成果”期间可写草稿和排队，待保存后继续执行。保存失败显示原因，查看不等于验收。

Swift29项、DeviceTests32通过/2跳过，模拟器Release和未签名iPhone SDK构建通过；完整证据见[v35连接文档](../docs/design-docs/2026-09-07-norma-ios-connection.md#自动冻结交付与原生成果页v35)。真实Claude本轮未取得完成证据，Computer Use仍受锁屏限制；验收回执/反馈、图片、APNs、物理iPhone/双Mac/Tailscale验收仍按Goal推进。

v35构建收尾：Mac Release2m21s、模拟器Release和未签名iPhone SDK build-for-testing通过；Runtime144通过/3忽略、Storage88通过、Swift29通过、DeviceTests32通过/2跳过。行数/no-allow/存储/性能/指南/diff与域层无GPUI护栏通过。未提交代码，未安装到物理iPhone；完整Goal保持active。

最后网络回归：15通过/3忽略；App check3.99秒通过，日志见v35连接证据。


## 成果验收（v36）

任务成果详情可明确确认验收，显示保存的验收时间。沿用原生命令Sheet和本地未决记录；断线后回查同一操作，不重放。Mac只允许控制权限下验收准确回合/指纹的可用completed成果；验收记录和成功回执同事务保存。当前历史列表未显示验收标签，修改反馈尚未接入。

Storage89、Swift30、DeviceTests33通过/2跳过、原生导航4通过；600kB冻结正文+验收的原生macOS/iOS网络检查通过，使用明确终态fixture。Mac已解锁，但Computer Use仍有Simulator超时与桌面窗口状态错误，尚无真实页面点击验收；v35真实Claude未完成、全仓5项E2E失败、物理iPhone/双Mac/Tailscale和APNs缺口保留。完整证据见[连接记录](../docs/design-docs/2026-09-07-norma-ios-connection.md#原生-swiftui-成果验收v36)，Goal保持active。

v36构建收尾：Mac Release4m21s、模拟器Debug/Release、未签名iPhone SDK build-for-testing和App check通过；行数/no-allow/存储/性能/指南/diff护栏通过。未提交代码，真实页面与物理设备验收仍未完成。


## 提出修改（v37）

成果详情中的“提出修改”把意见发回原Agent会话，创建新的任务回合。原始意见、旧成果指纹和新回合回执保持关联；关闭/重开的会话不会收到旧反馈。意见草稿复用本地存储并与普通聊天区分；先保存再发送，断线后只查结果。

两种真实Provider的macOS与iOS原生反馈检查通过，覆盖自动冻结、原文、草稿恢复、新turn和关闭后拒绝；Storage90、Swift31通过。Computer Use/物理设备未验收，图片/APNs和全仓5项E2E失败继续保留，详细构建结果见[连接证据](../docs/design-docs/2026-09-07-norma-ios-connection-evidence.json)。Goal保持active，未提交代码。

v37构建收尾：Mac Release、模拟器Debug/Release、未签名iPhone SDK构建与App check通过；DeviceTests34通过/2跳过、原生导航4通过，网络回归8通过。Mac已解锁，用户打开Norma后Computer Use仍报Simulator timeoutReached；Mac首页可读但连接按钮未产生页面变化，后续点击报user-changed，实际交互未验收。截图见artifacts/norma-ios-v37/mac-computer-use.png。

## 图片成果（v38）

文件详情支持原生栅格图片预览，新增/删除默认打开存在的版本；可切换任务起点、交付与暂存区，支持系统捏合放大及VoiceOver放大/还原。只解码已授权的成果字节，不读取额外路径。单图最多1MiB和16777216像素，缩略图最长边2048；动画显示首帧，损坏、超限及不支持的格式提示在电脑查看。

原生宿主图片/正文3项通过，DeviceTests36通过/2跳过、导航4通过，WSS图片字节与权限回归通过。Computer Use仍超时，实际页面与物理设备验收未完成；构建记录见[连接证据](../docs/design-docs/2026-09-07-norma-ios-connection-evidence.json)。

v38构建收尾：模拟器Release、未签名iPhone SDK构建与常规护栏通过。完整E2E199通过/11失败/29忽略（语音Apply、两项Chrome授权与8项语音锁poisoned），无HEAD基线；未完成Computer Use验收，未提交代码。


## 历史列表验收状态（v39）

成果历史显示真实验收标记，打开详情后会更新对应列表项，保留分页位置；错误身份/指纹和不可用成果不能显示为已验收。Storage90、Swift31、DeviceTests36通过/2跳过、导航4通过，原生macOS/iOS WSS核验列表与验收回执一致；Mac/模拟器Release和未签名iPhone SDK构建通过。完整E2E与物理设备验收仍待完成。


## 不用相机的配对（v40）

在Mac手机连接页点击「复制连接码」，在iOS添加电脑页选择「输入连接码」并粘贴；读取后展示电脑身份，连接与桌面批准仍沿用扫码流程。格式norma-pair-v1:加URL-safe无填充Base64，内容是原配对JSON；不使用公网注册中继，不把六位核对码当作认证密钥。两种入口共享120秒与单次认领约束，禁止日志记录码。

扫码页支持系统照片选择器，只读取选中图片；后台Core Image解码，20MiB编码与8192²源像素上限、4096px缩略图，拒绝多码和无二维码。用户仍需确认电脑身份。

桌面复制/到期/取消测试1项、Settings E2E2项、Swift31项、iOS聚焦原生2项及导航1项通过；Mac/模拟器Release和未签名iPhone SDK构建通过。真实配对尚未完成：Computer Use曾识别电脑，但二维码过期；内容区点击持续noWindowsAvailable，当时等待协助，已由 v41 验证码方案替代。此次更新保留隔离桌面数据库，未声明真机或Tailscale验收。


## 六位验证码配对（v41）

用户已改为数字验证码，v40 的长连接码 UI/解码入口已移除。Mac 显示、复制或刷新 6 位 ASCII 数字；手机首次填写 LAN/Tailscale 地址（默认 9443）和验证码，保留前导零。输入页使用黑色原生 Form、数字键盘、oneTimeCode、进度与取消反馈，验证成功后记住地址。二维码和图片扫码继续可用。

验证码不作为设备 token，也不在网络发送明文。`mobile_pairing` 通过 RustCrypto SPAKE2 0.4.0 交换一次性公共消息，使用派生密钥和随机 nonce 的 AES-256-GCM 加密原有 QR 身份/凭据；Swift 只在解密并验证原协议后进入严格证书 pin 的正式配对。有效期 120 秒，所有接口/连接合计最多 3 次交换，取消、刷新、到期或 QR 认领均关闭旧入口；Mac 仍需批准具体项目权限。两端共用小型 Rust 库，生成的 XCFramework 不包含 GPUI/runtime，支持 macOS 与 iOS 模拟器 arm64/x86_64、iPhone arm64。

验证：Rust Norma/配对库 117 通过、1 忽略；Swift 32 通过；原生 UI/输入校验 2 通过；真实 macOS 与 iOS WSS 均完成错误码拒绝、第三次成功/第四次锁定、正确码解密、Mac 只读批准、项目读取及撤销。QR 网络/持久化回归 4 通过，桌面复制生命周期 1 通过。小窗口弹窗先复现末行超出窗口（y=786.5px），复用组件内置滚动并限制高度后 Settings E2E 3 通过。

完整 E2E 为 207 通过、3 失败、29 忽略：MiniMax 手动语音选择断言仍失败，其余两项为该锁 poisoned；未声明完整 E2E 或完整 Goal 通过。Computer Use 已实际开启新版桌面服务并查看验证码，也打开了 iOS 配对向导；模拟器内容点击仍间歇性 noWindowsAvailable。当前尚未完成用户实际桌面注册，已请求停留在验证码输入页；没有物理 iPhone、双 Mac 或真实 Tailscale 验收。Goal 保持 active。

v41构建收尾：Mac Release（2m28s）、模拟器双架构Release、未签名iPhone SDK build-for-testing、App check与配对库Clippy通过。最终版本已更新隔离桌面App与iPhone 17 Pro模拟器，保留已有数据。

## 当前实际连接与通知准备（v42）

用户已确认当前桌面与iOS成功配对；保留当前运行实例。新构建修复了普通VPN地址误推荐和验证码超时提示，尚未覆盖已连接实例。双独立本地WSS服务的同名项目/相同ID隔离通过，不能代替物理双Mac验收。

[APNs私有relay](../services/push/README.md)已完成本地HTTP/2和签名验证。用户有开发者账号，服务尚未部署；Mac/iOS通知注册、撤销、真实事件投递与点击回源仍待接入，当前App没有锁屏推送能力。配置步骤及真实验证缺口见该说明。

## 通知订阅接口（v43）

NormaCore新增pushSubscription/updatePushSubscription，经原有证书pin和设备凭据管理当前手机的提醒订阅。更新包含所见版本，过期版本拒绝；关闭显式发送null。注册成功只说明Mac已保存设置，不代表APNs已送达。Mac撤销授权同时清除push token。

原生macOS与独立iOS simulator host已使用隔离token验证注册/关闭/重开/撤销；Storage91、Swift33和网络8项通过。iOS系统通知授权、APNs回调、用户开关与点击定位尚未接入，当前App仍无锁屏通知能力；下一段接入Mac真实事件和手机系统通知。


## 原生任务提醒（v44）

电脑页新增黑色原生「任务提醒」：用户先允许系统通知，再对每台Mac单独开启/关闭；系统登记和Mac订阅分别展示。新token仅续登记用户之前启用且Mac仍保留的订阅；关闭失败不冒充成功。v114起，忘记电脑立即移除本地连接和Keychain凭据，不依赖联网；停止本机提醒续登记，并尝试在后台关闭电脑端提醒。离线时需在Mac上关闭提醒或撤销授权，手机不保留凭据进行重试。列表右侧菜单与红色左滑操作均可进入原生确认框。

通知只携带目标身份。点击后等待本机电脑列表加载，经证书pin回源检查项目/窗口/运行/会话/回合和具体审批；旧目标显示过期，不自动执行决定。真实来源匹配后可进入现有聊天或定位指挥台。部署、签名、Debug/Release环境和已知投递边界见[推送配置](../services/push/README.md#mac与ios配置v44)。

本轮Swift35项通过；5项原生导航通过；新增系统回调与通知目标的模拟器检查通过。Computer Use已实际点击独立验收模拟器的提醒入口和空态；已连接电脑的提醒开关和通知中心点击尚待完整验收。用户现有v41桌面/模拟器保持运行，未覆盖；没有真实APNs或物理iPhone证据。

签名账号已按桌面Packager.toml沿用公司Team `2BF8G9AN4L`。原通配描述文件缺少推送能力；Xcode自动更新已成功生成专用描述文件与已签名Debug构建，签名校验及实际aps-environment=development检查通过。没有进行App Store/TestFlight上传。当前登记的物理iPhone 11不可用，锁屏验收仍待连接设备和APNs服务配置。


Mac通知服务现可从「手机连接 → 任务提醒 → 配置服务」填写HTTPS入口、relay访问凭据与环境；Debug选Sandbox，TestFlight/App Store选production。保存配置后仍需在iPhone内允许系统通知并开启对应电脑的提醒。保存不是APNs送达验证；Apple签名密钥只保留在服务器。细节见 `../services/push/README.md`（v45）。


## 已配对通知与点击验收（v46）

原生通知代理使用显式completion-handler并在MainActor完成，修复通知冷启动的UIKit主线程断言崩溃。独立模拟器已取得真实系统token并完成按电脑开启/关闭；本地注入通知可冷启动到准确机器/项目，旧窗口拒绝。此组不包含Apple服务投递或物理锁屏，Computer Use仍不能读取Simulator，不能与XCTest证据混淆。

可重复运行：先以显式 `NORMA_IOS_SIMULATOR` 和 `NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh"` 执行 `cargo test -p e2e --test mobile_notification_native -- --ignored --nocapture`。它只允许空的独立模拟器，走真实配对和订阅，最后撤销并删除自己的记录。

交互验收增加 `NORMA_NOTIFICATION_UI=1`，fixture完成配对后输出目录，最多保留15分钟。运行 `NotificationFlowUITests/testPairedComputerSystemRegistrationAndSubscription` 核验系统token与开关，再执行 `python3 scripts/check-norma-ios-notification-clicks.py <fixture目录> --simulator <同一独立设备UUID>` 注入五条本地通知并测试SpringBoard点击。最后以 `NORMA_TEST_MODE=notification-ui` 运行 `NotificationFlowUITests/testForgetAcceptanceComputer`，并在fixture目录创建 `ui-finished` 空文件正常停止后台测试服务。不要对用户已配对设备运行这些测试；不要把 `simctl push` 当作真实APNs。


## 大字号与聊天回归（v47）

项目/聊天的电脑信息在辅助字号下纵向展示；辅助字号输入框最多显示两行并原生滚动，默认保持五行。原文完整保存、权限禁用提示保留。使用上一节独立notification-ui fixture，以相同显式设备UUID和 `NORMA_TEST_MODE=notification-ui` 运行 `-only-testing:NormaUITests/AccessibilityFlowTests`；两项测试分别用默认Large和最大AX5字号，完成系统裁切/触控区/名称审计、键盘输入与只读拒绝。之后运行正式忘记测试并写ui-finished结束fixture。

该组是原生XCTest和系统无障碍审计，未包含真实Provider聊天正文、VoiceOver手势或Computer Use；不要把局部检查写成完整无障碍认证。详细结果见连接证据 `accessibility_acceptance_v47`。


## 桌面回归验收（v48）

桌面音色列表布局、不同App的音色状态隔离和GPUI帧检查已修复；完整Rust E2E为230通过/0失败/34忽略，受影响语音单测38项通过。本轮未修改或重装iOS，沿用v47原生验证范围。Computer Use因Mac锁屏尚未收口；正式Keychain跨进程恢复、APNs部署与真机锁屏仍未验收。Mac Release与全workspace验证通过（1737通过/0失败/62忽略），见[实时计划](../docs/exec-plans/2026-09-07-norma-ios-goal.md#v48--桌面回归失败的根因修复)。


## 凭据恢复与规划检查（v49）

Mac正式推送Keychain跨进程检查通过（安装隔离、替换、停用、损坏拒绝及清理），不再使用内存替身作为此项证据。Ollama规划增加原生JSON约束；真实小模型的结构/目标检查与内容质量分别记录，当前样例内容仍不合格。未切换用户模型，未修改/重装iOS。真实云模型、APNs、物理设备与完整界面验收仍待完成，详见[实时计划](../docs/exec-plans/2026-09-07-norma-ios-goal.md#v49--正式推送凭据恢复与真实规划检查)。


v49原生重排验收：两种真实Provider在macOS与iOS模拟器均通过；检查精确重排关联、只发送一次、拒绝项不执行和旧回执保留。初始未发送状态来自持久化fixture，仍需完整SwiftUI/真机操作验收。用户删除/tmp造成的测试环境问题已恢复，最终全workspace1738通过/0失败/64忽略、Mac Release通过，Swift35项通过；新日志在target/norma-ios-evidence/v49。


## 通知跨弹窗与连续点击（v50）

通知显示在当前弹窗上方，关闭后保留原导航、订阅和未发送输入。前台连续点击提醒按完整任务身份重新核验，不能仅依赖event_id。原生回归覆盖设置、前台目标变化、嵌套输入、冷启动和旧窗口；运行方式沿用v46的隔离fixture及通知点击脚本。已取得部分Computer Use设置/模拟器锁屏点击证据，完整界面、物理iPhone和真实APNs仍待完成。

若自己的15分钟fixture已过期，先导出该独立模拟器的`Norma/machines.json`元数据，再显式以`NORMA_TEST_MODE=notification-cleanup`、`NORMA_TEST_QR=<导出文件绝对路径>`运行`NotificationFlowTests/testRecoverExpiredNotificationFixture`。它要求仅一台名为「通知验收 Mac」的电脑且全部元数据一致；只清理该测试配对及其通知意图，不适用于用户设备。正常结束仍应先运行正式UI忘记，再写ui-finished。

v50最终验证：Swift35通过、DeviceTests42通过/4显式跳过、原生导航5通过、通知5条路径通过；模拟器Release与未签名iPhone SDK构建通过。新日志在`target/norma-ios-evidence/v50/`；iPhone SDK编译不代表物理设备安装或APNs投递。

## 项目成果与通信性能（v52）

打开项目后可从「项目成果」查看全部卡片的已保存回合，删除卡片不会移除这些成果或已有验收。仍只读取当前已授权、已打开项目；历史列表分页20项，按项目/序号索引读取，正文在选中成果后传输。此入口不等同于完整归档聊天。

实时项目与聊天WSS改为变化通知唤醒，50ms合并活跃突发、2s空闲鉴权心跳，关闭与撤销也会唤醒；小帧启用TCP_NODELAY。iOS逐条验证原始项目增量，再向UI保留最新完整快照，慢页面不会因缓冲区满误报事件缺口。

独立性能检查（避免与构建/全仓测试同时运行）：

```sh
cargo test -p e2e --lib e2e_mobile_snapshot::e2e_mobile_project_change_latency \
  -- --exact --ignored --nocapture --test-threads=1
```

本机20次p50 61.60ms、p95 75.04ms；全仓并行负载下另观察到2.43s，不能宣称高负载达标。物理iPhone/LAN/Tailscale及页面验收按用户安排集中进行。完整证据和剩余任务见Goal的v52记录。

## 会话归档准备（v53）

桌面已在受控Agent启动时保存准确会话身份，Claude的文件引用使用现有后台writer持久化；删除卡片或显式重开不清理已保存索引。索引不等于任务完成，也不意味着Provider文件永久存在。Codex关闭后的官方只读分页已通过真实协议验证，不需要恢复会话或发起新任务。

本轮未修改iOS界面。手机完整归档聊天列表与正文入口仍待接入；现有「项目成果」继续只表示冻结交付。旧会话补录、后台提交前强杀恢复，以及统一界面/真机验收仍未完成。详见Goal的v53记录。

## TestFlight发布准备（v54）

`0.1.0 (1)` 已生成发布签名IPA，路径为`../target/norma-ios-testflight/export/Norma.ipa`，尚未上传。App Store Connect缺少对应App记录，Apple公开API不支持创建，网页操作需解锁Mac。创建记录后继续上传、等待处理并完成真实加密问卷；详见[发布记录](../docs/exec-plans/2026-09-08-norma-ios-testflight.md)。


## 会话记录与归档聊天（v55）

项目→「会话记录」可查看已登记的Codex/Claude完整对话，删除卡片不删除入口；全文与工具详情复用原生聊天组件。正文按需分页并以原Provider文件为准，不恢复Agent任务。需电脑在线且手机服务开启；未登记旧会话或原文件已被清理时不会改读其他会话。

复验命令（后者只使用独立模拟器）：

```sh
NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" cargo test -p e2e --lib e2e_mobile_archive_native_transport -- --ignored --nocapture
NORMA_IOS_SIMULATOR=5265591D-704C-4515-868D-C2D64F94A7FA NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" cargo test -p e2e --lib e2e_mobile_archive_native_transport -- --ignored --nocapture
```

原生macOS/iOS都已验证621kB正文完整分片；真实Codex关闭进程和删除卡片后的只读历史一致。全workspace1747、Swift37、iOS44通过；iOS另4项专项测试明确跳过。页面Computer Use仍待统一验收，不能据此宣称物理双机或APNs已通过。

用户已取消TestFlight发布；v54安装包保留，未上传，也不会在解锁后自动继续。

## 通信分阶段采样（v56）

上面的独立性能命令现自动执行20次完整隔离场景，输出一条 `MOBILE_LATENCY_REPORT` JSON，含全部原始毫秒样本和nearest-rank p50/p95/max。TCP/TLS/WebSocket连接握手、已建连Ping/Pong、首份项目快照、持续订阅的项目关闭更新分别计时，未删除冷启动或慢样本；每个更新仍须低于150ms。

本轮p50/p95分别为：握手23.57/26.39ms、WebSocket往返0.048/0.069ms、首份快照6.40/11.13ms、项目更新60.38/65.66ms。握手是三个协议阶段合计，快照和更新包含授权、GPUI调度、序列化及网络处理，不能相减推导纯服务端耗时。

证据保存于 `../target/norma-ios-evidence/v56/summary.json`。范围仅为回环Rust客户端、headless GPUI和真实SQLite/WSS；SwiftUI呈现、审批提交、聊天首包、纯TLS阶段、空闲CPU/流量及物理LAN/Tailscale分位数仍待测，不能用本轮结果替代。

## 真实原生通信采样（v57）

现有消息与审批验证可选择增加测量，普通回归不增加请求。下面的iOS命令仅操作独立验收模拟器；将probe换为`ios/.build/debug/PairingProbe`、审批脚本参数换为`macos`可运行macOS原生端。

```sh
NORMA_MOBILE_MEASURE=1 PATH="$PWD/target/debug:$PATH" NORMA_IOS_SIMULATOR=5265591D-704C-4515-868D-C2D64F94A7FA NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" cargo test -p e2e --lib e2e_mobile_codex_messages_native -- --ignored --nocapture --test-threads=1
NORMA_MOBILE_MEASURE=1 PATH="$PWD/target/debug:$PATH" NORMA_IOS_SIMULATOR=5265591D-704C-4515-868D-C2D64F94A7FA NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" cargo test -p e2e --lib e2e_mobile_claude_messages_native -- --ignored --nocapture --test-threads=1
NORMA_MOBILE_MEASURE=1 NORMA_IOS_SIMULATOR=5265591D-704C-4515-868D-C2D64F94A7FA bash scripts/check-norma-mobile-approvals.sh ios
```

消息命令前需按原验证流程构建`multisoul-agent-hook`。审批脚本沿用已授权的隔离Hook方式，不改正式配置。每种Provider保留20次审批/聊天读取、原两次实际决定的接收与持久化回执、一次含模型生成的回复等待、6s聊天订阅客户端CPU；不增加模型任务。日志中的`NATIVE_LATENCY`/`NATIVE_IDLE`不包含正文或凭据。

iOS模拟器：Codex/Claude审批列表p95为29.23/31.68ms，已完成短聊天页p95为78.12/36.23ms；8个macOS/iOS真实原生场景通过。整页读取含新建连接与解码，不能作为首分片、纯TLS或物理跨网数据。2次决定和1次生成不足以宣称稳定分位数；客户端CPU也不代表完整产品或服务端。原始结果见`../target/norma-ios-evidence/v57/summary.json`，完整限制见Goal v57，实时指挥台已在下述v58接入。

## 指挥台实时审批（v58）

指挥台每台电脑保留一条 `watch_approvals` WSS，汇总全部授权项目；已删除原来的3秒轮询。电脑离线不会阻塞其他电脑，离开页面/进入后台取消订阅。断线、授权变化、序号缺口清除旧工具内容，最多按1/2/4秒退避重新读取完整基线，不重放决定。外部Hook仍仅显示等待提示，手机不能代替扩展审批。

服务端复用已有项目变更信号，活跃突发按50ms合并，2秒鉴权心跳不重传工具正文；全电脑快照上限500KiB，所有项目共享4秒读取预算。单项目错误显示无法读取，不以旧审批冒充当前状态。增加空卡或修改名称不会断开有效审批流；读取期间窗口路由变更则丢弃旧输入。

重跑真实Codex/Claude流程使用前述 `check-norma-mobile-approvals.sh`；现在会保持同一审批订阅，验证准确工具输入、拒绝无写入、批准内容相同、重复回查和已解决请求移除。独立模拟器仍须显式指定 `5265591D-704C-4515-868D-C2D64F94A7FA`。

```sh
cargo test -p e2e --test mobile_approval_events
# 仅在无其他构建/测试负载时测量；本机fixture，不代表SwiftUI或物理LAN/Tailscale。
cargo test -p e2e --test mobile_approval_events approval_feed_local_latency -- --ignored --nocapture --test-threads=1
NORMA_IOS_SIMULATOR=5265591D-704C-4515-868D-C2D64F94A7FA NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" cargo test -p e2e --lib e2e_mobile_project_events_native_transport -- --ignored --nocapture --test-threads=1
```

真实原生审批macOS/iOS共4项、双订阅降权/取消2项及iOS外部Hook等待/停止1项通过；Swift38通过。完整构建回归与日志以[实时计划](../docs/exec-plans/2026-09-07-norma-ios-goal.md)为准。用户已配对的旧桌面实例未升级，统一验收时才使用新构建；本轮不以模拟器测试代替Computer Use、物理设备或APNs验收。


## 无关项目变化不增加心跳流量（v59）

Mac端项目/聊天订阅现与指挥台一致：内容变化立即发送，空闲心跳只按2秒周期发送，其他项目/设备的全局唤醒不会产生额外网络帧。等待使用距上次发送的剩余预算，鉴权、序号和断线恢复不变，手机无需协议升级。

30次无关唤醒的真实WSS回归中，两条流合计JSON从21750降至725字节，仍有保活；这是应用消息流量，非TLS/TCP总流量或CPU测量。另新增长聊天首分片与整页的独立20次采样，命令、原始数据范围见[连接文档](../docs/design-docs/2026-09-07-norma-ios-connection.md#无关变更下的心跳与首分片采样v59)。实际原生恢复、连续对话和构建结果以[实时计划](../docs/exec-plans/2026-09-07-norma-ios-goal.md)为准，真机与完整页面继续统一验收。

## 原生分片与系统开销测量（v60）

现有大正文probe设置`NORMA_MOBILE_MEASURE=1`，可运行20次首完整分片/全文校验，以及持续聊天订阅的约6s空闲采样。系统脚本只测隔离服务PID的listener/peer线程CPU和nettop TCP字节，原始样本/CSV保留在`target/norma-ios-evidence/v60/`；正式App没有新遥测或计时设置。

独立iOS模拟器首分片p95=25.14ms，完整43帧校验126.43ms；6s空闲收到3个后续心跳、TCP发送1158字节，服务网络线程单核占比约0.52%。这些是回环测量，CPU不含完整GPUI/Provider，TCP计数不含全部网络头；不代表物理iPhone、Tailscale或耗电表现。重跑命令与边界见[连接文档](../docs/design-docs/2026-09-07-norma-ios-connection.md#原生首分片服务线程cpu与tcp字节v60)，完整回归与首次超时记录见[实时计划](../docs/exec-plans/2026-09-07-norma-ios-goal.md)。

## 空闲聊天不重复合并历史（v61）

聊天订阅将有效心跳作为独立`heartbeat`返回，页面持续接收但不重复合并缓存全文；新消息、缺口恢复与撤销照常处理。原生macOS/iOS各6s确认3次心跳、0次内容更新，仍完整保留长正文和工具详情。此结果不代表实测SwiftUI帧数或真机耗电。

两平台真实Codex/Claude消息、断线恢复/撤销与大页共8项场景通过；Swift38、iOS45通过/4跳过、E2E233通过/39忽略，模拟器Release和未签名iPhone SDK构建通过。源码与验收边界见[连接文档](../docs/design-docs/2026-09-07-norma-ios-connection.md#聊天心跳与正文事件分离v61)，原始数据留在`target/norma-ios-evidence/v61/`。TestFlight保持取消，完整界面与物理网络继续统一验收。

## Todo真实撤回与重新安排验收（v62）

原生Todo检查已取消预写“未发送失败”的前置，改为真实Agent忙碌时由手机入队、撤回，再停止原生成并明确重新安排。Codex/Claude在macOS与独立iOS各通过：原回执与取消状态保留，旧操作重放不影响新任务，只有两条被确认的Todo实际执行，拒绝项不进入对话。

E2E233、Swift38、iOS45通过/4跳过；本轮生产逻辑未改。计划建议仍是固定测试内容，原生传输验收不等于SwiftUI点击或真实云规划质量。命令与完整范围见[连接文档](../docs/design-docs/2026-09-07-norma-ios-connection.md#真实todo入队撤回与重新安排v62)，日志保存在`target/norma-ios-evidence/v62/`；TestFlight保持取消。

## 计划实时进度（v66）

计划与执行页使用持续WSS同步共享决定、撤回和准确回合终态；空闲心跳不重发计划内容。一次操作后的回查不覆盖有序流。此功能需配合包含`watch_plan`的本轮Mac构建，尚未重启用户当前桌面。Swift41、iOS48通过/4专项跳过、完整E2E233通过/39忽略，原生macOS/iOS Codex Todo通过；早期中断后校验失败尚无确定根因，日志保留。


## 卡片发送图片（v116）

聊天输入框的“+”打开系统照片选择器，每条最多4张，支持纯图片发送、草稿恢复、移除和缩放预览。图片由后台规范方向/缩小/重新编码，单张最多2048px、2MiB，不携带原EXIF位置。手机经已配对的TLS连接分片上传，Mac按项目鉴权和摘要校验后保存，再在原Codex/Claude卡片中发送文件引用；无需另开会话。

需同时更新Mac端以支持`upload_image`/`chat_image`。图片与原文沿用准确回执/队列，失败保留草稿，不自动重发；历史图片须连接到有授权的原电脑读取。每台Mac存储最多256MiB和2048个目录/文件项，不自动删除尚可能被Agent引用的图片。Agent配置的模型必须支持视觉：真实Codex读图通过，当前Claude的`ark-code-latest`仅支持文本，工具已返回图片但模型拒绝，不能把发送成功当作视觉回答成功。

重跑核心及正式网络验证：`swift test --package-path ios`、`cargo test -p e2e --test mobile_image_network`。原生Agent专项见`crates/e2e/src/e2e_mobile_images.rs`；显式设置`NORMA_SWIFT_PROBE`和`NORMA_TEST_IMAGE`，只使用隔离AppDB与测试图片。UI复测使用独立模拟器和`NORMA_DECISION_UI=1 NORMA_NOTIFICATION_UI=1` fixture，启用`ui-decision-deck`、`ui-chat-interactions`、`ui-card-management`后放入`photo-fixture.jpg`；依次运行`NotificationFlowTests/testSeedChatPhoto`与`ChatPhotoFlowTests`，最后正式忘记测试电脑并结束fixture。未上传TestFlight，证据见`../target/norma-ios-evidence/v116/`。
