# Norma iOS 连接与配对

状态：G2 的 Mac 配对设置、窗口 owner 与原生 SwiftUI 客户端已实现；真实项目 / 卡片快照、卡片管理、完整对话和原生消息发送已接入；原生草稿、中断和可靠消息队列已接入；手机审批与跨电脑原生指挥台已接入；项目卡片增量流与缺口恢复已接入；长对话分段传输已接入；Norma建议生成、原生计划页、Todo决定/执行关联和准确回合终态已接入；明确未发送后的重新安排、项目变更预览、任务起点、自动冻结交付与SwiftUI成果历史已接入，准确回合验收回执与反馈回原会话已接入；用户已确认当前桌面与 iOS 配对成功；物理 iPhone 扫码、通知和多机验收尚未完成，不能视为完整手机闭环。

## 已实现边界

- `norma::mobile` 提供 TLS 身份、一次性配对与 WSS 握手；`app::MobileAccess` / `MobileServer` 串行处理本地授权和网络请求，SQLite 仍由 storage 维护。不向网络暴露本地 Unix IPC 或 Provider 凭据。
- 电脑 ID 为 `mac-` + 证书 DER 的 SHA-256。App 数据库保存稳定、非秘密的 Keychain account；证书和私钥保存在本机 Keychain。并发初始化使用 SQLite insert-if-absent 与 Keychain SecItemAdd，不覆盖已存在身份；Keychain 拒绝或损坏明确失败。更换证书后手机必须重新核对和配对，不能凭同名或同 IP 自动信任。
- TLS 1.3，禁用 early data；用 rcgen 生成证书，复用 ring/rustls 后端。仅监听明确的局域网、Tailscale 或测试回环地址，拒绝通配地址和公网地址；最多四个监听地址。WSS 只接受 `/mobile`，拒绝 query 与浏览器 Origin。
- 连接数跨接口合计最多八个，握手整体最多三秒，未认证连接整体最多十秒，普通认证连接每次读取（含空闲）最多三十秒；项目 watch 持续发送经重新读取确认的事件/心跳。未认证 Ping 不续期。输入帧和消息最多 256 KiB，响应缓冲最多 512 KiB。
- 同一 App 数据库的窗口共享 `MobileController`；全局注册表只保留 weak 引用，最后窗口释放时显式 shutdown，即使后台仍持有 server Arc 也停止连接。后台 listener 最多二十毫秒内观察停止；启动中的取消由 generation 和共享原子标志阻止迟到结果。未开启桌面功能前，不自动监听端口。
- 桌面通过原生 `getifaddrs` 发现最多四个已启用私网地址，优先 Tailscale / IPv4，固定端口 9443 让保存的地址跨重启复用；不使用 shell、通配监听或无 scope 的 IPv6 link-local。接口变化仍需重新开启连接并扫码更新地址，尚无 Bonjour 自动发现。

## 桌面配对入口（v15）

首页「连接手机」与 Settings → 手机连接复用同一组件，空电脑无需先创建项目。采用 gpui-component 原生按钮和弹窗，以紧凑层级、明确 disabled / selected / focus 反馈落实 Apple Design。Core Image 生成带四模块白边的 QR，Apple Vision 独立解码验证通过；不新增二维码依赖。

前台只发送命令和读取内存状态；项目列表、Keychain、网络启动、配对批准与 SQLite 均在后台执行。仅二维码有效的两分钟内每秒查询 pending，无空闲 UI 轮询。项目权限默认不选，支持只读 / 控制、保存 / 取消、撤销。取消权限编辑丢弃草稿并刷新版本；重新打开编辑时不会继续使用已冲突的 revision。

真实隔离 `.app` 已启动，首页入口在截图中可见，关闭按钮使独立进程退出；未触碰正在运行的生产 Norma。Computer Use 坐标点击在 Mac 和模拟器均持续报 `noWindowsAvailable`，无法据此验收实际配对操作。测试 App 已关闭并清理，未创建其 Keychain 身份。

## 配对顺序

1. Mac 显式打开配对，生成包含协议版本、电脑 ID、名称、证书指纹、WSS 地址、offer ID 与随机 256 bit secret 的二维码。生成新二维码使旧请求失效；二维码不能记录到日志或遥测。
2. iPhone 扫码后，先对 TLS 证书做精确指纹核验，再发送配对信息。名称、IP、DNS 和六位核对码都不能替代 TLS pin。
3. iPhone 先生成并在 Keychain 保存针对本次配对的独立 device ID / 256 bit token，再提交 `pair`。凭据验证后二维码只能认领一次；Mac 此时只显示申请，没有项目访问权限。
4. 两端显示六位核对码。该值绑定 offer、扫码 secret、手机 ID 和手机 token，只用于视觉核对，不是六位密码认证。
5. Mac 明确选择真实稳定项目 ID 及只读/控制范围，本地调用 `approve`；不存在的项目拒绝。只有 SQLite 注册成功才报告批准成功，写失败可以在有效期内重试。网络协议不存在 `approve` / `revoke` / 权限修改操作。
6. 二维码签发到本地批准统一使用主机单调时钟，边界 120 秒过期。取消、刷新、关闭配对都使迟到批准无效。配对申请可跨短连接保留；丢失结果时手机用预存凭据再次认证，不重新注册或重放写指令。
7. 每次认证重新读取 SQLite，并核验目标电脑 ID。权限更新带预期 revision；降权或撤销关闭该手机已有连接，另一手机不受影响。返回 grant 是快照，后续任务派发还必须核验最新 revision 和窗口/运行/回合身份。

## 当前网络协议

请求：`version: 1`、有界 `request_id`、`machine_id` 和 `action`。

- `action.type = pair`：`request` 中包含 offer_id、secret、device_id、device_name、device_token。成功返回 pending 和核对码，等待本地授权。
- `action.type = authenticate`：device_id、token。成功只返回该手机的项目授权快照及版本，不返回凭据。
- `action.type = snapshot`：device_id、token 与可选 workspace_id。省略 workspace_id 返回获授权项目目录、窗口状态和卡片数；指定项目才返回该项目完整卡片元数据。成功状态为 snapshot，附带当前 grant 和 projects。
- `action.type = watch_project`：device_id、token、workspace_id。持续返回 project_event；每连接新 stream_id、sequence=0 的完整 snapshot，然后 delta（upserted、removed、order、display_name）或同序号 heartbeat。
- `action.type = command`：device_id、token、request（operation_id、workspace_id、route_id、grant_revision、command）。command 支持 create_card（profile=Codex/Claude、可选 title）、delete_card（expected 为完整 CardSnapshot、confirm_busy 为显式确认）、send_message / queue_message（expected 为完整快照、text 为真实消息）、interrupt_turn（expected 为准确回合）和 decide_approval（expected 为准确请求目标、decision 为单次选择）；创建不包含任务提示。
- `action.type = conversation`：device_id、token、request（workspace_id、route_id、node_id、run_id、session_id、可空 cursor 字符串）。只读权限即可请求；返回 conversation，包含原 request 和 page（items、cursor、has_more、pending_tail、direction）。items 提供 id、role、完整 text 与可空 details。cursor 不透明，不能视为授权。
- `action.type = watch_conversation`：与conversation相同身份，初始cursor必须为空；持续返回conversation_event（stream_id、sequence、原request、可空page）。sequence0必须含基线，正文变化递增，心跳同序号且page为空。大事件复用conversation_fragment，完整重组类型为ConversationEvent。
- `action.type = receipt`：device_id、token、workspace_id、operation_id。返回绑定设备 / 操作 / 项目 / 窗口的 accepted、succeeded、rejected 或 outcome_unknown 回执；succeeded 含准确 node_id；send_message 还包含 Provider 确认的 run_id、session_id、turn_id。
- 错误只返回稳定错误码，不回显原始消息、token 或解析内容。未知字段、动作或协议版本拒绝。

外层 request_id 只用于关联一次网络响应；内层 operation_id 在 SQLite 中按手机唯一，独立于连接。持久化去重 / 可查询回执已用于创建和删除卡片；网络层已提供历史读取与消息发送，队列与审批派发已在 v24 / v25 接入；项目卡片订阅与缺口全量恢复已在 v26 接入，聊天及审批仍独立查询。

## 原生 SwiftUI 客户端（v14）

`ios/` 提供原生 iPhone 工程和可独立测试的 `NormaCore`。SwiftUI 使用黑色系统页面、紧凑语义字号和居中指挥台；导航、Sheet 和 VisionKit 扫码均使用平台组件。网络与 Keychain 不运行在 MainActor。

`CredentialVault` 在系统 Keychain 保存每个电脑对应的独立手机身份与随机 token，使用 WhenUnlockedThisDeviceOnly / 非同步属性。`MachineStore` 只原子保存非秘密元数据；损坏文件明确失败，不用空列表覆盖。发送前保留凭据；v15 改为收到已核验的主机响应后才更新名称与地址。丢失响应仍可用页面内存中的 offer 和预存凭据回查，认证成功后补存元数据。

重复扫码优先通过新地址认证原手机凭据，返回原权限和 revision，不消耗新 QR、不产生第二条手机记录。仅明确未授权才进入配对流程；连接错误、证书不匹配不会自动改成注册或换凭据。真实 Swift / Rust 检查使用两个监听端口，并在首次 QR 已被使用后切换到另一端口重扫，macOS 与 iOS simulator 均通过。该证据不等同于两台物理 Mac 或真实 Tailscale。

`PinnedConnection` 使用临时 URLSession、TLS 1.3，先匹配准确叶证书 DER SHA-256，再以该证书为唯一 anchor 验证有效期/签名/server usage。变化的 IP/DNS 不参与机器身份。拒绝重定向，无 ATS 例外；每个端点的握手限五秒，最多四个端点，仅在尚未发送应用请求时可换路径，发送后绝不自动重放。每次交换有五秒期限和 512 KiB 响应限制。

手机独立计算视觉核对码，并校验响应 request ID、授权 device ID / revision / 项目权限。当前可读取项目 / 卡片快照，连接在每次交换后关闭；目录显示最近核验时间，可见项目每两秒刷新卡片。进入后台、离开页面时取消查询；切换电脑重建导航，迟到响应通过取消和 generation 拦截。错误时清空卡片并提供重试；未授权响应清除当前电脑的目录授权。聊天、决策和持久化命令回执仍未实现。

[开发与重跑说明](../../ios/README.md)。macOS 原生检查与真实 iOS 模拟器 host 都通过 Rust 回环 WSS 的错误证书、配对、批准、认证和撤销链路；自动 Mac 批准只存在于隔离 Rust fixture，不链接进产品。模拟器需使用 Xcode 默认本地签名，禁用签名会导致原生 Keychain -34018；不通过更改 Keychain 属性处理该错误。

## 项目与卡片快照（v16）

网络线程先重新认证并读取 SQLite 的授权项目，再向窗口 owner 发送有界请求。前台复用现有 WorkspaceRouteIndex / WeakEntity，核验 App 数据库路径和 workspace ID 后按需读取 Runtime 元数据；不加入 frame loop、PTY drain 或磁盘扫描。关闭 / 不可用项目不返回 cards、route_id 或 card_count；空卡片明确 idle，不启动 Provider。

等待前台时释放授权锁，允许桌面撤销；返回前再次认证并核对 grant revision，途中撤销 / 降权丢弃旧数据。前台队列容量 8、响应期限 2 秒；返回体最多 500 KiB，留出外层协议空间。超限返回 snapshot_too_large，没有分页，绝不把部分列表标成完整快照。

Swift 同时校验手机身份、精确项目集合、窗口身份、卡片 ID 唯一性及数量；读取其他项目、关闭窗口仍带旧卡片等响应不能进入页面。真实 WSS / GPUI 检查覆盖改名、删卡、项目关闭、另一安装数据库和未授权项目；撤销竞争在真实网络中检查。macOS Swift 与 iOS simulator XCTest host 均通过真实 Rust WSS 目录 / 卡片读取，route fixture 与正式 GPUI 窗口检查分别标明。

Apple Design 使用系统导航、紧凑语义字体、纯黑背景和 Dynamic Type。动效按 emil-design-eng 只表达正在执行的状态：图标轻微变换，边框线性转动；停止后静态，Reduce Motion 禁止循环动效，不增加库。品牌资源沿用已批准 HTML 的 LobeHub MIT SVG，许可保留在 ios/BrandIcons-LICENSE.txt。

| Before | After | Why |
|---|---|---|
| 仅有未同步项目占位 | 获授权项目目录与真实极简卡片 | 先呈现真实内容和来源 |
| 品牌与运行状态只在 HTML 原型中 | 原生矢量品牌、状态文字与运行动效 | 保留视觉识别；文字不依赖动效理解 |
| 未形成真实卡片页面操作证据 | 验收仍待解锁 Mac 后继续 Computer Use | XCTest / 域层证据不替代实际操作 |

## 空卡创建与回执（v17）

手机提交前原子保存 PendingMobileCommand；包含非秘密操作身份及规范化内容，不含 token。重新打开只查询，不自动提交。Swift 校验回执的 device / operation / workspace / route 身份和状态结构，accepted 不得携带成功节点，succeeded 必须有节点，结果未知不得伪装成失败后可安全重试。

Mac 在后台重新验证机器、设备、控制权限与 grant revision，SQLite 先预留唯一 `(device_id, operation_id)`。回查只需该项目现有读取权限；撤销后连历史回执也不能读取。同 ID 同参数返回已有记录，改参数冲突，不会再次投递。

前台接收有界请求后校验准确的现有 workspace route / App 数据库 / 运行窗口；一次性原子执行权与撤销 / shutdown 竞争。尚未执行的请求失效；已开始请求只执行一次，不因手机断线而撤回既有副作用。创建复用正常布局、Runtime 元数据和现有画布调度器，等待保存状态 Clean 才提交成功回执。没有新建并发快照 writer、Provider、终端提示或 daemon。

最多八项未完成处理，30 秒上限。结果写入失败、窗口消失和无法证明保存的情况报告 outcome_unknown，重开服务的未完成旧 owner 记录同样结果未知；不会自动重放。账本不自动删除历史 ID，避免旧手机重试绕过去重；长期保留策略未来需要结合明确重试期限与 tombstone 制定，当前没有静默清理。

SwiftUI 使用系统 Form / Picker / Sheet、紧凑字体和原生触控反馈落实 Apple Design。只读 / 未打开项目不能创建；未决操作入口保留查询和核实后显式清除。离开页面或进入后台取消手机查询，Mac 已开始操作继续遵循窗口生命周期。

## 下一接入

- 桌面 Settings 已接入，仍需真实 Computer Use 配对、权限选择与启停验收。
- SwiftUI 与 Rust 的原生网络检查已通过；正式桌面二维码到相机真机扫码、多电脑跨网络切换仍须验收。
- 继续聊天增量 / 大消息分页、计划 / Todo、成果 / diff 与 APNs；任务和审批保留独立可查询回执。
- 实际双 Mac、iPhone、局域网/Tailscale 和关窗生命周期仍须按 G2/G5 验收；本轮本机回环 WSS 不替代真机证据。

## 验证记录

[机器可读证据](2026-09-07-norma-ios-connection-evidence.json) 保存失败→实现→通过日志。v13 的真实隔离 Keychain、正确/错误 TLS 身份、WSS 路径与期限、扫码到 SQLite 授权、撤销断开和服务释放均已通过。v13 最终领域测试 180 通过、1 忽略，新网络/存储集成三个用例通过，App check 与 Release（2m 19s）通过。全量 E2E 和 workspace 仍有失败，详见实时计划；未建立 HEAD 基线，未做真实 .app/iPhone 验收。

可重跑：

```sh
cargo test -p norma --test mobile_tls --test mobile_pairing
cargo test -p norma --test mobile_tls native_keychain -- --ignored --nocapture
cargo test -p storage --test mobile_access
cargo test -p e2e --test mobile_pairing_network --test mobile_pairing_persistence
```


v15 最终验证：Swift package 4 通过，iOS Xcode 6 通过 / 1 显式跳过（网络项已另行通过）；真实 iOS simulator → Rust WSS 包含重扫重连，16.14 秒通过。桌面 owner / QR / 权限取消三项检查、两项 headless Settings 检查、Storage 72 项、App check 与 Release（5m 32s）通过。完整 E2E 最终 199 通过 / 5 失败 / 11 忽略；此前全仓仍停在 Packager 断言，无 HEAD 基线。详细失败和实际 UI 限制均保留在证据 JSON，G2 未完成。


v16 最终验证：Swift package 5 通过；原生 iOS Xcode 8 通过 / 1 显式跳过，实际 simulator → Rust 快照检查单独通过（13.30 秒）。正式 GPUI 路由 / 跨安装隔离检查通过（0.24 秒），网络三项通过；Norma / Runtime 241 通过 / 4 忽略、App check 和 Release（3m 49s）通过。修正 Packager 测试对可选参数的过时字符串后，workspace App 单测 794 通过 / 0 失败 / 7 忽略，随后停在 E2E 194 / 11 / 11；此前单独完整 E2E 为 200 / 5 / 11，两次结果均保留且没有 HEAD 基线。护栏通过。Computer Use 最新返回 Mac 锁定，已请求手动解锁，实际卡片页面仍未验收。

v17 最终收口：Storage 74 项、输入 / 聚焦回归 8 项、App check、Release（1m 56s）和行数 / no-allow / 存储 / 性能 / diff 护栏通过。全仓仍停在 E2E 的 5 项失败；未把模拟器网络测试视为真实创建页面验收。

## 卡片删除（v18）

SwiftUI 卡片菜单进入系统 Form / Sheet，显示电脑、项目、卡片与明确的删除按钮；忙碌时按钮为「停止并删除卡片」，不会由滑动手势直接删除。菜单触控区域至少 44pt、使用语义字号及系统破坏性按钮；减少动态效果继续沿用卡片静态呈现，落实 Apple Design 的直接操作、清晰反馈与可访问性。

`delete_card` 复用设备级持久化命令账本，提交前保存未决文件，返回成功前等待现有画布 / 会话保存完成。`expected` 与当前 Runtime 卡片快照逐字段比较，新增 `turn_id` 和 `preparing` 状态；旧 run/session/turn 或内容变化返回 `card_changed`。正在思考、执行工具或等待审批必须同时有准确运行 / 会话 / 回合与 `confirm_busy=true`。准备中、未知状态、未消费 Hook 或存在排队 / 协调器任务返回 `state_not_ready`；非受控旧卡不能直接删除。

关闭复用已有 Runtime kill、审批取消、布局与缓存清理。已有 quick definition、任务记录和 Provider 对话文件保留；会话 checkpoint 随画布更新，不用 checkpoint 冒充历史归档。手机历史列表仍待接入。回执写入失败时，无论 query 或重复 submit，均返回 `outcome_unknown`，不会从未知退回 accepted，也不会重放删除。

v18 证据：Swift package 8 项、Xcode 11 通过 / 1 跳过（fixture 网络另行运行）、正式 WSS → headless GPUI → SQLite 卡片创建 / 删除与历史保留通过；原生 macOS Swift → Rust 为 1.00 秒，iOS simulator XCTest host → Rust 为 12.90 秒。忙碌确认、回合变化、关闭终端与撤销审批在 Runtime echo 测试中通过；不把它称为真实 Provider 的手机删除验收。Computer Use 仍返回 Mac 锁定，没有新创建 / 删除页面操作截图。详见证据 JSON。


v18 最终回归：Norma / Runtime **243 通过、4 忽略、0 失败**；App check（4.45 秒）、Release（2m 01s）、行数 / no-allow / 存储 / 性能 / diff 护栏通过。最终正式 GPUI 创建 / 删除检查（含只读删除拒绝、历史保留）0.69 秒通过。全仓 App **794 通过、0 失败、7 忽略**，随后停在 E2E **201 通过、5 失败、11 忽略**，失败涉及两项 Chrome auth 和三项语音；无 HEAD 基线，不宣称全仓通过。


## 原生对话读取（v19）

SwiftUI 项目卡片进入原生对话页，黑色背景、语义字体、完整可复制正文和系统 DisclosureGroup 工具 / 附件展开。界面不挤入摘要面板。一次页面绑定一个运行，后台 / 离开停止查询；过期运行、错误电脑与撤销权限清空旧正文。空卡需要在电脑开始会话后重新进入，手机输入框 / 发送 / 草稿 / 排队仍待接入。

Mac 通过正式 WorkspaceRouteIndex 选择准确窗口，检查 App 数据库、项目、路由、run/session，再在后台复用 Codex 历史 RPC 或 Claude JSONL reader。返回前再次确认窗口仍存在及运行身份；网络层同时复核授权版本。请求预算 4 秒，留 1 秒供 iOS 网络传输；v19时正文响应超过500KiB返回 `history_too_large`；v27已将较大页改为下面的无损分段传输。

Codex 每页 20 回合，完整页中回合顺序转为正序，稳定 ID 为 turn:item；刷新替换相同 ID 的正文，前页插到更早位置。发现无交集且仍有前页时重建最近列表并提示重新同步。Claude 每页 20 条完整记录，工具与长正文保留，序列化 Provider 游标向后追加；文件尾半行等待补齐，不能静默跳过。无效游标需要用户重新读取，不声称已有项目事件 cursor 同步。

真实 macOS Swift 和 iOS simulator XCTest host 都已通过正式 WSS / headless GPUI 路由，读回实际 Codex / Claude 固定回复；原生客户端还验证旧 run 被拒，Codex Rust WSS 用例另有错 session 与撤销检查。测试配对只批准临时项目只读，不修改真实设备授权。Claude 最初失败是测试 PATH 缺 Hook；已保留并检查启动回执，补齐 PATH 后通过。iOS 初次失败为新增测试文件尚未生成到工程，`xcodegen` 后通过。

重跑真实 Provider 历史链路：

```sh
swift build --package-path ios --product PairingProbe
cargo build -p norma --bin multisoul-agent-hook
# macOS 原生 Swift；以下两个测试分别验证 Claude 与 Codex。
env PATH="$PWD/target/debug:$PATH" NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" \
  cargo test -p e2e --lib e2e_mobile_claude_conversation_native_transport -- --ignored --nocapture
env PATH="$PWD/target/debug:$PATH" NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" \
  cargo test -p e2e --lib e2e_mobile_controlled_codex_dispatch -- --ignored --nocapture
# 将 NORMA_SWIFT_PROBE 换为 $PWD/scripts/check-norma-ios-transport.sh 即运行实际 iOS simulator host。
```

Computer Use 仍返回 Mac 锁定，未验收对话页实际交互；这些证据不等于物理 iPhone、双 Mac、Tailscale 或完整聊天功能通过。


v19 最终回归：Norma / Runtime **245 通过、4 忽略、0 失败**，Swift package **9 通过**，Xcode **12 通过、1 显式跳过**（真实 Provider 网络 fixture 已另行执行通过）。App check（4.62 秒）、Release（2m 01s）、行数 / no-allow / 指南行数 / 存储 / 性能护栏通过。全仓 App **794 通过、0 失败、7 忽略**，随后停在 E2E **201 通过、5 失败、12 忽略**；失败为两项 Chrome auth、MiniMax 手动语音和两项语音锁 poisoned，无 HEAD 基线，不宣称全仓通过。Goal 继续 G2–G5，下一步接入输入 / 发送 / 可靠排队。


## 原生消息发送（v20）

`send_message` 共用创建 / 删除的 control 授权、准确路由与唯一操作账本；先保存手机未决请求，再发送一次。服务器只有在 Codex 返回正式回合 RPC 或 Claude 发出正文已核验的准确 PromptSubmit 后才完成成功回执。成功表示 Provider 已收到消息，不表示任务已执行完成；后者仍以实际状态和对话为准。

Runtime 比较完整卡片快照、精确运行与会话。普通消息不覆盖 Thinking / RunningTool / WaitingApproval；正在启动或发送也不可重入。原生终端统一输入入口追踪电脑草稿及 Enter 顺序，发送期间拒绝混入输入；迟到的 Provider 提交只清除对应 Enter 之前的草稿。没有可靠的原生输入缓冲 API，导航键目前也可能触发保守限制，直到一次电脑端提交被确认。

消息 UTF-8 上限 32 KiB；拒绝控制字符但保留 LF / Tab。 Claude 多行 / Tab 必须等待 PTY 启用 bracketed paste，在未启用时不写任何字节；冷启动只在已有 25 秒准备预算中等待。真实 Claude 2.1.202 将每个粘贴 Tab 展开为 4 个空格，回执按这个精确转换核对，除此之外不 trim、不折叠空白。输入区提示 Tab 转换；原始手机请求仍完整保留。Codex RPC 保留原始 Tab。手机未决文件包括原文，写入与读取均限制 256 KiB；iOS 使用完整文件保护，macOS 原生测试写入后设置 0600。启动后、写入后或连接中断时无法确认的结果统一为 outcome_unknown，保留原操作，不能换 ID 自动重试。

SwiftUI 底部 safeAreaInset 放系统多行 TextField 与 44pt 发送按钮，系统键盘直接推起输入区，正文保留完整滚动空间。当前支持空卡启动和空闲会话续发，等待结果 / 查询 / 成功清空输入与保留未知请求。忙碌排队、未提交草稿持久化和手机中断仍未接入，不把普通发送当成完整 G3。

真实 Codex 冷启动复现了新 rollout 文件存在但尚为空：任务已通过 turn/start 接收，原生 TUI 的第二连接却提前 resume。修复仅在当前新线程的这个特定错误下沿用 10 秒附着等待；不重新发送任务，不吞掉既有会话错误。TDD 和两种真实原生客户端均复验通过。macOS 的完整文件保护标志会返回 Cocoa 513，现按平台区分文件保护，iOS 保留原生保护而 macOS 测试使用 0600。

```sh
swift build --package-path ios --product PairingProbe
cargo build -p norma --bin multisoul-agent-hook
# macOS 原生 Swift，两个 Provider 顺序执行。
env PATH="$PWD/target/debug:$PATH" NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" \
  cargo test -p e2e --lib _messages_native -- --ignored --nocapture --test-threads=1
# 实际 iOS simulator host；避免并行操作同一模拟器 / DerivedData。
env PATH="$PWD/target/debug:$PATH" NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" \
  cargo test -p e2e --lib _messages_native -- --ignored --nocapture --test-threads=1
```

这里两种 Provider 都从空卡实际接收两条消息，手机读到回复，重复提交只返回原回合；错误快照被拒绝、关闭后控制失效。测试使用临时 App 数据库和只对测试项目的 control 配对，不等于真实 iPhone、双 Mac / Tailscale 或 Computer Use 输入操作验收。Computer Use 仍因 Mac 锁定未完成。

v20 最终回归：Norma / Runtime / Terminal **296 通过、6 忽略、0 失败**；正式命令 / 对话网络检查 **3 通过、1 显式忽略**；Swift **10 通过**；Xcode **13 通过、1 显式跳过**。两种实际 iOS simulator 多行链路在 **43.03 秒**通过，正文包含首尾空白、换行与连续 Tab，准确回执和去重均通过；当前 Provider 为 Claude **2.1.202** / Codex **0.153.4**。App check（2.94 秒）、Release（2m 41s）与行数 / no-allow / 指南 / 存储 / 性能 / diff 护栏通过。

全仓 App **794 通过、0 失败、7 忽略**，随后停在 E2E **195 通过、11 失败、14 忽略**：两项 Chrome auth、一项语音显式 Apply 断言与八项 poisoned voice lock。新增的语音 Apply 失败单独复跑 **0.25 秒通过**；没有未修改 HEAD 基线，也未修复这些全仓失败，因此不宣称全仓通过。最后代码仅修正注释中的实际 Claude 版本，没有功能变化。

## 本地草稿与消息回执（v21）

`ChatDraftBook` 为 App 共享的 MainActor 状态，`ChatDraftStore` 在独立 actor 上读写文件；SwiftUI 页面只绑定当前完整范围的文本。范围包括 machine / workspace / route / node / run / session，空卡和已启动会话互不自动迁移。编辑用 UUID 区分版本，后台写入串行且合并期间的新编辑，不积累逐键任务。页面取消不会取消草稿保存；进入后台用系统短期执行时间收口，失败不丢弃未保存正文。

手机的 `PendingMobileCommand.draftID` 仅作本地回执关联，服务器 Request 不包含它。发送前 flush 草稿，再持久化未决请求、发送一次；回执先复核完整命令和 Provider 身份，准确成功才清除对应草稿版本，并等待清除落盘后移除未决记录。晚到成功、同文新版本、未知结果、跨电脑 / 会话不能错误清理。重查遇到上次清理写失败仍明确失败。

文件采用原子替换；iOS 请求 completeFileProtection，macOS 使用 0600。每份草稿 64 KiB，总文件 4 MiB / 128 份；大小限制同时作用于写入和读取，不静默覆盖损坏文件、截断正文或淘汰旧草稿。全局本地草稿 Sheet 保留来源标签与时间，支持复制和确认删除，避免已关闭窗口或已删除卡片的草稿无法找回。该标签只用于显示，授权和发送仍由完整范围决定。

TDD 验证快速连续编辑、六维范围隔离、同文版本保护、原始消息记录重读、草稿版本不进入网络协议、结果未知保留、清除失败重查、超限和损坏文件保护。macOS Swift 与 iOS simulator 均通过两种实际 Provider 的草稿恢复 / 发送 / 清除链路。iOS 模拟器读取文件保护属性为 nil，不能证明设备锁屏保护；真实设备断言单独保留，模拟器明确跳过。原生导航 UI 测试覆盖本地草稿入口和返回，但不代替实际编辑、切换和锁屏的 Computer Use / 真机验收。

v21 最终验证：Swift **13 通过**；Xcode **16 通过、2 明确跳过、0 失败**，含本地草稿入口 / 关闭导航。两项跳过分别是普通运行未提供网络 fixture、物理设备文件保护；两种真实 Provider 的最终 iOS 草稿链路已另行在 **33.45 秒**通过。iOS 模拟器 Release 构建和 iPhone SDK 的 build-for-testing 均通过；后者未签名、未安装到物理设备，不能作为真机验收。

本轮 E2E **201 通过、5 失败、14 忽略**，失败仍涉及两项 Chrome auth、MiniMax 手动语音与两项 poisoned voice lock；没有 HEAD 基线，不宣称全仓通过。生产 Rust 行为本轮未变，复用 v20 Mac Release；diff / 行数 / no-allow / 指南护栏通过。Computer Use 再次返回 Mac 锁定，未取得输入 / 导航切换的实际操作证据，不重复请求解锁。G2–G5 的可靠排队、手机中断 / 审批、增量恢复、指挥台 / Todo / APNs、真实双 Mac / iPhone / Tailscale 与无障碍范围均继续保留。

## 原生手机中断（v22）

手机通过 interrupt_turn 停止准确回合。SwiftUI 聊天工具栏使用原生按钮、44pt 触控与可访问性名称；等待时使用系统进度反馈，结果 Sheet 显示“已停止”“自然完成”或“失败结束”。这是 Apple 内容优先与即时反馈的落地：聊天正文不被新面板挤压，按下停止不会伪造任务终态。

命令复用现有项目控制权限、授权版本、窗口路由、持久化唯一操作账本。Swift 本地先存盘再提交一次，断线仅按原 ID 查询。Runtime 比较 node/profile/run/session/turn，不比较会正常变化的标题或 thinking/tool/approval 阶段；等待期间投影 interrupting，仍属于忙碌删除。旧回合请求不能停止新任务，重复 submit 只能取得原回执。

Codex 等待实际 turn/completed；Claude 复用后台捕获 EOF、前台一次 Escape、后台读取新增准确 prompt_id 中断记录。接收 RPC、写入 Escape、超时、关闭和读取失败均不能冒充“已停止”。前台等待使用非阻塞检查；25 秒本地等待给网关 30 秒总预算留下 5 秒保存回执。Claude 系统 Escape 直接使用已预约的 LiveNode 输入，不进入电脑草稿追踪；不得清除真实电脑草稿，且消息正在提交时不能混入系统按键。

~~~sh
swift build --package-path ios --product PairingProbe
cargo build -p norma --bin multisoul-agent-hook
env PATH="$PWD/target/debug:$PATH" NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" \
  cargo test -p e2e --lib interrupts_native -- --ignored --nocapture --test-threads=1
# 换为原生 iOS simulator XCTest host：
env PATH="$PWD/target/debug:$PATH" NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" \
  cargo test -p e2e --lib interrupts_native -- --ignored --nocapture --test-threads=1
~~~

真实无工具任务由正式 App 启动；Claude 观察到真实回复首行后才停止，Codex 以正式 turn/start 确认的活动回合为准。两种 Provider 都验证原生客户端停止、持久化后回查、重复停止返回原回执、同一 session 续发真实消息，以及新回合不被旧操作重放影响。macOS Swift 最终链路 29.28 秒通过。实际 iOS simulator 对应链路在 50.85 秒通过；Computer Use 仍因 Mac 锁定未完成，不等于真实 iPhone、双机或 Tailscale 验收。

v22 回归：Runtime 140 通过 / 3 忽略；Swift 14 通过；iOS DeviceTests 16 通过 / 2 明确跳过，真实网络已另行跑通。App check、Mac Release（2m 21s）、iOS 模拟器 Release 和未签名 iPhone SDK build-for-testing 均通过。全仓 App 794 通过 / 7 忽略，之后 E2E 201 通过 / 5 失败 / 16 忽略；两项 Chrome auth、MiniMax 手动语音、两项 poisoned voice lock 未修复，无 HEAD 基线。常规约束检查通过，完整证据和未完成范围见 JSON。

## 可靠消息队列存储（v23）

本阶段只实现存储，尚未提供手机入队接口或自动发送调度。MobileCommandStore 在同一 SQLite 事务中保存完整 NewQueuedMessage 和不可变的入队回执，新队列表通过设备 / 操作 ID 关联原命令账本；同 ID 不同正文或范围冲突。入队时的成功与 Provider 收到消息是两个独立事实，客户端接入时必须读取最新队列状态。

queued → dispatching 的领取与 queued → cancelled 的撤回使用事务竞争；同一运行卡片按真实入队顺序领取，其他卡片独立。明确输入前拒绝允许 defer，未知执行结果禁止重新领取。确认发送需要准确 node/run/session/turn 回执，已有终态不可被另一结果覆盖。服务 owner 更换时旧队列不自动执行：未领取显示取消，已领取未确认显示未知，原文继续保留。

活跃队列最多 128 条，每条最多 32 KiB，与原生消息正文约束一致。写入失败、超限或同 ID 冲突都不截断 / 淘汰内容。测试注入第二次 INSERT 失败，确认第一张表中的入队成功也回滚；同时验证原有命令结果不受新表影响。

v23 证据：聚焦 4 项通过；Storage 全量 78 项通过；既有移动网络命令、配对持久化和卡片持久化共 4 项通过；App check 和常规约束通过。尚无队列后台调度、WSS、SwiftUI 或真实 Provider 排队证据。这些是后续实现的必要步骤，完整 G2–G5 范围未变。

## 后台队列与 SwiftUI 接入（v24）

queue_message 持久化完整消息并返回不可变交接回执；queued_message 查询独立状态，cancel_queued_message 仅在 queued 时撤回，带当前 grant_revision。同一设备 / 操作 ID 不能换正文或目标，读取限定授权项目和该手机自己的条目。旧窗口关闭后只查询历史，不取得新执行权。

Mac-owned worker 每 500ms 检查最多 128 waiting / 8 sending；通过既有快照请求拿到最新 live card，在 control/revision、route/node/profile/run/session 一致且 ready/done/ended/failed 时领取。权限锁持有期间注册 CommandLease 并 claim，前台仍只执行原有 send_mobile_chat。普通手机 send_message 遇到已有队列返回 queue_pending，不能插队。等待队列没有 30 秒超时；实际发送保留原有 30 秒完成预算。

state_not_ready / desktop_input_pending / terminal_not_ready / card_changed 仅来自前台输入前验证，允许 defer；已开始后无回执或不确定失败为 outcome_unknown。新 owner 不执行旧队列。已 dispatching 不允许声称安全撤回，即使尚未显示 Provider 确认。写入失败停止移动服务，不继续接收无法可靠处理的消息。

SwiftUI 输入框在忙碌或有未完成队列时使用「加入队列」；收到准确交接回执才清除同版本草稿并允许追加，消息请求本身留在本机记录继续跟踪。Sheet 中分别呈现等待、提交中、已收到、取消和未知；详情只在前台每两秒查状态，不自动 submit。电脑页提供独立入口，即使项目/电脑已移除，也可找回原文。复制有即时反馈，清除记录需确认且不停止电脑任务。Apple Design 落实为紧凑原生列表、系统安全区/键盘避让、语义字号和44pt触控；实际页面操作尚待解锁验收。

~~~sh
swift build --package-path ios --product PairingProbe
cargo build -p norma --bin multisoul-agent-hook
env PATH="$PWD/target/debug:$PATH" NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" \
  cargo test -p e2e --lib queues_native -- --ignored --nocapture --test-threads=1
env PATH="$PWD/target/debug:$PATH" NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" \
  cargo test -p e2e --lib queues_native -- --ignored --nocapture --test-threads=1
~~~

真实无工具任务运行时排三条，第二条撤回，停止原任务后由 Mac 自动发送第一和第三条；客户端只查询。验证两条准确新回合、完整LF/Tab正文和顺序，撤回项不出现在真实历史，重复原入队仍仅返回交接回执，发送完成后不可撤回。Claude 真实历史多一条 `[Request interrupted by user]`，测试明确验证而非泛化忽略所有额外记录。macOS 两种 Provider 最终36.71秒通过，iOS simulator59.32秒通过；不是物理iPhone或双Mac/Tailscale验收。最终回归详情见 evidence JSON 的 message_queue_v24。


v24 最终检查：Storage78通过、Swift15通过、iOS DeviceTests17通过/2明确跳过、UI导航2通过，移动网络与持久化6通过。App check、Mac Release（2分钟）、iOS模拟器Release与未签名iPhone SDK build-for-testing通过。全仓App794通过/7忽略，E2E195通过/11失败/18忽略；两项Chrome授权、一项语音显式Apply、8项poisoned voice lock。Apply单独重跑0.24秒通过，没有HEAD基线，不宣称全仓成功。实际Computer Use、物理iPhone、双Mac/Tailscale和无障碍验收未完成。

## 原生审批与指挥台（v25）

`action.type=approvals` 接受 device_id、token 和 request（workspace_id、route_id），只读可查。响应回显 request，并返回完整的 ApprovalSnapshot；前后核验授权版本，等待 GPUI 时不占权限锁。最多128项 / 500KiB，超限明确失败，不截断工具输入。

写入沿用 `command` 账本，新增 `decide_approval {expected, decision}`；expected 绑定 node/profile/run/session/turn/request，外层绑定电脑、手机、项目、窗口、权限版本和 operation。每次调用的 request_id 不可变，即使工具输入相同也不能复用上一请求。只接受 Provider 提供的单次 allow/deny；选择复用 G1 审批入口，在后台增量读取项目审计，25秒预算给30秒通用命令预留5秒回执落盘时间。GPUI 前台未增加文件或 SQLite 读取。

`submitted` 表示本地传输层接收了选择，不证明工具执行或多端竞争获胜；`request_closed` 表示请求关闭，无法认定本机选择生效。未知结果保留 outcome_unknown，重新连接只查询原操作。Mac 先处理、过期、错误运行与重复的新操作均无法复活旧请求；原 operation 重放只返回原回执。

SwiftUI 指挥台汇总全部电脑；v25最初每轮最多四台并行、前台每3秒查询，v58已替换为下述电脑级实时订阅。读取过程中授权变化会丢弃该电脑本轮全部内容；读取失败不能以无待办表示成功。系统分页卡片保留滚动正文、44pt按钮、语义字号与原生状态反馈；左右滑动切换，显式按钮提交。待决记录按完整来源匹配，不与另一电脑 / 项目 / 窗口的相同局部ID混合，也不落入普通卡片创建结果页。

手机的完整审批预览只保存于受完整文件保护的本机 pending command，不进入网络写命令；数字使用 Decimal 保留大整数。决定记录保持可读和可回查，直到用户明确清除；清除不撤销电脑动作。Codex 同步外部 Hook 以无操作的 informational 项展示；不会读取或传送 Hook 命令 / 输出 / 配置路径。真实 Hook ID 可能含路径，统一列表使用 `external:`加 SHA-256摘要，保留稳定身份且不泄露路径。

原生两 Provider 检查：macOS Swift **46.56s**，iOS simulator **64.57s**。真实精确写入的拒绝/批准、存储重读、同ID回查和旧请求拒绝均通过。Codex 配置了阻塞外部 Hook，因此客户端协议检查在用户已授权的隔离进程内使用临时 ZDOTDIR 与 Codex wrapper 暂不加载外部 Hook；read-only / untrusted 与单次审批保持。生产代码、全局配置和正常应用进程不使用此 wrapper。另有保留全部外部 Hook 的正式原生读取/停止检查：macOS 12.43s、iOS simulator 21.48s 通过，四项真实外部等待不提供手机批准，精确停止后无写入。最终 Mac Release（2m13s）、模拟器 Release 与未签名 iPhone SDK 构建通过；详细证据见 JSON。

```sh
bash scripts/check-norma-mobile-approvals.sh macos
bash scripts/check-norma-mobile-approvals.sh ios
# 保留电脑全部外部 Hook，仅验证提示与停止，不批准写入：
env PATH="$PWD/target/debug:$PATH" NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" \
  cargo test -p e2e --lib e2e_mobile_codex_external_notice_native -- --ignored --nocapture --test-threads=1
```

领域、网络、Swift、Xcode、构建和未完成验收均保存在 [连接证据](2026-09-07-norma-ios-connection-evidence.json)。全仓 E2E 仍有5项失败，没有 HEAD 基线；不宣称全仓通过。Computer Use 本轮仍返回 Mac 锁定，实际页面与双物理Mac / iPhone / Tailscale、计划 / Todo、成果 / diff、APNs 与无障碍范围继续保留。


## 项目增量订阅（v26）

`watch_project` 复用既有 WSS 身份、项目授权与 GPUI 快照来源。每连接创建独立 stream_id 与 sequence=0 全量基线；同一窗口只有变化卡片和明确删除项进入 delta，并附完整 ID 顺序。换窗口/关闭/不可用状态发送完整快照。心跳沿用当前序号，缺口或新连接不能沿用旧基线。

这是一份可合并的实时页面投影，不是事件审计日志。没有新数据库表或历史回放服务；断网时的中间状态可合并，最终执行结果仍以准确命令/消息/审批回执为准。每项目/事件最多500KiB；超限明确失败，不能展示不完整列表。

前台在既有画布/会话脏标记、运行事件与项目路由变更处只递增原子计数；后台最多250ms合并变化，闲时2秒重新核验授权并读取GPUI后才发心跳。不会在GPUI前台读SQLite、序列化网络数据或扫描历史。进程级计数可能唤醒另一安装的订阅，但不携带内容，读取仍按各自数据库和授权隔离；若实际空闲开销需要优化，再改为每owner失效信号。

Swift `ProjectTimeline` 校验来源、序号、唯一卡片ID、删除存在性、顺序覆盖和完整快照结构。错误清空基线。`projectUpdates` 在原Keychain凭据下最多三次只读重连（1+2+4秒退避），每次重新取得全量；撤销、证书变化与无效协议终止。缓冲满显式报缺口，不能静默跳事件；重连不触发命令重发。实际macOS断流产生NSPOSIXErrorDomain而非URLError，已在共用socket接收入口归一为连接失败，并保留取消和格式错误区别。

SwiftUI项目卡片页已改为订阅，恢复期间清空卡片和控制权限，显示系统进度提示；窗口或权限变化关闭旧操作Sheet，未决操作仍保存供回查。继续使用Apple Design的系统按钮、语义字体、44pt触控和减少动态效果。聊天与指挥台审批暂仍按原查询周期刷新。

证据：
- Rust投影测试（完整基线、变化/删除、心跳、换窗口）通过：`/tmp/norma-ios-events-domain-v26.log`；红测保留于同前缀 `domain-red-v26.log`。
- 真实TLS/WSS + SQLite测试（delta、换窗口、降权断流、新基线、越权和关服）通过：`/tmp/norma-ios-events-wire-v26.log`。前台内容为明确fixture。
- Swift全量17项通过：`/tmp/norma-ios-events-swift-final-v26.log`；含缺口清空、重复事件不重复删除、错项目拒绝。
- 原生客户端 → 真实WSS → 正式GPUI/Runtime/SQLite：macOS 12.76秒，iOS simulator 35.51秒通过持续保活、空卡创建/删除、降为只读后重连、写入拒绝与取消；日志分别为 `/tmp/norma-ios-events-native-mac-v26.log` 和 `/tmp/norma-ios-events-native-ios-v26.log`。macOS初次因断线错误类型失败的证据保留于 `native-mac-red-v26.log`，已修复复跑。
- 本轮Computer Use仍返回Mac锁定，未取得页面实际操作证据；真实双Mac/iPhone/Tailscale、聊天事件、计划/Todo、成果/diff、APNs、语音与无障碍仍待完成。

v26最终构建：App check（5.57s）、Mac Release（2m 03s）、模拟器Release与未签名iPhone SDK build-for-testing通过；产物未安装到物理iPhone。原生DeviceTests 19通过/2跳过，导航3通过。全仓App 794通过/7忽略，E2E 201通过/5失败/22忽略；两项Chrome授权、MiniMax手动语音与两项poisoned lock仍失败，未做HEAD基线。Norma 113通过/1忽略、移动网络/持久化11通过与常规护栏通过。日志统一为 `/tmp/norma-ios-events-*-v26.log`，详细映射见同目录证据JSON。


## 长对话分段传输（v27）

现有Provider页内可含大于500KiB的单条正文或工具记录，原实现会整页拒绝。现在保留原历史游标和页顺序：普通页仍一次返回conversation；较大页在本次同一WSS连接连续返回conversation_fragment，包含transfer_id、offset、total_bytes和text。offset按原始UTF-8字节计数，分片只在字符边界切分；每片64KiB，最坏JSON转义64×6=384KiB，为协议外层留余量。完整组装的JSON是原ConversationResponse，不裁剪正文或工具详情。

每次读取冻结一个Provider页，仅占既有8个连接中的一个；单页32MiB，重连重新读取原Provider游标，无持久化分片缓存或断点blob存储。此上限不能突破Codex私有RPC的8MiB响应或Claude单行8MiB/每页扫描16MiB边界，超限明确失败。该变化解决手机500KiB门槛，没有声称支持无界历史页。

Mac开始/结束分段前用正式快照来源核对App数据库、项目窗口、节点、run和session；每片前复验设备权限与revision。授权变化会关闭已标记连接。最后一片前运行变化返回session_changed，客户端已有的部分数据不形成完整响应。全部网络、序列化和JSON重组仍在后台；GPUI不增加文件或SQLite读写。

Swift检查请求ID、冻结传输ID、总长度、每片offset与尺寸，再完整解码和匹配原ConversationRequest。缺片、混流、断线和取消不更新Timeline或游标；后续只读重试不重发任何任务。原生页面布局本轮未修改，继续沿用正常SwiftUI聊天；聊天仍两秒刷新，事件订阅待继续接入。

验证证据：
- Rust/Swift先红后绿：UTF-8边界、转义、完整性、错误offset/混流/超限拒绝；日志 `/tmp/norma-ios-chat-transfer-{domain,swift}-red-v27.log`、对应绿测，以及Swift全量18通过日志 `swift-final-v27.log`。
- 真实TLS/WSS + 明确前台fixture：600000字节中文正文与600000字符工具输出无损；最后一片前run变化拒绝完成；撤销不能开始读取。两项网络检查通过：`/tmp/norma-ios-chat-transfer-wire-final-v27.log`。
- 原生Foundation/Keychain + Rust fixture大响应：macOS0.75秒、iOS simulator12.15秒；`/tmp/norma-ios-chat-transfer-native-mac-v27.log`、`native-ios-v27.log`。
- 保留真实Codex/Claude原生终端与正式GPUI/Runtime，两种真实Provider首条/续发/完整历史/去重回归28.20秒通过：`/tmp/norma-ios-chat-transfer-provider-mac-v27.log`。不使用该小消息回归证明真实模型产生过大响应。

v27最终验证：Norma114通过/1忽略，移动网络/持久化12通过/1忽略（大响应原生项已单独运行），Swift18通过，iOS DeviceTests20通过/2跳过。App check5.35秒、Mac Release2m 03s、模拟器Release与未签名iPhone SDK build-for-testing通过。全仓App794通过/7忽略，E2E201通过/5失败/22忽略；仍是两项Chrome授权、MiniMax手动语音与两项poisoned lock，无HEAD基线。常规护栏通过，日志 `/tmp/norma-ios-chat-transfer-*-v27.log`。Computer Use再次返回Mac锁定，本轮没有页面布局修改或新的真实UI操作证据；沿用v26原生导航，不能视为长内容页面验收。


## 持续聊天事件（v28）

`watch_conversation` 使用独立WSS连接，scope绑定准确workspace/route/node/run/session，初始cursor不得非空。新连接有独立stream_id，sequence0完整基线；正文变化才推进序号，无变化心跳的page为null。Codex在最近20回合页变化时发送该完整页，Claude沿准确JSONL游标连续读完整页，有后页立即继续；当前没有逐token/item差分协议。

复用现有后台项目失效信号、250ms合并与2秒空闲复验。每个事件来自新的Provider读取、准确GPUI状态与授权复核；权限revision变化终止旧流。普通响应和事件共用v27分片，大事件完整JSON为ConversationEvent；分片起止核验run/session，每片前核验授权。没有新增数据库、依赖或缓存服务，前台不做IO。

Swift `ConversationEventSequence` 核验完整scope、流/序号、页结构与固定方向；缺口清空状态。`conversationUpdates` 使用系统 `AsyncThrowingStream(unfolding:)`，仅在消费者继续时读下一项；没有生产Task或可静默丢页的应用缓冲。断流/缺口最多1/2/4秒三次只读重连，每次新基线；撤销/错会话终止。取消中断URLSession读取，丢弃序列释放连接所有者。

SwiftUI聊天页独立消费项目流和对话流，控制权限只来自当前项目状态。恢复先清空历史，旧翻页取消并通过epoch防迟到覆盖；保留原生输入框、草稿、可靠队列和停止任务。后台/离开页取消订阅。Apple Design沿用语义字号、系统键盘避让和完整正文滚动，本轮未取得Computer Use实际聊天操作证据。

验证：

- TDD：域状态/序号、Swift缺口/错run清空与真实WSS新协议均先红后绿，日志 `/tmp/norma-ios-chat-events-{domain,swift,wire}-red-v28.log` 与对应绿测。
- 正式WSS → GPUI/Runtime → 实际Codex/Claude连续两轮消息，原生macOS **23.13秒**、iOS simulator **35.29秒**通过；确认两条准确用户原文、assistant更新和取消，日志 `provider-{mac,ios}-v28.log`。
- 原生大响应与大事件fixture在iOS **12.48秒**通过（`large-ios-v28.log`），精确核验完整中文正文和转义工具JSON；它不证明真实Provider生成超长消息。
- 新原生恢复fixture：一次unavailable结束原流，客户端recovering清空旧页，新基线为精确替换文本，随后真实服务撤销，客户端终止为unauthorized。macOS连同网络检查 **5.80秒**、iOS **13.91秒**通过，日志 `recovery-{mac,ios}-v28.log`。此故障来自明确测试fixture，不称为真实Provider断网验收。
- Norma **115通过/1忽略**，移动网络/持久化 **13通过/2忽略**（原生项已另行显式通过），Swift **19通过**，DeviceTests **21通过/2跳过**，导航 **3通过**。App check **8.09秒**，Mac Release **2m12s**、模拟器Release与未签名iPhone SDK build-for-testing通过；行数/no-allow/指南/存储/性能/局部rustfmt/diff护栏通过。完整日志前缀均为 `/tmp/norma-ios-chat-events-`。

全仓仍停在E2E：App794通过/7忽略，E2E201通过/5失败/22忽略；两项Chrome授权、MiniMax手动语音与两项poisoned voice lock，未做HEAD基线，不宣称全仓通过。Computer Use再次返回Mac锁定；实际聊天交互、长内容/无障碍、双物理Mac/iPhone/Tailscale及计划/Todo、成果/diff、APNs仍按完整Goal推进。


## 计划生成与原生下一步页面（v29）

- Norma复用既有ModelFactory、Provider配置与Rig，只生成项目总结和最多8项建议，没有执行工具或AgentCardAdapter。模型只能引用Mac给出的受控Codex/Claude；不存在、未受控、重复身份或伪造完成字段会被拒绝。生成尚未经过真实云模型质量验证。
- 每份建议保存完整用户方向、项目/窗口、目标卡片及run/session、具体指令；新方向形成新计划，旧建议不覆盖。SQLite发布后才返回`plan_id/proposed`回执，重复操作复用原结果。生成过程不会创建卡片或派任务。
- `generate_plan`复用既有控制权限、授权版本和幂等命令链路；`project_plans`允许已授权只读，按项目严格分页，每页最多4份。模型最多20秒，App关闭窗口会取消本地请求；迟到的已删除/变更目标不能发布。已开始的生成不因权限撤销回滚，撤销后不能读取计划或回执；执行Todo仍须重新核验。
- SwiftUI从指挥台和项目进入「下一步」，原生列表展示方向、总结、执行Agent和完整指令，支持切换电脑、历史分页及调整方向。未决方向先存手机，断线/重开只回查。计划回执与卡片/消息回执分别校验，不伪造node_id或任务完成；Todo接受/拒绝、任务关联和真实完成状态仍未接入。
- Apple Design落到黑色背景、语义字号、系统导航/输入/DisclosureGroup、可滚动完整文本和原生按钮反馈。Computer Use再次返回Mac锁定，因此实际计划页面的操作、长内容和无障碍验收仍未完成；模拟器导航不能替代这些证据。

验证：Agent/Storage **152通过**；正式WSS+GPUI/SQLite的生成、关窗取消和删卡拒绝 **3通过**。原生生成链路macOS **1.21秒**、iOS simulator **8.69秒**通过，使用本地HTTP模型fixture，经真实模型工厂/网络与持久化链路，不等同真实云模型或物理iPhone。Swift **21通过**，DeviceTests **23通过/2跳过**，原生导航 **4通过**。App check、Mac Release（3m26s）、模拟器Release和未签名iPhone SDK构建通过。全仓App **794通过/7忽略**，E2E **204通过/5失败/23忽略**；5项仍为Chrome授权/语音，未做HEAD基线。详细范围及可重跑命令见连接文档和evidence JSON。

下一步：为Todo建立一次确认/拒绝和原始建议到实际命令的准确关联，复用已有发送/队列/回执；继续成果/diff、APNs与G5，完整Goal保持active。

输入界限：direction最多8KiB、项目context最多32KiB、卡片最多64个；输出summary最多4096B、标题最多320B、每项指令最多8KiB、完整draft最多64KiB/记录最多80KiB。超限明确拒绝，不截断内容。`project_plans`严格按sequence降序，`before`排除上一页末项。读取与撤销共享授权门，查询不在GPUI前台执行。

```sh
cargo test -p agent -p storage
cargo test -p e2e --lib e2e_mobile_plan_generation -- --nocapture
swift test --package-path ios
swift build --package-path ios --product PairingProbe
NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" \
  cargo test -p e2e --lib e2e_mobile_plan_generation_native -- --ignored --nocapture
NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" \
  cargo test -p e2e --lib e2e_mobile_plan_generation_native -- --ignored --nocapture
```

首次聚焦WSS检查返回`invalid_request`（缺少生成协议），Swift检查缺少类型，原生导航缺少入口；这些失败后才加入对应实现。生命周期检查还发现macOS的accept可能继承非阻塞标志，测试socket改为有5秒预算的阻塞读取后，真实HTTP EOF取消检查通过。全仓检查发现旧空卡fixture未穷举新的生成命令，已明确拒绝该fixture执行计划，计划由正式GPUI测试覆盖；最终全仓剩余5项失败如上。


## Todo 决定与执行关联（v30）

- 计划详情可确认/不采纳每项Todo；确认Sheet完整展示原指令、电脑/项目/执行Agent和当前任务。执行会话与生成时不同会明确提示，空卡说明确认后才启动。保持Apple Design的黑色背景、系统List/Form/Sheet、紧凑语义字号、完整滚动内容和至少44pt行操作。
- Mac只从已保存的计划取指令，确认绑定用户预览的当前CardSnapshot，同node/profile可显式选择当前run/session；不允许手机替换指令或改派别的卡片。计划原目标和方向不覆盖。项目权限/版本、准确窗口/运行由正式链路复核。
- SQLite以(plan_id,todo_index)保证唯一决定，与原命令/可靠消息队列同事务提交。拒绝只保存决定，不派任务；接受复用原发送/排队/回执。跨手机只能一方决定，重试查询原结果。修复查询决定时重开SQLite导致的迁移锁竞争，复用当前连接。
- SwiftUI从项目事件取得当前控制状态，每2秒只读该计划的共享决定；后台/断流清空可操作状态，旧确认不能用于新会话。先保存本机决定再提交一次，恢复只回查。本人尚未发送的队列可撤回；真实送达后可进入同一实际会话，重开的run/窗口不冒充旧任务。已送达/拒绝/撤回后本机查询记录可清理，Mac原计划和决定保留。
- 「Agent已收到」仅表示实际发送回执，**不代表任务完成**。实际任务终态、成果验收及明确未发送后的显式重新安排尚未接入；未知结果不自动重放。确认后的建议不可改写，后续方向仍生成新计划。

验证：Agent/Storage **154通过**，移动网络/持久化 **14通过/3忽略**（含Todo真实WSS竞争/拒绝/排队检查）。两种真实Codex/Claude经正式GPUI/Runtime与原生macOS **23.44秒**、iOS simulator **42.93秒**通过；原始3项计划为明确fixture，实际执行两条无工具任务并核验真实回复，拒绝项不进入对话。该测试不证明真实Norma生成质量。Swift **24通过**，DeviceTests **26通过/2跳过**，既有导航 **4通过**。全仓App **794通过/7忽略**，E2E **204通过/5失败/25忽略**；5项仍是Chrome授权和语音，未做HEAD基线。Computer Use本轮仍返回Mac锁定，真实Todo页面操作和物理iPhone验收未完成。最终构建状态与日志见连接文档及evidence JSON，完整G2–G5仍未完成。


协议复用现有`command`信封，新增`decide_plan_todo {plan_id,todo_index,choice}`；choice为`{type:"reject"}`或`{type:"accept",target:CardSnapshot,queue:bool}`。接受返回原发送/队列回执；拒绝返回`{plan_id,todo_index,status:"declined"}`。`project_plan {workspace_id,plan_id}`按当前read权限返回不可变plan和带真实receipt/queued_message的decisions；控制权限与grant_revision继续在Mac端复验。

`ProjectTodoDecision.deliveryState`只投影declined/dispatching/sent/not_sent/queued/cancelled/outcome_unknown；`deliveryResult`关联原生run/session/turn，不能从当前卡片Done推断过去Todo完成。只有原receipt.route和实际run/session仍对应当前卡片，页面才显示「查看所在对话」。

```sh
cargo test -p agent -p storage
cargo test -p e2e --test 'mobile_*'
swift test --package-path ios
swift build --package-path ios --product PairingProbe
cargo build -p norma --bin multisoul-agent-hook
PATH="$PWD/target/debug:$PATH" NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" \
  cargo test -p e2e --lib todos_native -- --ignored --nocapture --test-threads=1
xcodegen generate --spec ios/project.yml
PATH="$PWD/target/debug:$PATH" NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" \
  cargo test -p e2e --lib todos_native -- --ignored --nocapture --test-threads=1
```

本轮未关闭生产外部Hook或修改用户全局配置。原生Provider任务严格要求不读写文件、不调用工具，验证FIRST/SECOND原文与实际assistant回复各一次，并确认DECLINED不存在、两个不同真实turn仍绑定同一run/session。底层使用正式WSS/GPUI/Runtime，GPUI为headless测试，不等同启动真实macOS .app。SwiftUI已编译，导航检查未覆盖Todo确认交互，Computer Use结果为锁屏，不能据此宣称完整页面验收。

日志前缀`/tmp/norma-ios-todos-`：`storage-red-v30.log`、`network-red-v30.log`、`swift-red-v30.log`和`ui-core-red-v30.log`记录聚焦红测；`domain-v30.log`、`network-regression-final-v30.log`、`swift-v30.log`、`provider-mac-v30.log`、`provider-ios-v30.log`、`device-v30.log`、`navigation-v30.log`、`workspace-v30.log`记录最终对应验证。单次`network-regression-v30.log`误用Cargo --tests也跑了lib并停在相同5项失败，最终移动集成按明确`--test 'mobile_*'`完成，不宣称全仓通过。

v30构建收尾：App check **5.41秒**、Mac Release **2m17s**、模拟器Release与未签名iPhone SDK build-for-testing通过；行数/no-allow/存储/性能/指南/diff护栏通过，canvas/terminal/runtime依赖树无GPUI。未安装到物理iPhone。日志为`/tmp/norma-ios-todos-{app-check,mac-release,ios-release,device-build,dependencies}-v30.log`。


## Todo 准确回合终态（v31）

`project_plan.decisions[].outcome` 独立于发送回执，返回 `{key:{node_id,profile,run_id,session_id,turn_id},state}` 或 null。仅真实发送成功回执或队列 sent 回执可查询对应终态；拒绝、未发送、未知回执没有执行结果。SwiftCore 要求 key 与真实回执及原目标完全匹配，拒绝错回合、错节点、非法状态和尚未送达的伪造完成。

Codex 从准确 thread/turn 的 `turn/completed` 读取 completed/failed/interrupted；Claude 的准确 prompt 接收 Stop/StopFailure，中断继续使用既有新增 transcript 证据。事件清空当前回合前，复用本次运行的 BackgroundApprovalAudit 有界线程写入 `mobile_turn_outcomes`。正式 App 使用共享 AppDB 和显式 workspace scope，审批仍在原项目库，SQLite commit 后才给内部回执；独立Runtime测试可使用各自项目库。没有新增执行器或常驻服务。重复同结果幂等，冲突不能覆盖，读取完整身份不匹配返回无结果。

SwiftUI 原生列表显示“本轮执行已完成 · 待验收”、失败或停止，不挤占原指令；沿用语义字体、完整滚动和系统操作。旧卡片更换 run 后仍可读取保存的历史结果；没有观察到终态时只显示真实送达状态。Provider完成不等于软件成果通过验收，关窗未观察到的中止不伪造成已完成。

验证：Storage83、Runtime143/3忽略、移动网络14/3忽略、Swift25、DeviceTests27/2跳过和导航4项通过。包含红测后实现、SQLite幂等/冲突/项目隔离、共享AppDB不跑项目migration、两种Provider准确终态、Claude中断落盘、跨手机读取和旧run保留。macOS首次Codex通过而Claude返回invalidResponse，合计43.10秒；增加阶段日志后Claude6.23秒通过，未做行为修复，首次原因未确定。iOS两种真实Provider均通过，51.89秒，两个不同turn为completed且拒绝项为null。固定计划fixture不证明Norma云模型规划质量；外部Hook保留，无工具任务，不读写用户文件。

全仓App794通过/7忽略，E2E204通过/5失败/25忽略，仍未做HEAD基线；失败为两项Chrome授权、MiniMax手动语音和两项poisoned voice lock。Computer Use再次返回Mac锁定；导航XCTest、headless GPUI和真实Provider协议验证不替代页面交互、物理双Mac/iPhone/Tailscale或无障碍验收。

重跑沿用v30的 `todos_native` 命令，并运行 `cargo test -p storage -p runtime`、`cargo test -p e2e --test 'mobile_*'` 与 `swift test --package-path ios`。日志前缀 `/tmp/norma-ios-turns-`：`storage-red-v31.log`、`runtime-red-v31.log`、`network-red-v31.log`、`swift-red-v31.log` 为红测；`storage-regression-v31.log`、`runtime-final-v31.log`、`network-regression-v31.log`、`swift-v31.log`、`xcode-v31.log`、`provider-mac-first-v31.log`、`claude-diagnostic-v31.log`、`provider-ios-v31.log`、`workspace-v31.log` 保留实际测试结果。

v31构建收尾：App check **5.15秒**、Mac Release **2m13s**、模拟器Release与未签名iPhone SDK build-for-testing通过；行数/no-allow/存储/性能/指南/diff护栏通过，canvas/terminal/runtime依赖树无GPUI。未安装到物理iPhone。日志为`/tmp/norma-ios-turns-{app-check,mac-release,ios-release,device-build,dependencies}-v31.log`。


## 明确未发送后的重新安排（v32）

`decide_plan_todo` 新增可选 `retry_of:{device_id,operation_id}`；未携带时保持原决定语义和原规范化请求指纹。`project_plan.decisions[]` 新增 `retry_of` 和 `can_retry`。只有上一项明确未发送，且请求指向当前最新决定，才能创建新的接受尝试；当前控制权限、授权版本、窗口与Agent快照仍全部复核。只读不能重排，旧身份不能替换新决定，未知、已派发或已经送达均不可重排；任务失败也可能已有副作用，不冒充未发送。

原 `project_todo_decisions` 保持不可变；`project_todo_retries` 只追加新尝试，唯一previous设备/操作身份保证并发只有一方获得新执行权。同一个SQLite立即事务验证旧结果、冻结旧owner未领取队列、预留新命令/排队并保存关联；另一方若已领取则拒绝重排。旧回执继续按原操作查询；返回每项最新尝试，新结果沿用v31准确回合终态。没有第二套派发器或自动重试任务。

SwiftUI复用原List/Form/Sheet：上次未发送时显示重新安排，再次显示完整指令和当前执行目标；原操作与新操作分开保存。原确认已被另一手机替代时关闭旧Sheet，重新连接只查。成功/拒绝/撤回/明确未发送的当前本机查询可清理，旧尝试的本机记录保留核实后清理入口，清理不取消电脑任务。语义字体、黑色背景、44pt行操作与完整可滚动内容延续Apple Design。

验证：Storage85项通过，含并发两手机、准确旧身份、未知拒绝、旧owner尚未领取队列的事务取消和旧回执保留；移动网络14项通过/3忽略，真实WSS fixture覆盖撤回后另一手机重新安排、重复只回查、旧预览拒绝和仅一次派发。Swift26项、DeviceTests28项/2跳过、导航4项通过。两种真实Provider经iOS simulator验证已送达Todo重排被拒绝，39.19秒通过，仍使用固定计划且不调用工具；成功重排的实际Provider和真实UI操作尚未验收。Computer Use再次返回Mac锁定。

全仓App794通过/7忽略，E2E204通过/5失败/25忽略；两项Chrome授权、MiniMax手动语音和两项poisoned voice lock仍未修复，未做HEAD基线。v31首次Claude invalidResponse保留为未确定原因的问题，本轮通过不能证明修复。日志前缀 `/tmp/norma-ios-retry-`：`storage-red-v32.log`、`network-red-v32.log`、`swift-red-v32.log`记录聚焦红测；`storage-regression-final-v32.log`、`network-regression-v32.log`、`swift-v32.log`、`xcode-v32.log`、`provider-ios-v32.log`、`workspace-v32.log`保留最终测试结果。原生详细日志 `/tmp/norma-ios-todos-native-{Claude,Codex}-v32.log` 已保存；该次运行最初写到v31文件名，之后复制至v32并修正后续日志路径，无重跑模型。

v32构建收尾：App check **5.42秒**、Mac Release **2m15s**、模拟器Release与未签名iPhone SDK build-for-testing通过；行数/no-allow/存储/性能/指南/diff护栏通过。本轮未改依赖，域层无GPUI的v31检查保留。未安装到物理iPhone。日志为`/tmp/norma-ios-retry-{app-check,mac-release,ios-release,device-build}-v32.log`。


## 项目当前未提交变更（v33）

SwiftUI在项目列表与聊天页提供“项目变更”。它只展示本次读取的项目范围：HEAD原文、暂存区、工作副本、HEAD→工作副本和HEAD→暂存区diff。创建卡片/查看文件不派发任务、不提交代码。系统List/菜单与原生TextKit维持黑色界面、紧凑系统字体、完整可选正文、动态字体与44pt操作；二进制和冲突无唯一暂存版本时给出明确说明。

`project_review {device_id,token,request:{workspace_id,route_id}}`不接受任意文件路径或commit。普通回复`status:project_review,review:{request,review:{base_commit,files,conflicted}}`；大回复使用`review_fragment`复用已有64KiB UTF8分片codec，完整组装后才解码。每帧不超过500KiB、完整JSON不超过32MiB；原始每版本不超过1MiB，所有版本+diff合计8MiB、最多128文件，失败不会变成部分快照。

沿用现有App-owned MobileController与有界source，GPUI前台只解析实际workspace/route并克隆目录，Git/文件/diff在background_executor读取，完成后复验同一安装/窗口。网络层在读取前后、逐片和最后一片重查当前权限/窗口；只读可看，撤销后迟到的正文不可送达。项目内部`.git/.norma/.multisoul`排除；路径分量通过目录fd与NOFOLLOW打开，最终符号链接只返回文字，不读目标。Git读操作不使用外部diff/textconv，全局配置与继承GIT路由被隔离；存在本地clean/process filter或submodule时明确Unsupported。HEAD/变更名单前后核对、文件读取期间元数据核对，但不宣称整个工作树原子一致。

当前没有持久化交付快照、任务起点baseline或验收决定。已提交的成果不在此页面，不能用“没有未提交变更”推断任务通过；多Agent共用目录，也不能从diff断言作者。图片预览、不可变成果与验收回原任务是后续G4工作。

验证：`cargo test -p workspace`27项（23原有+4成果域层）；正式GPUI/WSS目录读取与授权撤销/大响应分片通过；Swift27项；原生TextKit检查完整100000字节diff、只读/可选/动态字体与正文视口通过。600000字节中文文件经正式App/GPUI/Runtime目录与macOS Foundation、iOS simulator读取，无Provider任务或工具调用；这不是实际iPhone页面验收。Mac仍锁定，Computer Use未完成。

初次全仓新增测试遇到测试socket3s先于服务捕获4s结束；调齐后仍记录过`review_unavailable`。预算按128文件×最多6个短进程重新设为10s捕获，首帧18s（2s×2窗口核验+10s捕获+4s传输/调度），后续5s。最新全仓App794通过/7忽略，E2E205通过/5失败/26忽略，5项为Chrome授权/语音，未做HEAD基线。保留`review-workspace{-final,-diagnostic}-v33.log`中的失败；最新日志`/tmp/norma-ios-review-workspace-bounded-v33.log`，不能把一次后续通过等同于完成性能验收。

日志统一前缀`/tmp/norma-ios-review-`；完整列表和最终构建结果见[evidence JSON](2026-09-07-norma-ios-connection-evidence.json)的`project_review_v33`。产品功能仍按[Goal实时计划](../exec-plans/2026-09-07-norma-ios-goal.md)推进。

v33最终验证：移动网络15通过/3忽略，Swift27通过，DeviceTests30通过/2跳过，原生导航4通过。最新真实iOS预览检查10.18秒通过；App check14.06秒、Mac Release2m15s、模拟器Release和未签名iPhone SDK build-for-testing通过。文件行数、no-allow、存储/性能/指南及diff护栏通过，canvas/terminal/runtime/workspace依赖无GPUI。完整Goal未完成，未安装到物理iPhone；Computer Use与成果验收等缺口按上述记录保留。

## 任务起点与准确发送回执（v34）

SwiftUI继续使用已有原生发送与回查协议。Mac在所有实际SendMessage路径（含Todo转发送、队列派发）后台捕获起点，随后仍由原入口核验实际窗口/卡片并取得可撤销执行权。明确未发送的重排或队列defer不保存起点；后续尝试重新捕获。等待Provider回执使用非阻塞轮询，SQLite保存留在后台，不阻塞GPUI执行器。

`MobileTaskBaselineStore`以workspace/run/turn唯一保存完整key、原device/operation/route及TaskBaseline。数据源必须匹配已预留命令；相同内容幂等，不同正文/任务不可覆盖。起点捕获失败记录Unavailable原因；起点写库失败不抹去真实发送回执。保存时间不等于捕获瞬间，捕获→发送→落库存在进程崩溃窗口，缺失时不能用后来的文件补造。每次发送40s：10s捕获+25sProvider确认+5s存储；其余命令仍30s。原生验证客户端查询最多45s，不重复提交。

`workspace::review::capture_since`保留起点时已有的工作副本与index，同时使用起点commit到当前commit的路径差集，覆盖已提交的新增/删除/修改；首次commit和授权子目录均有聚焦证据。过滤内部状态、限制内容与路径的规则沿用v33；新增拒绝assume-unchanged/skip-worktree，防止Git状态遗漏真实修改。仍是项目范围，不能判断共享目录中是哪位Agent修改；也不是整个目录的原子快照。

重跑真实发送检查（只在命令进程补PATH，不改变全局Hook配置）：

```sh
cargo build -p norma --bin multisoul-agent-hook
swift build --package-path ios --product PairingProbe
PATH="$PWD/target/debug:$PATH" cargo test -p e2e --lib e2e_mobile_real_task_baselines_bind_exact_sends -- --ignored --nocapture
PATH="$PWD/target/debug:$PATH" NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" cargo test -p e2e --lib _messages_native -- --ignored --nocapture --test-threads=1
PATH="$PWD/target/debug:$PATH" NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" cargo test -p e2e --lib _messages_native -- --ignored --nocapture --test-threads=1
```

真实Codex直发/Todo/队列的起点检查21.01秒通过；两种实际Provider的原生macOS30.57秒/iOS simulator40.61秒通过，并按持久化发送回执核验起点。完成后的卡片可能没有活动turn，不能用它代替原任务身份。本轮首次Claude失败的实际原因是测试PATH缺少审批Hook；修正环境后通过，保留失败日志，不将该原因套用到v31。

Workspace31、Storage87、Swift27及DeviceTests30通过/2跳过。全仓App795通过/7忽略，E2E205通过/5失败/27忽略，五项仍在Chrome授权/语音，无未修改HEAD基线。本轮没有页面改版；沿用v33的Computer Use锁屏限制，原生网络验证不代替真实界面或物理双Mac/iPhone/Tailscale验收。

成果接口、交付指纹/快照、验收回执及反馈回原任务尚待实现；手机“项目变更”仍为v33未提交预览。详细构建和失败→通过日志见[evidence JSON](2026-09-07-norma-ios-connection-evidence.json)的`task_baseline_v34`。

v34构建收尾：Mac Release2m10s、App check3.80秒、模拟器Release和未签名iPhone SDK build-for-testing通过；移动网络/持久化15通过/3忽略。最终Codex直发/Todo/队列起点检查21.01秒通过。依赖与机械护栏通过，未提交代码、未安装到物理iPhone；完整Goal继续active。

## 自动冻结交付与原生成果页（v35）

`mobile_task_baseline`在确认并保存准确发送身份后启动弱窗口watch；运行中仅查内存，终态后等待最多2s审计交接，后台按既有10s预算捕获。Runtime保留最后真实turn，捕获前后核验同一回合、无待提交消息/桌面Enter、同一窗口；旧回合不能借新Done复活。App适配层以collecting表示整理，队列自然等待，原生终端仍在。

`MobileTaskDeliveryStore`只接受已有持久化终态，固定正文/失败原因与内容指纹；唯一(workspace,run,turn)，不同正文拒绝覆盖。显式自增sequence用于每卡20条游标摘要，完整正文按需读取。WSS的task_deliveries返回摘要，project_review的delivery_key读取已冻结正文；权限和窗口在读取前后及末片复验，网络不接受文件路径。存储校验不是全工作树事务，保存失败/进程崩溃不保证补收，缺证据不能冒充成功。

SwiftUI新增TaskDeliveriesView，从聊天菜单进入；复用ProjectReviewView/ReviewFileView和原生TextKit，显示“任务起点→本次交付”，与当前未提交预览区别。600000字节旧正文与后续600000字节工作树不同，原生客户端验证冻结正文无损。该大文件用例使用明确的终态fixture；真实Provider链路另由消息和Codex直发/Todo/队列测试验证，不能混称模型真实执行。

Swift29项、DeviceTests32通过/2跳过；SwiftUI首次菜单类型推断失败已修复，模拟器Release与未签名iPhone SDK构建通过。真实Claude未取得完成证据，最新终端仍Generating(45s)，不推断交付成功；各次失败和最终测试/构建结果见连接JSON的task_delivery_v35。Computer Use因锁屏未完成；验收回执/反馈、图片、通知和G5继续保留。

v35构建收尾：Mac Release2m21s、模拟器Release和未签名iPhone SDK build-for-testing通过；Runtime144通过/3忽略、Storage88通过、Swift29通过、DeviceTests32通过/2跳过。行数/no-allow/存储/性能/指南/diff与域层无GPUI护栏通过。未提交代码，未安装到物理iPhone；完整Goal保持active。

最后网络回归：15通过/3忽略；App check3.99秒通过，日志见v35连接证据。


## 原生 SwiftUI 成果验收（v36）

accept_delivery 通过既有 mobile_command 请求提交准确 key 和 fingerprint。网络复用设备授权/revision、workspace/route、窗口owner、执行租约和幂等预留；App仅允许卡片当前无正在执行任务时验收。后台验收事务同时写 mobile_delivery_acceptances 与原命令 succeeded 回执，数据库唯一约束阻止另一手机覆盖。相同原请求回查成功；错指纹、旧route、无控制权、preparing、失败、不可用或冲突成果拒绝。已删除空卡的历史完整成果仍可验收，既有决定不会转给新回合。

project_review 的固定 delivery 元数据返回 acceptance。Swift核验保存时间、任务身份、指纹与workspace，历史旧route不会被当前重开route误判为新成果。ProjectReviewView复用CardCommandSheet：确认前预览电脑/项目/成果，提交前持久化原命令，未知结果只回查；详情显示已验收时间。系统List/Sheet/按钮、44pt操作区、黑色语义字号和完整正文落实Apple Design。列表级验收标签与修改反馈尚未接入。

Storage89、Swift30、DeviceTests33通过/2跳过、原生导航4通过。正式GPUI/WSS0.66秒、原生macOS2.50秒/iOS simulator52.31秒通过；原生用例读取600000字节不可变旧正文，保存命令、单次提交、从磁盘恢复原操作回查并再次核对验收和正文。用例使用明确completed fixture，没有实际模型执行。本轮未重跑真实Claude，v35完成超时/未知回执/持续Generating三次证据仍未解决。

红测与修复保留：新增命令未接入时Swift/E2E失败；原生fixture授予control后Probe仍要求read，已同步；空卡测试穷尽match显式拒绝不支持的验收，未伪造成功；冲突fixture最初conflicted与空文件不一致，被生产校验拒绝，补准确UU文件后通过。新增事务故障注入和两写线程Barrier竞争验证通过，未放松生产校验。

全仓停于E2E（205通过/5失败/27忽略），App795通过/7忽略；失败为两项Chrome授权、MiniMax手动voice及两项poisoned voice lock，没有未修改HEAD基线。精确日志与构建结果见[evidence JSON](2026-09-07-norma-ios-connection-evidence.json)的delivery_acceptance_v36。

Mac已解锁，Computer Use读取到了隔离Norma首页及“连接手机”入口；Simulator读取超时，桌面点击仍报窗口状态已改变，工具重连后亦未通过真实操作。没有开启实际手机连接或创建配对凭据。以下截图仅证明真实窗口可读，不证明配对或手机页面验收：

![解锁后的隔离Norma首页](/Users/alan/Documents/mybook/norma-ai-os/artifacts/norma-ios-v36/mac-launchpad-unlocked.png)

当前隔离App可能仍运行；Computer Use关闭也曾报告状态改变，未确认退出。物理iPhone/双Mac/Tailscale、无障碍、图片和APNs仍待完成，Goal保持active。

v36构建收尾：Mac Release4m21s、模拟器Debug/Release、未签名iPhone SDK build-for-testing和App check通过；行数/no-allow/存储/性能/指南/diff护栏通过。未提交代码，真实页面与物理设备验收仍未完成。


## 修改意见与新任务回执（v37）

feedback_delivery复用mobile_command，携带expected与feedback:{key,fingerprint,text}。边界要求旧成果node/profile/run/session与目标一致，原意见最多30KiB（32KiB发送上限减2KiB有界来源上下文），不trim。共享发送队列检查、40s预算、授权revision/route/owner/lease保持适用；不冷启动替代会话。

SQLite mobile_delivery_feedback先按原device/operation保存意见，核验已有可读交付的完整key和fingerprint。同操作原文不可覆盖；旧成果可已验收、可含冲突，但反馈不会改变验收。后台起点捕获结束后，原FeedbackDelivery仍到达正式send_mobile_chat入口核验实际快照，再发原会话。原发送回执finish在同一事务内核验node/run/session和不同于旧成果的合法新turn，并附加feedback:{key,fingerprint}；正文不塞进身份回执。来源持久化失败时不发Agent，发送结果未知时只查原操作。

SwiftUI成果详情可提出修改；复用CardCommandSheet与ChatDraftBook。草稿Scope增加可选feedbackFingerprint，旧草稿兼容且与普通聊天隔离；提交前flush，只有对应版本的成功回执才清除，后写草稿保留。原始意见可从未决命令查看。使用系统TextEditor/Form/Sheet、语义字号、44pt按钮和原生滚动，继续黑色风格。

Storage90与Swift31通过，包含原文保存/错指纹/跨项目读取拒绝、不可覆盖、错session/旧turn回执拒绝、相同回执幂等及反馈/聊天草稿隔离；正式GPUI/WSS旧反馈在准备态经过后台捕获后被拒绝，0.85秒通过。两种实际Provider的macOS Swift反馈链路37.85秒通过，iOS Codex通过、Claude43.83秒通过。原生客户端核对两次真实消息、首轮自动冻结成果、带引用的完整意见、不同turn、原草稿恢复/清除、旧成果不变与关闭后拒绝。它们不是物理设备或Computer Use。

保留失败：父仓库本地LFS配置令初始隔离子目录成果明确不可用；测试改成自有Git元数据后通过，未关闭生产边界或修改用户配置。iOS首轮Claude测试模式未登记导致跳过，外层必须检查真实marker所以判失败；补白名单后真正执行通过。SwiftUI Section带footer的构造方式及Probe错误使用非Equatable文件数组比较也已修正。v35历史Claude问题不凭本轮通过宣称修复。

全仓App795通过/7忽略、E2E205通过/5失败/29忽略，具体失败与v36一致，无HEAD基线。精确日志和构建见[evidence JSON](2026-09-07-norma-ios-connection-evidence.json)的delivery_feedback_v37；图片/APNs/物理网络/无障碍及真实点击验收继续按Goal推进。

v37构建收尾：Mac Release、模拟器Debug/Release、未签名iPhone SDK构建与App check通过；DeviceTests34通过/2跳过、原生导航4通过，网络回归8通过。Mac已解锁，用户打开Norma后Computer Use仍报Simulator timeoutReached；Mac首页可读但连接按钮未产生页面变化，后续点击报user-changed，实际交互未验收。截图见artifacts/norma-ios-v37/mac-computer-use.png。

## 图片预览（v38）

协议不变，手机从已校验的ReviewFileVersion.bytes解码栅格图片。ImageIO在后台先检查类型、完整性、尺寸，再生成带方向变换的2048px缩略图；不访问文件路径、网络或符号链接目标。单版本仍限制1MiB，源图限制4096²像素总量。GIF等多帧格式只显示首帧；SVG/PDF等主动或非栅格内容不渲染。SwiftUI文件页默认打开新增/删除图片存在的一侧，以原生UIScrollView处理缩放，保留VoiceOver自定义操作与减少动态效果。

新增原生宿主检查覆盖正常PNG、超限/损坏/符号链接拒绝、版本切换与图片视口；既有WSS回归新增真实PNG完整字节/二进制标记/无假diff断言。DeviceTests36通过/2跳过、导航4通过，实际Computer Use仍未验收。完整日志见image_preview_v38。

v38构建收尾：模拟器Release、未签名iPhone SDK构建与常规护栏通过。完整E2E199通过/11失败/29忽略（语音Apply、两项Chrome授权与8项语音锁poisoned），无HEAD基线；未完成Computer Use验收，未提交代码。


## 历史列表验收状态（v39）

历史列表单次LEFT JOIN读取真实验收，校验workspace、完整key、fingerprint和合法终态；损坏记录明确失败。Swift兼容旧响应缺字段，但不推定已验收；详情回传仅更新同身份列表项，保留分页位置。原生macOS/iOS WSS验证验收前后列表与回执一致，均输出NATIVE_ACCEPTANCE_HISTORY_OK。

Storage90、Swift31、DeviceTests36通过/2跳过、导航4通过；Mac/模拟器Release和未签名iPhone SDK构建通过。两项Chrome E2E因测试推进12ms小于生产16ms而失败，改用已有runtime_drain_step后2项通过。MiniMax切换仍未解决，临时等待探针已移除；未宣称完整E2E通过。精确日志见history_acceptance_v39。


## 不用相机的配对（v40）

在Mac手机连接页点击「复制连接码」，在iOS添加电脑页选择「输入连接码」并粘贴；读取后展示电脑身份，连接与桌面批准仍沿用扫码流程。格式norma-pair-v1:加URL-safe无填充Base64，内容是原配对JSON；不使用公网注册中继，不把六位核对码当作认证密钥。两种入口共享120秒与单次认领约束，禁止日志记录码。

扫码页支持系统照片选择器，只读取选中图片；后台Core Image解码，20MiB编码与8192²源像素上限、4096px缩略图，拒绝多码和无二维码。用户仍需确认电脑身份。

桌面复制/到期/取消测试1项、Settings E2E2项、Swift31项、iOS聚焦原生2项及导航1项通过；Mac/模拟器Release和未签名iPhone SDK构建通过。真实配对尚未完成：Computer Use曾识别电脑，但二维码过期；内容区点击持续noWindowsAvailable，当时的长码协助请求已由 v41 替代。此次更新保留隔离桌面数据库，未声明真机或Tailscale验收。


## 六位验证码配对（v41）

用户已改为数字验证码，v40 的长连接码 UI/解码入口已移除。Mac 显示、复制或刷新 6 位 ASCII 数字；手机首次填写 LAN/Tailscale 地址（默认 9443）和验证码，保留前导零。输入页使用黑色原生 Form、数字键盘、oneTimeCode、进度与取消反馈，验证成功后记住地址。二维码和图片扫码继续可用。

验证码不作为设备 token，也不在网络发送明文。`mobile_pairing` 通过 RustCrypto SPAKE2 0.4.0 交换一次性公共消息，使用派生密钥和随机 nonce 的 AES-256-GCM 加密原有 QR 身份/凭据；Swift 只在解密并验证原协议后进入严格证书 pin 的正式配对。有效期 120 秒，所有接口/连接合计最多 3 次交换，取消、刷新、到期或 QR 认领均关闭旧入口；Mac 仍需批准具体项目权限。两端共用小型 Rust 库，生成的 XCFramework 不包含 GPUI/runtime，支持 macOS 与 iOS 模拟器 arm64/x86_64、iPhone arm64。

验证：Rust Norma/配对库 117 通过、1 忽略；Swift 32 通过；原生 UI/输入校验 2 通过；真实 macOS 与 iOS WSS 均完成错误码拒绝、第三次成功/第四次锁定、正确码解密、Mac 只读批准、项目读取及撤销。QR 网络/持久化回归 4 通过，桌面复制生命周期 1 通过。小窗口弹窗先复现末行超出窗口（y=786.5px），复用组件内置滚动并限制高度后 Settings E2E 3 通过。

完整 E2E 为 207 通过、3 失败、29 忽略：MiniMax 手动语音选择断言仍失败，其余两项为该锁 poisoned；未声明完整 E2E 或完整 Goal 通过。Computer Use 已实际开启新版桌面服务并查看验证码，也打开了 iOS 配对向导；模拟器内容点击仍间歇性 noWindowsAvailable。当前尚未完成用户实际桌面注册，已请求停留在验证码输入页；没有物理 iPhone、双 Mac 或真实 Tailscale 验收。Goal 保持 active。

v41构建收尾：Mac Release（2m28s）、模拟器双架构Release、未签名iPhone SDK build-for-testing、App check与配对库Clippy通过。最终版本已更新隔离桌面App与iPhone 17 Pro模拟器，保留已有数据。

## 实际配对与后续验收（v42）

用户确认实际配对成功。普通VPN点对点地址被误推荐的问题已修复，保留真实LAN及Tailscale；验证码页超时不再显示任务操作回执文案。新构建保留在产物目录，当前已连接实例未重启。

双独立WSS/SQLite服务验证同名项目与完全相同局部ID的隔离，覆盖A撤销/关闭不影响B、准确决定和重复回执；网络3项通过。APNs服务已在[独立目录](../../services/push/README.md)通过本地HTTP/2与ES256检查，但Mac/iOS注册与事件投递尚未接入，未部署或验证真实锁屏。用户已有开发者账号。

App check、两端Release与地址回归通过；完整E2E仍有MiniMax语音及2项poisoned锁失败。Computer Use桌面可读取，Simulator内容验收仍受工具错误影响。详细证据见JSON的pairing_followup_v42和push_relay_v42。

## 通知订阅协议（v43）

Mac新增受现有WSS认证保护的push_subscription/set_push_subscription，SwiftCore同步接入。每个手机独立保存token、环境、Mac注册毫秒时间和单调版本；显式null才代表关闭。版本不匹配返回push_subscription_changed，不能重放旧更新。关闭不撤销项目授权，撤销授权则在同一事务中移除token；迟到APNs失效必须匹配注册版本和时间。

Storage91、Swift33与网络回归8项通过；原生macOS/独立iOS simulator host完成注册、旧版本拒绝、关闭、重开和撤销。生产Apple设备token回调、通知开关、真实事件投递及通知点击回源尚未接入；没有真实APNs或锁屏证据。用户已连接实例未重装，完整Goal保持active。协议与配置见[通知服务说明](../../services/push/README.md)。


## v44 · 通知投递与原生系统接入

订阅回执加入真实machine_id/device_id。Swift更新前核对所见电脑，并与本次实际加载的Keychain设备身份比较；网络响应也必须对应这两个身份。回归先复现了A状态误用于B会走到凭据读取，再修复为联网前拒绝；原生WSS检查包括另一手机状态拒绝且原订阅不变。

Mac复用当前MobileServer生命周期和已有实时源，从已持久化的MobileTurnOutcome按项目/rowid分页读取完成/失败。配置私有HTTPS relay后才启动后台消费者；授权、订阅版本、项目/窗口/会话和审批在投递前重新验证。无正文出站，未知结果不重发，410不会误撤销设备授权。详细容量和生命周期边界见[服务说明](../../services/push/README.md#mac与ios配置v44)。

SwiftUI新增通知设置、系统授权/APNs回调、按电脑订阅和token续登记、通知定位页与指挥台定向卡片。显式Info.plist与entitlement共用APS_ENVIRONMENT，修复生成Info.plist遗漏自定义环境键导致回调被拒绝的问题。Apple Design落实为黑色系统List/Sheet、语义字号、44pt操作区、原生按钮状态和可见的加载/失败反馈。

Computer Use本轮恢复可用，实际打开独立「Norma Acceptance iPhone」的Norma、任务提醒空态；不等同已配对设置或通知中心验收。系统成功/失败回调用隔离token注入测试，没有请求Apple推送。待补：开发者Team/Push capability、APNs密钥与relay部署、已配对开关/通知点击、物理iPhone锁屏和双Mac/Tailscale。用户已连接的v41实例未重启或重装。

签名账号已按桌面Packager.toml沿用公司Team `2BF8G9AN4L`。原通配描述文件缺少推送能力；Xcode自动更新已成功生成专用描述文件与已签名Debug构建，签名校验及实际aps-environment=development检查通过。没有进行App Store/TestFlight上传。当前登记的物理iPhone 11不可用，锁屏验收仍待连接设备和APNs服务配置。


## v45 · Mac 任务提醒服务配置

Mac「手机连接 → 任务提醒 → 配置服务」已支持HTTPS地址、遮蔽relay凭据和Sandbox/production选择。配置在后台写入同一个安装隔离的Keychain项；停用清掉凭据并保留显式关闭标记。运行中替换只切换通知发送端，通过代次检查丢弃旧读取，保留已尝试审批的去重；不重启手机连接或Agent。保存不探测APNs，不表示手机已收到通知。

Computer Use已在独立真实Mac App完成填写、拒绝HTTP、保存、关闭重开与清除。此验收使用e2e内存凭据存储，不代表正式Keychain跨进程恢复。配置2项、Settings E2E3项与本地网络检查通过；完整E2E为202通过/9失败/29忽略，语音激活断言与8项共享锁失败仍未解决。原用户已配对实例保留，服务部署与物理锁屏验证未完成。证据见连接JSON `push_configuration_v45`。


## v46 · iOS 系统通知与冷启动回源

独立模拟器已取得真实系统APNs token，经原生UI完成已配对电脑的提醒开启/关闭；真实WSS/Keychain状态测试覆盖续token、停用不复活、撤销与清理。`simctl push`本地注入通知后，通过SpringBoard点击进行冷启动，重现并修复异步通知代理完成时的UIKit主线程崩溃：现在显式completion-handler在MainActor完成。当前窗口通知正确回源；旧窗口通知明确拒绝且不打开新任务。真实Provider/Apple投递不属于这组fixture证据。

原生验证、Debug签名iphoneos和Release simulator构建通过，测试配对已清理，原用户桌面保持运行。Computer Use仍无法读取Simulator窗口；XCTest操作与截图只是补充，完整Computer Use、物理iPhone锁屏和双Mac/Tailscale仍待验收。完整Rust E2E208通过、3失败、29忽略，未宣称全仓通过。详见连接evidence JSON `notification_acceptance_v46`。


## v47 · 大字号卡片与聊天验收

复用现有独立只读WSS fixture，通过原生XCTest分别按系统默认Large和最大辅助字号AX5导航项目/卡片/聊天。系统审计真实发现项目与聊天顶部电脑名称截断；键盘展开并输入五行后，对话滚动区域仅剩28pt。修复后辅助字号采用纵向电脑信息和完整卡片标题，输入框最多显示两行并原生滚动，五行原文完整保留；默认仍显示最多五行。权限提示保留，未开放只读发送。Apple Design落实为语义字号驱动布局、内容完整与原生文本滚动，不缩小用户选择的字号。

两项原生UI测试通过，覆盖两页的textClipped/hitRegion/sufficientElementDescription审计、五行输入/清空和只读拒绝；AX5正文滚动区域117pt，默认260.3pt。正式忘记流程清理通过，两次隔离fixture均正常退出并撤销授权。Debug已签名iphoneos与Release simulator构建、签名校验和代码护栏通过。完整Rust E2E为202通过/9失败/29忽略：本地语音显式Apply断言和8项共享锁poisoned；没有未修改HEAD基线，不宣称全仓通过。

Computer Use读取Simulator再次timeoutReached，不能将系统审计/XCTest截图等同真实Computer Use或VoiceOver操作；本组fixture没有真实Agent对话正文，不重复声称发送链路已验收。减少动态效果、真实语音输入、物理双Mac/Tailscale、APNs投递和锁屏仍待完成。用户已配对的桌面PID21316未重启，其模拟器未覆盖。详见连接JSON `accessibility_acceptance_v47`；Goal保持active。


## v48 · 桌面回归验收

修复MiniMax目录零宽点击区；活跃TTS选择归属各自App，创建另一实例不会清空原音色。GPUI选择器检查现随帧清理并正确重放缓存，旧模型升级与Setup返回的正负断言通过。完整E2E230通过/0失败/34忽略（headless212、集成18）；语音38项与App check通过。Mac Release通过；完整workspace1737通过/0失败/62忽略。具体红绿日志与验收边界见evidence JSON `desktop_regression_v48`。

独立voice-catalog真实App的Computer Use验收未完成：坐标操作失败后Mac再次锁屏。测试实例已关闭，用户现有配对实例保持运行。正式Keychain跨进程恢复、APNs部署/真实投递、物理双Mac/Tailscale和完整G5验收仍待完成。


## v49 · 正式Keychain与规划质量边界

推送配置已通过独立正式Keychain worker的跨进程恢复、替换、停用、损坏拒绝与安装隔离；共11个进程步骤，1项L3检查通过，合成测试项已删除。没有启动GPUI、模型或APNs网络；正式App升级签名ACL不在本次范围。重跑命令见[推送说明](../../services/push/README.md#正式keychain跨进程验证v49)。

Ollama规划复用Rig原生JSON schema，移除提示词中的固定任务示例；不改变OpenAI协议、20秒超时或执行授权。真实0.8B模型仍有超时/格式/方向错误；1.5B样例结构与指定卡片通过，但实际内容虚构进度、偏离视觉方向且缺少可观察验收条件，规划质量未通过。云模型尚因Keychain交互未调用；不会自动切换用户模型配置。详细原文与范围见 `keychain_and_planning_v49`。

当前Mac锁屏，物理iPhone通道不可用且Developer Mode关闭，Tailscale已停止。正式推送凭据恢复的旧缺口已关闭；APNs部署/实际投递、物理网络、真实云规划、完整页面/无障碍验收继续保留；重排的最新证据见下文。


v49重排补充：初始card_changed拒绝回执由隔离持久化fixture提供；原生客户端明确引用此前决定重排，之后走正式WSS、GPUI/Runtime和实际Provider。Codex/Claude在macOS两项54.87秒、iOS模拟器两项77.26秒均通过；准确原文仅各发送一次，拒绝项缺席，重排关联正确且原失败回执没有被成功覆盖。原始失败并非真实界面触发，这组测试也不代表Computer Use或物理iPhone验收。

用户删除/tmp后，已按授权恢复/private/tmp为root:wheel、01777，并仅重启独立验收模拟器重建testmanagerd socket。最终全workspace1738通过/0失败/64忽略，App check和Mac Release通过；Swift35项、正式Keychain复验1项通过。旧临时日志无法恢复；新证据保存在target/norma-ios-evidence/v49，实际模型输出此前已嵌入证据JSON。用户配对桌面未重启。


## 通知呈现与目标更新（v50）

通知页现使用当前scene的原生呈现链，显示在设置、嵌套输入等已有弹窗之上；关闭返回原上下文。SwiftUI宿主显式继承scenePhase，并按完整通知目标重建，避免相同事件ID携带不同目标时沿用旧任务。入口仍回源核验机器、项目、窗口和准确回合；不执行通知中的控制动作。

5条原生通知路径通过（51.026秒），包括前台目标更新、输入保留、冷启动与旧窗口拒绝。Computer Use已观察设置开关和模拟器锁屏点击后的正确目标页；后续工具连接中断，最终目标更新与返回输入仅有XCTest证据。simctl注入不代表APNs或物理锁屏通过。过期fixture仅按完整元数据清理自身配对，后续fixture均正式忘记并撤销；详见`notification_presentation_v50`和实时Goal。

v50最终验证：Swift35通过、DeviceTests42通过/4显式跳过、原生导航5通过、通知5条路径通过；模拟器Release与未签名iPhone SDK构建通过。新日志在`target/norma-ios-evidence/v50/`；iPhone SDK编译不代表物理设备安装或APNs投递。

## 指挥台电脑级审批订阅（v58）

`watch_approvals` action携带当前 `device_id/token`，沿用version1及已配对机器的TLS身份。每台电脑仅建立一条连接，汇总该手机全部授权项目，不为每个项目新开连接。回复 `status=approval_event`，`approval_event={stream_id,sequence,snapshot}`；完整snapshot为 `{grant,projects:[{project,items,error}]}`，project是无cards的现有项目目录项，items沿用原审批详情结构。

每次新连接sequence0必须包含完整基线，变化时递增并发送完整新状态；无变化心跳保留序号且snapshot为null，不复制工具输入。复用既有条件变量变更信号和50ms突发合并，空闲每2秒重新鉴权。目录、全部项目审批、最终目录核验共用4秒读取预算；完整回复最多500KiB，超限明确报错，不截断。每个项目仍最多128项，Swift仍逐项验证原始详情和身份。

前后检查授权与workspace/state/route，读取期间项目关闭不能回传旧输入；名称/空卡数量变化不会使同一路由失效，采用最终核验的当前元数据。单项目暂时失败返回该项目error和空items，其余项目继续显示；整体授权/路由失效则清除整份状态。新订阅在access→sockets锁顺序下标记手机，撤销不会漏掉刚建立的连接。

SwiftUI `CommandCenterView`已直接消费 `MobileClient.approvalUpdates`，取消3秒轮询。每台电脑独立任务；序号缺口、换流、授权版本变化或断线会清空旧内容，最多1/2/4秒只读退避重连。缓冲只合并已经校验的完整状态，恢复通知覆盖旧缓冲；进入后台/离开页面取消消费。决定仍使用原持久化命令与准确请求身份，不自动重放；外部Hook信息项无批准按钮。保留黑色、语义字号、原生分页/滚动和44pt按钮，Apple Design落实在即时状态反馈与内容空间，未重新设计布局。

真实Provider审批在macOS/iOS共4项通过；并行项目/审批订阅的降权、重建基线与取消在两个原生宿主各通过1项；正常外部Hook的iOS等待提示/停止通过。服务端WSS/SQLite fixture覆盖授权项目隔离、心跳不重复输入、卡片数量变化、审批清空、路由读取竞争和撤销。单独本机采样65.329ms低于150ms门槛，含50ms合并和正式读取/传输，不能作为物理LAN/Tailscale或SwiftUI响应数据。

重跑命令见[iOS README](../../ios/README.md#指挥台实时审批v58)。最终回归、失败修复过程和原生日志见连接证据 `approval_feed_v58` 及实时Goal；本轮未重启用户已配对的旧桌面、未发布TestFlight，Computer Use/物理设备/APNs按用户要求后置。


## 无关变更下的心跳与首分片采样（v59）

全局变更信号可能由其他项目或设备触发。此前项目和聊天订阅在每次唤醒后即使投影没有变化也发送心跳；新WSS回归在30次无关唤醒中读到每流30帧。现复用审批订阅的发送判断：有实际内容立即发送，无变化只在距上次发送达到2秒时发送心跳。共享等待按上次发送时间计算剩余预算，避免无关变更停止后延后保活。序号不变，读取和发送前授权检查保留；聊天不再为不会发送的心跳额外做最终快照核验。

同一约3.36s的隔离场景，项目流由11760降至392字节，聊天流由9990降至333字节，合计21750→725字节（96.7%减少）；两条流仍各收到1个无正文心跳，撤销后正常结束。这是完整UTF-8应用消息字节，排除基线、TLS/TCP与握手，不是设备总流量或电量测量。全局信号仍会唤醒后台读取，尚不能据此宣称服务端CPU同幅下降。

既有大响应WSS测试新增单独的20次采样入口，复用相同600000字节中文和精确工具输出fixture。每次新建连接，分别记录TCP/TLS/WSS连接完成、发送开始至首个完整WSS分片到达（JSON解析前）、全部分片接收/重组/原文校验结束；每次保留全文与工具的准确断言，最后仍跑换run中断尾片和撤销后拒绝。首分片p50/p95为24.04/45.36ms，整页为71.28/139.79ms；每页43帧、4809022字节应用JSON。模型生成、SwiftUI呈现和物理网络不在该fixture内，不能将首分片当作模型首token或TCP首字节。

```sh
cargo test -p e2e --test mobile_conversation_events unrelated_changes_do_not_flood_project_or_chat_watches -- --nocapture --test-threads=1
# 以下测量须与其他构建/测试串行。
cargo test -p e2e --test mobile_conversation_transfer conversation_transfer_first_fragment_measurement -- --ignored --nocapture --test-threads=1
```

原生与完整回归、构建状态和全部原始样本见实时Goal及连接证据 `idle_watch_and_fragment_v59`。本轮手机源码和协议字段未改变，继续使用v58原生客户端；物理设备、真实APNs和完整Computer Use仍按用户安排后置。

## 原生首分片、服务线程CPU与TCP字节（v60）

仅隔离probe在Debug下访问正式PinnedConnection与分片解码，20次完整读取分别记录Keychain/准备、TLS/WSS-ready、首完整分片与全文校验。每次43帧、4810140字节应用JSON，准确中文和工具输出、旧run拒绝仍必须通过。未向App添加观测API、计时开关或生产依赖。

```bash
python3 scripts/measure-norma-mobile-idle.py --self-test
swift build --package-path ios --product PairingProbe
NORMA_MOBILE_MEASURE=1 NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" \
  cargo test -p e2e --test mobile_conversation_transfer native_large_conversation_transport -- --ignored --nocapture --test-threads=1
xcodegen generate --spec ios/project.yml
NORMA_MOBILE_MEASURE=1 NORMA_IOS_SIMULATOR=5265591D-704C-4515-868D-C2D64F94A7FA \
  NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" \
  cargo test -p e2e --test mobile_conversation_transfer native_large_conversation_transport -- --ignored --nocapture --test-threads=1
```

逐条执行，不与其他构建/测试并行。大页基线后独立Task持续消费公共conversationUpdates，测试owner按ready/done标记采样自己的PID。libproc仅统计准确命名的listener/peer线程，nettop记录两次间隔6s的TCP累计字节；脚本拒绝错误线程集合、缺失样本或计数回退，并保留原始CSV。CPU窗口含nettop启动/退出，不能与网络窗口混算吞吐；不直接监控用户桌面实例。

最终iOS模拟器首分片p95=25.14ms、全文校验126.43ms；macOS分别45.67/191.47ms。两平台空闲约6s各收到3个后续更新、TCP发送1158字节，服务网络线程总单核占比约0.52%。首分片是完整Data而非TCP首字节或模型首token；ready阶段含Ping/Pong。字节不含完整TCP/IP开销，CPU不含Provider/GPUI/fixture线程，也不是完整产品或物理网络性能。

macOS/iOS场景、Swift38、iOS45通过/4跳过、最终E2E233通过/39忽略；初次E2E关窗规划网络超时单项与完整复跑通过，保留未重现记录。生产代码未变，构建证据沿用v59/v58。原始数据、所有样本和限制见`target/norma-ios-evidence/v60/summary.json`与连接证据native_fragment_and_idle_v60；真实云规划、APNs/物理设备与完整界面验收仍待完成。

## 聊天心跳与正文事件分离（v61）

`ConversationWatchUpdate`增加`heartbeat`：它仅代表通过原有scope/stream/sequence核验的保活，不携带正文。`current(page)`仅用于完整基线或实际内容事件；`recovering`仍要求清空旧内容。SwiftUI继续消费心跳，不重复调用全文merge；缓存仍保留，错误心跳仍清空并按原策略恢复。Mac JSON协议和2s心跳周期不变，项目订阅的连接核验时间继续更新。

v60的原生测量命令现在还断言空闲窗口的`content_updates == 0`，并要求至少2次持续接收，防止通过停止消费得到“零更新”。本轮macOS/iOS各收到3次心跳、0次内容回调，原文与工具详情完整保留；对应CSV均发送1158字节。原生事件数证明省去重复合并，不直接证明SwiftUI重绘、稳定CPU百分比或电量收益。

无效心跳/缺口单测、两平台大正文、真实Codex/Claude双回合与恢复/撤销共8项原生场景通过。Swift38、iOS45通过/4跳过、E2E233通过/39忽略，模拟器Release/未签名iPhone构建通过。重跑仍使用上一节命令，当前日志位于`target/norma-ios-evidence/v61/`；v60历史证据保留。完整记录见native_chat_heartbeat_v61；真机与完整Computer Use继续统一验收。

## 真实Todo入队、撤回与重新安排（v62）

既有原生Todo检查现在从没有决定的计划开始：实际Agent生成时手机入队首项，确认未发送，再通过`cancelTodoQueue`取得可重排状态；停止原生成、确认同会话保留后显式重排。后续仍拒绝第二项、按队列派发第三项。重排前没有Todo原文，结束后只有两条准确Todo指令与两个完成回合；原入队回执/取消状态保留，原操作重放不能改变最新尝试。计划内容仍为fixture，决定与取消结果不再由测试预写。

```bash
swift build --package-path ios --product PairingProbe
cargo build -p norma --bin multisoul-agent-hook
PATH="$PWD/target/debug:$PATH" NORMA_SWIFT_PROBE="$PWD/ios/.build/debug/PairingProbe" \
  cargo test -p e2e --lib todos_native -- --ignored --nocapture --test-threads=1
PATH="$PWD/target/debug:$PATH" NORMA_IOS_SIMULATOR=5265591D-704C-4515-868D-C2D64F94A7FA \
  NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-transport.sh" \
  cargo test -p e2e --lib todos_native -- --ignored --nocapture --test-threads=1
```

两平台各两种真实Provider通过，原生日志分别保存在`target/norma-ios-evidence/v62/macos/`和`ios/`。E2E233、Swift38、iOS45通过/4跳过；只有测试与文档改变，生产构建沿用v61/v59。这里验证真实原生请求与执行，没有完成SwiftUI按钮、云规划质量或物理网络验收；完整范围见native_todo_cancel_retry_v62。

## 统一验收条件（v63）

本轮仅复核环境，没有新增实现或测试结果；最新回归仍为v62。2026-09-09 01:34–01:35 CST，Computer Use报告Mac锁屏，物理iPhone 11不可用，Tailscale停止且无在线peer；当前任务环境未设置APNs/正式云模型变量，未再次读取Keychain。此检查不代表全机器配置扫描。完整界面、真实云规划、物理双机/跨网性能与APNs锁屏继续待验收；TestFlight取消，连续三轮实时核验仍为相同外部阻塞，Goal已标记blocked（未完成）。原始记录见`target/norma-ios-evidence/v63/summary.json`与连接证据`environment_recheck_v63`。

## 本地搜索与Todo收口（v64）

项目/卡片通过当前已验证快照进行名称、任务标题、状态和Agent筛选，未引入额外网络轮询、缓存或跨电脑合并。SwiftUI使用系统搜索/菜单，空结果可清除恢复；原生WSS实测创建/删除卡片会更新筛选结果。Todo通过准确回合终态区分默认待办与全部记录，失败、停止及结果未知继续保留，完成不等于成果验收。

Swift40、iOS47通过/4跳过，完整E2E最终233通过/39忽略；首次规划route缺失断言失败已保留，单项及整套复跑通过但根因未定。当前代码构建通过；新搜索UITest与完整Computer Use/真机验收按用户要求后置。功能状态见Goal的v64表格，具体证据见`target/norma-ios-evidence/v64/summary.json`。

## 计划/Todo持续同步（v66）

`watch_plan`复用已授权历史计划读取与项目变更信号，完整变化快照配合无正文心跳；SwiftUI移除每2秒重新建立连接的轮询。正文更新只来自有序流，单次操作回查只核对本机请求。接收端验证手机、项目、计划、授权版本、流身份及序号，心跳在内部消费，不覆盖缓冲中未消费的正文。命令/队列写入与准确回合证据落盘后通知订阅，关闭项目仍可只读查历史；权限撤销关闭连接。

Swift41、iOS48通过/4跳过、E2E233通过/39忽略；macOS/iOS真实Codex Todo通过新流获得终态。首次两次macOS在中断后校验失败，补诊断后两平台通过，根因仍未定。证据：`target/norma-ios-evidence/v66/summary.json`。此轮恢复TestFlight发布，物理设备、跨网、APNs和完整页面验收继续独立跟踪。
