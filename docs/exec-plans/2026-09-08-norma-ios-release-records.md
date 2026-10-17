# Norma iOS 与双端自测发布记录

按时间从新到旧排列。较新的安装包状态优先于较早条目里的「当前可安装」句子；各节仍保留当时的版本号和证据。

> 历史：见 git 提交。原文路径：`docs/exec-plans/2026-10-06-mobile-self-test-release.md`、`docs/exec-plans/2026-10-05-workspace-chat-self-test-release.md`、`docs/exec-plans/2026-09-30-norma-ios-testflight-dispatch.md`、`docs/exec-plans/2026-09-08-norma-ios-testflight.md`。

<a id="release-mobile"></a>

# 2026-10-06 双端自测包

用户授权：重新本地打包并安装 Mac，发布 iOS，由用户进行蜂窝任务与断线恢复验收。

## 已交付

- Mac：0.1.73（20261006.131442），已替换 `/Applications/Norma OS.app` 并通过 Computer Use 启动。正常退出旧版后安装，旧包备份于 `/Users/alan/Documents/mybook/norma-ai-os/build/release-backups/local-test-20261006/Original installed Norma OS.app`。
- iOS：0.1.0（53），Apple 回读 `VALID / IN_BETA_TESTING`，加入原「Norma 内测」组，中文测试说明逐字核对。Build / Delivery ID：`1b72c9d2-358b-46fc-87cd-70b87ef9a472`。没有新增测试邀请或公开 App Store 上架。
- Mac 手机服务已开启，原 iPhone 配对保持「全部项目 · 控制 · 10 个项目 · 权限版本 1」。新版进程建立到 `47.251.242.24:443` 的出站连接。未修改数据目录或安装证书。

## 产物与验证

Mac DMG：`/Users/alan/Documents/mybook/norma-ai-os/build/Norma OS_0.1.73_aarch64.dmg`，197449962 字节（188.30 MiB），SHA-256 `dc1003dd2369f26ad4edf93d6a394f9e0318b1f5942fd371ecae05044393e1f9`。Developer ID 团队 `2BF8G9AN4L`；严格签名、DMG 布局与完整性、安装后的 4 个可执行文件哈希均通过。此本地自测包未公证，Gatekeeper 评估不通过且无 stapler 票据；未将其描述为正式公共分发包。按现行打包脚本保留 manifest 版本 0.1.73，更新 bundle 构建号。

iOS IPA：`/Users/alan/Documents/mybook/norma-ai-os/build/local-test-release-20261006/Norma_0.1.0_53.ipa`，91179046 字节，SHA-256 `d3e5c113d95f2bbf8cf3716ba314746cfcf2d1cda6db660f14f299e3a1cfc85a`。App 与 Widget 均为 build 53；严格签名、production APNs、禁止调试、无测试 bundle、无隔离网关配置、Apple 校验与单次上传通过。沿用既有相同加密用途与分发范围。

Swift 核心：XCTest 111 项（2 项条件跳过，零失败），Swift Testing 26 项通过。Rust 文件行数、禁止 allow、Agent 指南行数检查通过。Mac Release 构建通过。Mac 1197 项、iOS 195 项生产输入在构建前后没有漂移。真实 App 已观察到原 5 个工作空间、已开启手机服务与原 iPhone 配对。

发布原始证据、哈希清单及 Apple 回读：`/Users/alan/Documents/mybook/norma-ai-os/build/local-test-release-20261006`。iOS 版本在 `ios/project.yml` 从 52 更新为 53，通过 xcodegen 重新生成 Xcode 项目；没有提交或推送代码。

## 用户自测

1. 在 TestFlight 更新到 0.1.0（53），Mac 保持在线和唤醒。
2. 若手机仍显示「启用云端聊天」，先与 Mac 连接同一 Wi-Fi，在「Norma → 输入文字」点击启用；现行初次注册需通过已配对的 Mac 直连取得证明。已有云端凭据可直接继续。
3. 关闭手机 Wi-Fi，使用 4G，在工作空间群聊通过 @ 选择一张空闲 Agent 卡片，对隔离测试项目发送创建唯一标记文件的任务，检查真实文件和手机结果。
4. 执行中断开手机网络并恢复，检查原任务继续、回执与历史补齐、唯一标记只出现一次。结果未知时先查原回执，不重复派工。

蜂窝任务与断线恢复本轮由用户验收；构建、上传、TCP 连接和模拟器记录均不替代真机验收。iPhone 11 支持 4G，5G 尚未验证。


---

<a id="release-workspace"></a>

# 工作空间群聊双端自测发布

日期：2026-10-05。授权：用户要求「发布手机和桌面版本我自测一下」。

本次发布当前 `dev` 工作区的自测包；不部署云服务，不发布公共 GitHub Release 或 Sparkle 更新，不表示完整实现 Goal 或 UI 验收已通过。

## 安装包状态

| 平台 | 版本 | 当前状态 |
| --- | --- | --- |
| iOS | 0.1.0（47） | 顶部概览修复；Apple 回读 VALID / IN_BETA_TESTING，原「Norma 内测」组可安装；中文说明一致 |
| macOS | 0.1.71（20261005.010042） | Developer ID 签名自测 DMG 已生成；安装盘布局和完整性通过；未公证 |

发布证据保存在 `target/self-test-release-2026-10-05/`。手机与桌面生产输入分别冻结并校验，不把隔离验收的端点、证书 pin、测试 namespace 写入手机 Release。

iOS Build / Delivery ID：`a8d8f402-a9ff-48e4-974e-a45cad568d20`。IPA 87,369,838 字节，SHA-256：`cb4200e7fef1eec67dbe70784c545668c017b90c20d96026244672dbe9cf5da8`。严格分发签名、production APNs、禁止调试 entitlement、Apple 校验与单次上传通过；沿用既有加密用途分类，没有新增邀请或外部 Beta。

桌面自测包：`build/self-test-20261005/Norma OS_0.1.71_aarch64.dmg`，197,144,768 字节，SHA-256：`9713cf1d5f1a2bd7b77ab2c62f7ce49a88594f74a27d44b1dc62d63931c64eba`；相邻 `.sha256` 校验文件已生成。App 标识 `com.norma.os`，签名团队 `2BF8G9AN4L`。本次打包使用最新冻结源码和 production-data 配置，源输入无漂移。

Computer Use 已打开真实 Finder 安装盘并核对 App／Applications 图标、箭头、名称与背景布局；截图：`target/self-test-release-2026-10-05/mac-installer.png`。没有覆盖用户已安装的 App 或终止其运行项目。

**桌面正式发布尚未完成。** `release-dmg` 要求正式发布包通过 Apple 公证、stapler 与 Gatekeeper。公证 CLI 的加速上传、标准 S3 端点和官方 Notary REST API 上传均在当前网络/代理下超时；REST 提交与 AWS TLS 连通可用，但大文件上传速率不足。没有将公证记录的 `In Progress` 当作成功，没有生成虚假的公证票据。本机先交付明确标注为未公证的签名自测包；更换可用网络或代理后再补公证。临时上传凭据文件已清除，系统代理设置未修改。

DMG 首次生成受旧 Norma 映像挂载影响：上游 create-dmg 错取首个挂载盘而卸载失败。核验旧 Norma 映像没有打开文件后正常卸载该映像，再按仓库打包脚本生成本次新包；没有强制卸载其他磁盘或终止用户 App。

## 本包变化

- 手机读取并配置真实桌面工作空间、项目区域与 Agent 卡片；进入工作空间直接打开群聊。
- 工作空间消息通过 Mac 处理；普通讨论由 Norma 回应，明确执行或选择具体 Agent 后使用桌面会话与真实执行目录。
- 支持局域网优先及同协议中继、稳定操作 ID、回执核查、增量历史和本地长期保存。旧工作线程只读保留，旧待发送操作不自动重新派发。
- 群聊保留浅色页面、系统导航与 Sheet、玻璃功能层、键盘避让及减少动态效果处理；修复长历史首次打开时定位到最近成果。

## 自测顺序

1. 退出旧桌面 App，安装本次 DMG，再打开 Norma OS。先确认桌面的 Norma 模型可以正常对话，再建立工作空间、关联实际项目，并准备一张可执行的 Agent 卡片。群聊普通讨论使用 Mac 的模型设置。
2. 开启桌面的手机连接服务；手机更新到最新可用的 TestFlight 构建。首次连接扫描本次桌面二维码；已有配对可先尝试复用，配对失效再重新扫码。
3. 手机进入「工作空间」，确认名称、项目和 Agent 与桌面一致。修改一项设置，检查桌面和手机刷新后均一致。
4. 发一条普通讨论，检查 Norma 回应且没有自动新建执行任务。然后通过 `@` 选择具体卡片，给出容易核验的任务，例如在测试项目创建 `mobile-proof.txt` 并写入指定的一行内容。
5. 检查手机接收回执、桌面卡片实际执行、磁盘文件真实存在，以及手机成果内容。对同一成果追问，确认复用原卡片和会话。
6. 查看较早消息后等待新回复，检查是否保留阅读位置。关闭并重开手机，检查历史和未发送草稿仍在。Mac 重启后检查回执与历史补齐。
7. 同一 Wi-Fi 流程完成后，再测试关闭 Wi-Fi、蜂窝网络远程连接；该场景尚未由开发端验收。结果未知时先查询原操作回执，避免手动重复派工。

## 当前证据与限制

- Swift 核心 XCTest：107 项，2 项按条件跳过，0 失败；Swift Testing：11 项通过。
- 群聊长历史定位原生 UI 测试：1 项通过；Computer Use 已进入真实模拟器群聊，确认最近真实 Codex 成果可见，截图为 `ios-latest-result.png`。
- `cargo check -p app` 通过；旧线程写入退休聚焦测试 2 项通过；源码行数、禁止 allow、存储边界和性能护栏通过。
- 前序隔离环境已跑通手机提交 → 真实 Mac/Codex 创建文件 → 成果回手机及原任务追问；本次发布包的真机扫码、蜂窝网络和正式 Mac Norma 模型回复由用户继续验收。
- 独立只读 UI 子 Agent 复核本批修复为 PASS（限已核验样例）：最新成果定位、紧凑具名回执、用户时间、Markdown 路径/代码围栏、@ 勾选、键盘避让与草稿保留。它核对了最新截图与累计三项原生 UI 测试通过日志。
- 独立 UI 总验收尚未完成；空群聊、大字号、VoiceOver、惯性滚动中新回复、Reduce Motion、完整 Sheet 手势等不能标为已通过。语音通话后置。

`git diff --check` 仍报告旧代码清理文件的末尾空行，未影响编译或运行；本次不在输入冻结后改写这些文件。

## 构建 47：群聊顶部概览修复

- 原因：概览原来位于消息 `LazyVStack` 内，打开长历史默认定位最新消息后，概览被滚出可视区域。
- 修复：将概览移到系统导航栏下、消息滚动区外，保留项目与 Agent 数量、Mac 连接状态和原生概览 Sheet。消息仍默认显示最近成果，输入栏继续避让键盘。采用 apple-design 的稳定空间关系、系统导航与可逆 Sheet；阅读卡保持实底。
- 聚焦失败测试已复现原实现；修复后四项原生 UI 测试通过，零失败、零跳过。随后补上「最新成果滚出可点击区域」的具体断言，再跑概览聚焦测试一项通过，证明实际进入早期历史且概览保持原位置（容差 1 pt）。
- Computer Use 已操作真实模拟器，核对概览、项目/成员 Sheet、关闭与草稿编辑；实际滚动位移以原生 XCUITest 及附件证明。独立只读 UI 子 Agent 复核 PASS，限定本轮顶部概览与既有交互回归，不代表完整 UI 总验收。
- 证据：`target/workspace-overview-fix-2026-10-05/` 中的 `red.log`、`green.xcresult`、`scroll.xcresult`、`test-attachments/`、`overview-top.png`、`overview-sheet.png`。Release 生产输入冻结后未变化，仅加强了不参与分发的 UI 测试断言。
- IPA：87,383,558 字节；SHA-256 `09646dfa0c3c75906b0163def2101ea2ed2761cd75088088b83cfb122327df99`。分发签名、production APNs、禁止调试 entitlement、隔离网关配置为空、Apple 校验均通过。仅更新手机包。
- 单次上传成功，Build / Delivery UUID：`adfb33f2-7273-4a3d-98d6-93efa3a62d54`。最终 Apple 回读 `VALID / IN_BETA_TESTING`，确认加入原内测组 `1e9af1ec-bf35-4a49-b7fd-01169c421384`，中文更新说明逐字一致；手机可从 TestFlight 更新到 47。


---

<a id="release-dispatch"></a>

# TestFlight：单次扫码注册与 iPhone → Mac 派工验收

## 当前状态（2026-09-30）

- **0.1.0（45）已可安装**。2026-09-30 14:28（北京时间）Apple 回读确认 `VALID / IN_BETA_TESTING`，已关联原「Norma 内测」组。Build/Delivery UUID `950c0463-4f23-4ac9-883c-2ddceed60424`，中文说明已核对。
- 43 的双码流程已被替代。44 的真实页面验收发现“添加电脑”闪退，已在 Apple 标记 `expired=true` 并回读确认，不能用于验收。45 已修复环境注入并通过对应 XCTest UI 回归。
- Mac 专用验收版和常驻本地中继已升级，配对身份与数据保留；原安装 `/Applications/Norma OS.app` 未替换。备份在私有验收目录的 `backups/single-scan-44/`。
- 当前模拟器已恢复常驻服务。用户原 `Test / 2222` 线程中的 `@Alice 123123` 自动送达 Mac，Alice 返回真实澄清回复，错误消失；未替用户验收成果。

## 你如何验收

1. 在 TestFlight 更新到 **0.1.0（45）**。手机和 Mac 连接同一 Wi-Fi，Mac 保持唤醒。
2. 保持 **Norma iPhone Acceptance** 与本地中继运行。Mac 打开 `Codex Task` 项目，在 Settings → Mobile Connection 确认连接开启。
3. 已配对手机：打开 Norma 后自动补注册，原待发送消息继续发送，不需要重新扫码或第二个邀请码。新手机：更多模块 → 已连接电脑 → 添加电脑，在 **Norma 内扫描 Mac 设置页二维码**，在 Mac 确认控制权限后自动完成注册。
4. 在工作空间设置关联 `Codex Task` 与 `Alice`；进入工作线程，通过 `@` 选择 Alice，发送：**创建 iphone-acceptance.txt，写入 hello from iPhone；只修改这个文件，不提交 Git，完成后告诉我。**
5. 检查消息“已送达 Mac”、任务状态、Alice 回复与成果；查看真实文件，再自行选择验收或退回。退出并重开后应回到同一线程，不能重复执行。

本轮测试入口是 **直接 @Agent 派工**。`@Norma` 真实模型编排、实时通话、公网跨网络尚未开放，不应据本次注册通过宣称全部 Goal 完成。

本版本固定访问 `https://192.168.31.165:9444/`，TLS 证书固定校验。Mac/IP 或网络变化后需要重新准备验收环境；没有部署云端，也无需用户输入供应商 Key。

## 常驻环境与模拟器

私有目录：`~/Library/Application Support/Norma Local Acceptance/`。

- `启动本地中继.command`：回环端口 8787、手机 HTTPS/WSS 端口 9444，无临时测试到期计时。
- `打开 Mac 验收版.command`：专用二进制、原数据和原 Codex 配置；重开后在设置确认手机连接已开启。
- `打开 iPhone 模拟器验收版.command`：给当前模拟器绑定常驻服务。模拟器需用此脚本重新启动，**TestFlight 不依赖该脚本或启动环境变量**。后续自动化使用独立模拟器，禁止把用户正在验收的模拟器切换到会到期的临时服务。

本轮用户遇到的 `Could not connect to the server` 根因：验收者留下临时测试端口 49165，该服务 10 分钟到期退出；用户随后在同一模拟器中配对了真实 Mac，但手机仍访问临时网关。已改回常驻 8787 与固定 namespace，没有删除配对或待发送记录。此问题属于本地测试交接，不能通过让用户重复扫码解决。

## 单码注册修复与证据

| 验收点 | 结果 |
|---|---|
| 单码真实协议 | Rust TLS 配对设备请求短期注册证明 → Node 验签和独立设备注册 → HTTPS/WSS 中继；`final-green.log` 1/1 PASS |
| 注册边界 | 错配对 token、撤销、篡改、未知 Mac 证书、过期证明拒绝；相同身份重试幂等、重启恢复；同一集成测试覆盖 |
| 原生身份与持久化 | `final-native-run.log` 1/1 PASS；`native-test.log` 2/2 PASS，真实二维码图像解码、Mac 审批、App 注册、Keychain 恢复、真实中继认证；零邀请 |
| 页面闪退回归 | `ui-guide-red.log` 真实失败；修复 RootView modal 环境注入后 `ui-guide-green.log` 1/1 PASS |
| Computer Use | 实际进入添加电脑、扫码和系统图片选择器；用户配对后，实际打开原 Test/2222 线程，确认原消息送达、真实 Alice 回复、无注册错误。`user-thread-recovered.png` |
| 常驻服务 | 更新后 HTTPS/WSS 与 Mac 在线路由 PASS，`persistent-ready.json`；没有把临时测试服务当成用户环境 |
| 只读 UI 审查 | `ui-review.md`：修复后台状态、共享错误、只读权限及并发注册反馈；最终结论见报告限定范围 |

证据目录：`target/norma-ios-evidence/single-scan-20260930/`。模拟器图片识别不等于物理相机扫码；真机权限、扫码、Apple Speech 与实际手感仍由用户验收。

注册复用 Mac 已有 TLS 身份和服务的可信 Mac 名单。Mac 仅签发初次注册证明，不发语音通行证；手机独立生成长期凭据，收到服务成功回执才保存为已授权。失败仅保留待注册凭据以便幂等重试。只读 Mac 授权仅显示“可查看项目”，不会承诺派工。

45 IPA：88,399,037 字节，SHA256 `fa3b0666ddff362bb5c075b5a00d3ac7e0875286c17f72bfc452a8bfb98452ef`。

45 发布证据目录：`target/norma-ios-testflight/single-scan-45/`。186 项生产输入在归档前后核验哈希；未提交或推送当前工作区。Issue #57/#59 与整体 Goal 仍保留剩余验收项。

## 43 历史验收记录（不再使用旧邀请步骤）

## 本次已验证

| 检查 | 结果与证据 |
|---|---|
| Swift Core | 142 项，8 项按条件跳过，0 失败；`target/norma-ios-evidence/physical-dispatch-20260930/swift-core.log` |
| 真实 Mac/Codex 原生闭环 | 原生页面建空间、绑定项目/Agent、发送任务、真实生成两文件、冻结成果、重启恢复；执行记录仅一行。`real-dispatch.log`，L3 用例 1/1 PASS |
| Computer Use | 实际操作 Mac 打开项目/手机服务；模拟器点击原线程成果、查看文件和返回，确认未自动验收；`cua-result.md`、`cua-real-result.png`、`cua-real-thread.png` |
| 本地 TLS | 错 pin 拒绝、邀请只能兑换一次、真实 HTTPS/WSS、管理端点不暴露；`tls-green.log` |
| 原生打包配置 | App 默认模型读取 Info.plist，真实 HTTPS 邀请兑换、Keychain 恢复及 WSS；`tls-native.log`，1/1 PASS |
| 中继边界 | 认证、隔离、撤销、断开、重启检查通过；`relay-regression.log` |
| 常驻验收环境 | 私网 HTTPS/WSS 与配套 Mac 实际在线路由通过；`persistent-ready.json` |
| 发布输入与签名 | 185 项生产输入归档前后哈希一致，`aps-environment=production`，禁止调试，分发签名通过 |

本轮未把模拟器测试算作物理手机扫码/麦克风/Apple Speech 验收。**直接 @Agent 是此次入口**；`@Norma` 真实模型编排、实时语音、跨公网、完整断网/后台与后续修改闭环仍按既有 Goal 跟进，不能视为全部完成。

## 实现与发布记录

- iOS 复用现有证书固定校验，同时用于邀请 HTTPS 和中继 WSS；端点/证书/隔离 namespace 只可由构建配置提供，邀请不能改变端点。默认配置为空，普通正式构建仍走原服务。
- Mac 继续使用现有中继注册、端到端加密和项目授权。Node 本地 TLS 入口复用正式网关；管理员接口与 Mac 注册仅监听回环，手机端不含管理员或供应商 Key。
- 配套 Mac 复用本轮真实派工通过的同一二进制，独立副本和数据目录；本地 ad hoc 签名，不作为正式 DMG 发布。
- IPA：`target/norma-ios-testflight/local-dispatch/export/Norma.ipa`，88,365,682 字节，SHA256 `99256e2fbbdee810609daad9fb4db8e2cbb845bfce7b928b251052c16aaabb6d`。
- 发布证据：`target/norma-ios-testflight/local-dispatch/`。真实 L3 全量证据：`/private/var/folders/kr/106cttn15rv7xlgv2y_7v4wr0000gn/T/norma-thread-real-D9JclA/`。
- 发布期间 Node 直连 Apple API 两次 TLS 连接重置；使用本机已配置的网络代理后恢复。关联组后第一次回读误用了 Apple 不支持的 `builds/{id}/betaGroups` GET 路由，改为读取现有组的 builds 列表并核对 Build ID；最终可安装状态、组成员关系、中文说明与未过期状态全部通过。未重复上传 IPA。
- 发布来自当前未提交工作区；未执行 commit/push，未部署云端。Issue #57/#59 在剩余验收满足前保持 OPEN，整体 Goal 保持 active。


---

<a id="release-testflight"></a>

# Norma iOS TestFlight发布记录

> 2026-09-30 单码注册更新：43 的额外邀请码步骤已被替代，44 已停止测试；45 发布与当前验收流程见 [单次扫码与派工验收](2026-09-08-norma-ios-release-records.md#release-dispatch)。真机验收及其余 Goal 项保持未完成。


更新时间：2026-09-30。当前状态：**0.1.0（43）已进入现有「Norma 内测」组，可安装**；Apple 回读确认 VALID / IN_BETA_TESTING、未过期，中文说明一致。本次是同一 Wi-Fi 的本地派工验收包，需要配套 Mac 专用版，详见 [安装与验收步骤](2026-09-08-norma-ios-release-records.md#release-dispatch)。

## 原生工作空间与真实 Mac 派工 · TestFlight 0.1.0（43）（2026-09-30）

按用户「可以发布到 TestFlight 然后我来验收」授权，完成真实 Mac/Codex 派工与成果回传后发布。包含当前工作区原生想法库、Workspace/项目与 Agent 关联、工作线程、任务与成果页和交互动效。新增仅在构建时显式启用的固定证书私网 HTTPS/WSS；保持本地运行，不部署云端。物理手机由用户验收，`@Norma` 真实模型编排、语音与公网场景不在本包完成声明内。

2026-09-30 12:06（北京时间）回读 `VALID / IN_BETA_TESTING`，Build/Delivery ID `b97f7208-83c6-4a47-81b0-1e7d54b7877a`；原「Norma 内测」组关联、未过期与中文说明通过。沿用既有加密声明；没有新增测试者、外部 Beta 或公开发布。185 项生产输入哈希前后一致，分发签名和 production entitlement、Apple 校验及单次上传通过。IPA 88,365,682 字节，SHA256 `99256e2fbbdee810609daad9fb4db8e2cbb845bfce7b928b251052c16aaabb6d`。

验证：Swift Core 142 项、8 条按条件跳过、0 失败；真实 Mac/Codex 原生 L3 1/1、Computer Use 成果浏览、原生打包配置 HTTPS 邀请与 Keychain 恢复 1/1、中继与 TLS 边界通过。证据在 `target/norma-ios-testflight/local-dispatch/` 与 `target/norma-ios-evidence/physical-dispatch-20260930/`。未 commit/push；整体 Goal 仍 active。

## 原生删除确认 · TestFlight 0.1.0（42）（2026-09-16）

按用户“发布和构建一下iOS手机端”授权发布。相对构建 41 的 iOS 差异为卡片删除流程：选项页改为系统原生确认弹窗，确认后直接提交；成功后自动回到项目列表，删除过程中禁止关掉选项页；结果未知时保留查询入口，不自动重发。涉及 `CardOptionsView`、`CardCommandSheet`、`CardCreationFlowTests` 与 e2e 卡片 fixture。本轮只发布 iOS，未重新打包桌面。

归档、导出、分发签名与 Apple 校验通过。IPA：`target/norma-ios-testflight/rt-build42/export/Norma.ipa`，82626811 字节，SHA256 `9b07d38c442a93d1abb3aab7297f6d41f8ec9bebd08c23b44a1255b9b5570987`。签名为 Apple Distribution (2BF8G9AN4L)，`aps-environment=production`、`get-task-allow=0`、`beta-reports-active=1`。Delivery UUID：`17228816-43bf-43eb-96f1-abd1f2a7dfc7`（同 Build ID）；Apple 回读 VALID、未过期，加密分类沿用 `usesNonExemptEncryption=false`，已关联「Norma 内测」组（IN_BETA_TESTING），中文更新说明已写入。没有新增邀请、外部 Beta 或公开上架。验证：Swift Core 84/0。

## 项目 Agent 卡片与 Cursor 支持 · TestFlight 0.1.0（41）（2026-09-16）

按用户"打包桌面并发布 TestFlight"授权发布。相对构建 40 的 iOS 差异为三个提交（`75d51185`/`a23e81c3`/`e5d74673`）：新建卡片改为读取项目自身的 Agent 定义，可选 Agent 与桌面配置一致；新增 Cursor Agent 卡片支持，并按卡片实际能力显示可用操作；项目卡片页与计划页细节调整。涉及 `NewCardView`、`NewTaskView`、`ProjectCardsView`、`ProjectPlansView` 及 NormaCore `MobileCommand`/`Snapshot`/`Wire` 等 24 个文件（+393/−77）。

归档、导出、分发签名与 Apple 校验通过。IPA：`<scratchpad>/rt-build41/export/Norma.ipa`，82620565 字节，SHA256 `ded38d237444376fd7cc30cf604165db5ffd41b25a2955f1ba5b4489f8cebd8b`。签名为 Apple Distribution (2BF8G9AN4L)，`aps-environment=production`、`get-task-allow=0`、`beta-reports-active=1`。Delivery UUID：`177aa3dd-6ee1-40a8-a341-b9a444c4d276`（同 Build ID）；Apple 回读 VALID、未过期，加密分类沿用 `usesNonExemptEncryption=false`，已关联「Norma 内测」组（IN_BETA_TESTING），中文更新说明已写入。没有新增邀请、外部 Beta 或公开上架。验证：Swift Core 84/0。

首次归档误加 `SKIP_INSTALL=NO`，把测试 target 产物装入归档且缺 `ApplicationProperties`，导致 export 报 `method` 无法识别；去掉该标志重新归档后正常。`xcrun altool` 忽略 `--apiKeyPath`，只从 `~/.appstoreconnect/private_keys/` 等固定目录读取 `.p8`，已复制一份到该目录。

配套桌面已重新打包：`build/Norma OS_0.1.71_aarch64.dmg`，193086846 字节，SHA256 `7e1ef3cb7e4bc1b31454768c053223665db67f6e97975dc02ce78316212f14f3`；Developer ID 签名、未公证，`hdiutil verify` 与相邻校验和通过；未安装或启动新包。本轮桌面证据未落盘到 `target/norma-ios-testflight/`（发布脚本目录历史上被多次清空），iOS 产物与日志在会话 scratchpad 内。

## 语音服务迁移自有服务器 · TestFlight 0.1.0（38）（2026-09-15）

按用户"打包桌面并发布 TestFlight"授权发布。语音服务从 Vercel 网关迁移到自有服务器 `soulvoiceai.com/norma/voice`（部署细节见 [迁移记录](2026-10-06-norma-cloud-deployment.md#cloud-superseded)）。相对构建 37 的 iOS 输入差异为 `VoiceCall`、`VoiceCallAuthorization`、`VoiceCallDoubao`、`VoiceCallEvents` 与 NormaCore 的 `VoiceAuthorization`、`VoiceSessionTicket` 共 8 项（含版本配置）：iOS 默认地址改为新基址并自动迁移旧 Norma/Vercel 默认值（自定义地址保留），票据校验保留同源/同挂载目录/协议/过期/凭据边界；配套 Rust 侧 `realtime.rs`/`voice_authorization.rs`/`archive.rs` 同步更新并进入桌面包。验证：Swift Core 81/81、`cargo check -p app`。

Release 归档、导出、严格分发签名、production entitlement、Apple 校验与单次上传通过；归档及上传后两次复验 119 项生产输入哈希一致。期间 verify 一度报 `VoiceCall.swift` 哈希不符，排查为发布脚本从 rt-build37 复制时路径未替换（对比的是旧构建清单）而非源码漂移，修正后两次归档均与冻结清单一致。IPA：`target/norma-ios-testflight/rt-build38/export/Norma.ipa`，82576333 字节，SHA256 `1643a5dfc9a1e1190bc5a0901a1ca25697068f9a69544a2428c13498b2fb590f`。Build ID：`3665f83c-2c2d-4958-bc3d-0bc2164c26d2`；Apple 回读 VALID / IN_BETA_TESTING、未过期，原组关联及中文说明均通过（并确认停用的 build 23 未分发）。沿用既有加密分类与不在法国分发范围，没有新增邀请、外部 Beta 或公开上架。证据：`target/norma-ios-testflight/rt-build38/summary.json`。

配套桌面已重新打包（含语音服务器迁移的 Rust 接线）：`build/Norma OS_0.1.71_aarch64.dmg`，193066849 字节，SHA256 `86e0acb2211e59764b287bb9968a3d4ca008c188da2417989f54e7c5bab86491`；Developer ID签名、未公证，DMG 完整性与相邻校验和通过。提醒：新路径的 Mac 工具能力需要更新桌面版，手机也需更新到 38 才能使用新语音基址。

## 指挥台取景与状态提示 · TestFlight 0.1.0（37）（2026-09-14）

按用户"iOS TestFlight 重新发布"授权发布。相对构建 36 的 iOS 生产输入差异为 `AvatarScene.swift`、`CommandCenterView.swift`、`ConversationView.swift` 及配套 UITests/e2e 更新：虚拟形象相机改为半身像取景（1.30/1.08 ≈ 1.20，保留发冠），Avatar 区域增高并加底部渐隐遮罩；通话状态、工具进度、派工目标错误与讨论保存失败提示内联集中在字幕区下方（可点击跳转）；卡片对话页只读历史与项目发现并行加载、发送仍等待快照。工作区另有 push relay 部署配置与文档更新，不进 iOS 包。`target/` 第三次被清空，发布脚本全部重建（entitlements 解析改用 `--entitlements - --xml`，旧 `:` 语法在新版 codesign 下报 No such file or directory）。

Release 归档、导出、严格分发签名、production entitlement、Apple 校验与单次上传通过；归档及上传后两次复验 119 项生产输入哈希一致。IPA：`target/norma-ios-testflight/rt-build37/export/Norma.ipa`，82575158 字节，SHA256 `fbe761204593f8b8eb744fadbe39a58ee32afcdd5633614e446d47cec1c00a91`。Build ID：`40a6ccbe-8f4d-486e-93c3-e6047ed83460`；Apple 回读 VALID / IN_BETA_TESTING、未过期，原组关联及中文说明均通过（并确认停用的 build 23 未分发）。沿用既有加密分类与不在法国分发范围，没有新增邀请、外部 Beta 或公开上架。验证：Swift Core 79/79。证据：`target/norma-ios-testflight/rt-build37/summary.json`。配套桌面随后按用户要求重新打包：`build/Norma OS_0.1.71_aarch64.dmg`，193067024 字节，SHA256 `56fcbf9d539695fdf93de65344eb107524d7371c9b6371e38b873878fed7498e`；Developer ID签名、未公证，DMG 完整性与相邻校验和通过（Rust 源码相对上次打包无改动，仅测试与文档变化）。

## 同内容重发 · TestFlight 0.1.0（36）（2026-09-14）

按用户明确要求重新发布。本构建与 35 的 iOS 生产输入完全一致（119 项哈希逐项核对，仅 `project.yml`/`project.pbxproj` 构建号 35→36 变化），源码零改动，仅用于重新触发一次 TestFlight 发布与安装。工作区相对 `4d08e136` 的改动只有 push relay 服务器端部署配置与文档，不进入 App。桌面沿用 0.1.71（20260914.052310）DMG，未重新打包。

Release 归档、导出、严格分发签名、production entitlement、Apple 校验与单次上传通过；归档及上传后两次复验 119 项生产输入哈希一致。IPA：`target/norma-ios-testflight/rt-build36/export/Norma.ipa`，82571796 字节，SHA256 `9080e2339e034f8324e58903c74ff6f7934fbb5d46ab119358eeb3b05351e936`。Build ID：`358a9b1e-22c2-4ab1-8b3b-c78582598dd6`；Apple 回读 VALID / IN_BETA_TESTING、未过期，原组关联及中文说明均通过（并确认停用的 build 23 未分发）。沿用既有加密分类与不在法国分发范围，没有新增邀请、外部 Beta 或公开上架。证据：`target/norma-ios-testflight/rt-build36/summary.json`。

## 语音免配置接入 · TestFlight 0.1.0（35）（2026-09-14）

按用户"打包桌面并发布 TestFlight"授权发布。独立 Vercel 语音网关（`norma-voice.vercel.app`）已部署，iOS 免 Key 接入：配对并取得项目控制权限后无需填写 URL、Key 或访问口令即可使用默认豆包语音；手机通过已配对 pin TLS 通道领取固定用途授权（24 小时有效、钥匙串保存、最后一小时自动刷新），Mac 使用 P-256 安装身份签名、网关仅信任登记的公开证书。相对构建 34 的 iOS 输入差异为 `CallSettingsView`、`VoiceCall`、`VoiceCallAuthorization`、NormaCore `VoiceAuthorization` 与 `Wire` 共 5 个文件，生产输入扩至 119 项冻结。部署细节见 [Vercel 部署记录](2026-10-06-norma-cloud-deployment.md#cloud-superseded)；GPT 云端同会话派工与物理耳机锁屏测量仍未验收。

Release 归档、导出、严格分发签名、production entitlement、Apple 校验与单次上传通过；归档及上传后两次复验 119 项生产输入哈希一致。IPA：`target/norma-ios-testflight/rt-build35/export/Norma.ipa`，82571795 字节，SHA256 `e7daeda923a700fa9eed0c628129f1b3079e8f1973360a7088b21b02b2e3b972`。Build/Delivery ID：`6ad164e9-6fb3-4d2e-825a-37316a8e0a6c`；Apple 回读 VALID / IN_BETA_TESTING、未过期，原组关联及中文说明均通过。沿用既有加密分类与不在法国分发范围，没有新增邀请、外部 Beta 或公开上架。验证：Swift Core 79/79、`cargo check -p app`。证据：`target/norma-ios-testflight/rt-build35/summary.json`。

配套桌面已重新打包：`build/Norma OS_0.1.71_aarch64.dmg`，版本0.1.71，新构建20260914.052310（UTC）。193066969字节，SHA256 `b175d397bed57be94547d859c2a9cbe1f403dd6454d202be882e5c46d696df4e`；Developer ID签名、未公证，DMG 完整性与相邻校验和通过。首次打包因本机 SOCKS 代理致 create-dmg 下载失败（与 build 29 同因），仅对本打包进程去掉代理后重试成功；未安装或启动新包。

## 豆包闭环完成 · TestFlight 0.1.0（34）（2026-09-14）

按用户"打包桌面并发布 TestFlight"授权发布。实时 Goal 已推进至 v20 并收口：豆包语音派工任务网关接入完成，可通过豆包对话向绑定项目 Agent 派工并接收真实回执，完成后通话中播报；网关三组回归、原生豆包音频 L3 与"真实豆包云端→真实 Codex→原生播报"端到端 L3 通过。相对构建 33 的 iOS 输入差异（按 mtime）：`ChatDictation`、`CommandCenter/{CallSettingsView,VoiceCall,VoiceCallBinding,VoiceCallDoubao,VoiceCallReports}`、`CommandCenterView`、`NotificationPresenter` 及 NormaCore 的 `RealtimeBinding/RealtimeOperation/VoiceSessionTicket`，共 117 项生产输入冻结。GPT 云端同会话派工、物理耳机锁屏测量与界面用户验收保留未测标记，已在更新说明中明示；默认线上网关未部署。

Release 归档、导出、严格分发签名、production entitlement、Apple 校验与单次上传通过；归档及上传后两次复验 117 项生产输入哈希一致。IPA：`target/norma-ios-testflight/rt-build34/export/Norma.ipa`，82545985 字节，SHA256 `07ed1818b555c2bf88132d656a2501935868a38bdb276a5fe41808e477d8e9ec`。Build/Delivery ID：`c0b533a2-0018-41e8-957a-cbae8fe8f3bd`；Apple 回读 VALID / IN_BETA_TESTING、未过期，原组关联及中文说明均通过。沿用既有加密分类与不在法国分发范围，没有新增邀请、外部 Beta 或公开上架。验证：Swift Core 78/78、`cargo check -p app`、agent crate 测试全过。`target/` 夜间再次被清空，发布脚本第二次重建（Node WebCrypto ASC 客户端）。证据：`target/norma-ios-testflight/rt-build34/summary.json`。

配套桌面已重新打包：`build/Norma OS_0.1.71_aarch64.dmg`，版本0.1.71，新构建20260914.003205（UTC）。193090793字节，SHA256 `a55a66bce0aa885dc18c55be916cdc4d083bd57ba0ec78d4915fdba55ccba653`；Developer ID签名、未公证，DMG 完整性与相邻校验和通过；包含豆包任务网关桌面接线；未安装或启动新包。

## 实时指挥台当前状态 · TestFlight 0.1.0（33）（2026-09-14）

配套桌面已重新打包：`build/Norma OS_0.1.71_aarch64.dmg`，版本0.1.71，新构建20260913.165230（UTC）。193058065字节，SHA256 `51a141d25bcd4b7a477c0c54339787da34e9881e16ee822dcea5a0697ac02e11`；Developer ID签名、未公证，DMG 完整性与相邻校验和通过。包含实时指挥台桌面接线（Agent 实时意图、存储持久回执、无窗口 Adapter 等）；首次打包因本项目旧 DMG 遗留挂载（`/Volumes/Norma OS`）致 create-dmg 失败，正常卸载后重试成功；未安装或启动新包。

按用户"打包桌面并发布 TestFlight"授权，并在确认另一会话仍在开发豆包任务网关（工作区 180 处改动、文件持续写入）后，用户选择**立即发布当前状态**。包含实时指挥台 v7：原生陪伴界面取代旧分段指挥页、GPT WebRTC 与豆包语音传输、插话/字幕/静音/挂断重连、审批与成果收件箱、原话草稿与持久回执、明确切换通话 Mac/项目。豆包派工闭环、云端 GPT 自动汇报与真机（物理设备）验收**未完成**，已在更新说明中向测试者明示。相对构建 32 生产输入从 103 项增至 117 项（新增 `ios/App/CommandCenter/` 等实时文件）；`target/norma-ios-testflight/` 证据目录曾被清空，本轮重建了全部发布脚本（ASC 客户端改用 Node 内置 WebCrypto ES256）。

Release 归档、导出、严格分发签名、production entitlement、Apple 校验与单次上传通过；归档及上传后两次复验 117 项生产输入哈希一致（另一会话写入未混入）。IPA 含 WebRTC 框架，体积增至 82,520,963 字节，SHA256 `9c726e875762d37338e9d40659955a63e319b66b8ab6d28d8285811d3cde433e`。Build/Delivery ID：`96071022-03b1-4c97-9373-866026c930c3`；Apple 回读 VALID / IN_BETA_TESTING、未过期，原组关联及中文说明均通过。沿用既有加密分类与不在法国分发范围，没有新增邀请、外部 Beta 或公开上架。验证依据：Swift Core 76/76、`cargo check -p app`、实时 Goal 文档记录的全仓串行 1842/0。证据：`target/norma-ios-testflight/rt-build33/summary.json`。

## 长回复布局修复 · TestFlight 0.1.0（32）（2026-09-13）

配套桌面已重新打包：`build/Norma OS_0.1.71_aarch64.dmg`，版本0.1.71，新构建20260913.070128。192860228字节，SHA256 `d736a8b50922c19deba19c47a75e0ba33fa3d8c88abdf460a8cffe5d4949a360`；Developer ID签名、未公证，DMG 完整性与相邻校验和通过。包含本轮桌面 Runtime Codex 续跑能力与推送 relay 部署配置；未安装或启动新包。

按用户"打包桌面并发布 TestFlight"授权发布。相对构建 31 的 103 项生产输入仅 `ConversationView.swift` 与两份版本配置变化：卡片对话页历史消息由 `LazyVStack` 改为即时 `VStack` 布局，修复单条可选回复高于屏幕时滚动定位反复跳动；输入栏从 `safeAreaInset` 移入主布局区域并与历史区域分离（`chatContent.clipped()`），长历史下输入栏位置稳定、键盘弹出不再遮挡。不改协议。

Release 归档、导出、严格分发签名、production entitlement、Apple 校验与单次上传通过；上传后生产输入及 IPA 哈希一致。IPA：`target/norma-ios-testflight/chat-build32/export/Norma.ipa`，5676372 字节，SHA256 `bc64fdea51588a5d8fbb8893e085f172459c8982d690bc56a0174d88afed67c3`。Build/Delivery ID：`96f56cf8-3501-4f33-b8ad-c2605d9233b0`；Apple 回读 VALID / IN_BETA_TESTING、未过期，原组关联及中文说明均通过。沿用既有加密分类与不在法国分发范围，没有新增邀请、外部 Beta 或公开上架。

验证：Swift Core 63 项通过；Runtime 168 项通过（含本轮桌面 Codex 续跑能力，不进 iOS 包）；新增 `mobile_codex_resume` e2e 4 项通过；`ChatLayoutFlowTests` 6 项在无 TLS fixture 时按设计跳过，配对的 `ConversationView` 行为由布局代码审查与既有聊天回归覆盖。打包期间 Apple 时间戳服务两次 502 中断，恢复后桌面重试成功。证据：`target/norma-ios-testflight/chat-build32/summary.json`。

## 项目页与任务页精修 · TestFlight 0.1.0（31）（2026-09-13）

按用户"打包桌面并发布 TestFlight"授权发布。相对构建 30 的 103 项生产输入仅 `ProjectCardsView.swift`、`TasksView.swift` 与两份版本配置变化：项目页移除"项目脉络"信息卡、标题更紧凑、加载统一居中，"会话记录／项目成果／项目变更"入口移至卡片对话页菜单；任务页顶部布局重构、项目筛选合并。不改协议。

Release 归档、导出、严格分发签名、production entitlement、Apple 校验与单次上传通过；上传后生产输入及 IPA 哈希一致。IPA：`target/norma-ios-testflight/ui-build31/export/Norma.ipa`，5676693 字节，SHA256 `d56620d7d1b569827d0ca17e7f8e3a3b2c95b7667b3a7d128c6892851303fac6`。Build/Delivery ID：`153dc3d1-4b77-4f2a-a9b7-f62ce27c877f`；Apple 回读 VALID / IN_BETA_TESTING、未过期，原组关联及中文说明均通过。沿用既有加密分类与不在法国分发范围，没有新增邀请、外部 Beta 或公开上架。

验证：Swift Core 63 项通过；模拟器全量 76 通过、8 失败全部位于 PairingFlowTests——清理模拟器遗留配对数据后 9 项中 5 项转绿，余 4 项为历史测试期望漂移（"全部电脑"已改为 accessibilityLabel、"连接电脑后梳理下一步"移至 PlanProjectsView、配对地址提示文案改写、TabBar 查询类型不匹配），涉及文件与线上构建 30 哈希一致，非本次回归；覆盖本次改动视图的 TasksFlowTests／ProjectBoardFlowTests／PrototypeLayoutFlowTests／ChatLayoutFlowTests 均通过。证据：`target/norma-ios-testflight/ui-build31/summary.json`。



## 草稿校验修复 · TestFlight 0.1.0（30）（2026-09-13）

按用户“发布到 TestFlight”授权，只发布 iOS 草稿兼容修复。后台原卡已有历史 session 而尚无 run 时，草稿可以保存；完整范围隔离、准确回执和成果反馈校验继续保留。相对构建 29 的 103 项生产输入仅 `ChatDraft.swift` 与两份版本配置变化。复用 Swift Core 63 项、原生草稿单测 6 项（另 1 项物理文件保护明确跳过）、原生聊天保存／重启／发送流程及完整 e2e 回归，详见 [修复记录](2026-09-13-ios-background-draft-fix.md)。

Release 归档、导出、严格分发签名、production entitlement、Apple 校验与单次上传通过；上传后生产输入及 IPA 哈希一致。IPA：`target/norma-ios-testflight/draft-build30/export/Norma.ipa`，5672611 字节，SHA256 `e76ee65d07101dae3df9ccb063eda36fae2215208709d7678bbd4e1cfd688135`。Build/Delivery ID：`23d5e98f-8d5a-4ada-8892-b6cc79737005`；Apple 回读 VALID / IN_BETA_TESTING、未过期，原组关联及中文说明均通过。沿用既有加密分类与不在法国分发范围，没有新增邀请、外部 Beta 或公开上架。

配套桌面沿用 0.1.71（20260912.154818），本轮未重新打包或替换桌面。更新说明提醒更新前复制未保存的草稿。APNs 继续延期。发布证据保存在 `target/norma-ios-testflight/draft-build30/`。

## 后台项目 · TestFlight 0.1.0（29）（2026-09-12）

按用户“编译一下 Norma 桌面版本和发布 TestFlight”授权构建。iOS 支持直接加载已授权项目；配套 Mac 无需打开项目窗口，关窗保留原卡会话、重开复用，主动 Quit 停止服务与 PTY。实现及完整工作区回归、Swift Core 62 项、项目 UI 2 项与双端 Computer Use 验收见 `2026-09-12-mobile-background-projects.md`。真实云 Provider 调用及真实 iPhone／跨网场景不冒充已完成验收。

Release 归档、导出、严格分发签名、production entitlement、Apple 校验及单次上传通过；103 项生产输入与 IPA 在上传后哈希保持一致。IPA：`target/norma-ios-testflight/background-build29/export/Norma.ipa`，5672726 字节，SHA256 `51996fc4e91074d7f32acec7c1fbf2a769774193101ea71fd5d95ef575d59890`。Build/Delivery ID：`b2069a05-ac5a-4319-bb8b-bf326e3410ff`；Apple 回读 VALID / IN_BETA_TESTING、未过期，原内测组关联与中文说明均通过。沿用用户此前确认的加密分类和不在法国分发范围；未新增邀请、外部 Beta 或公开上架。APNs 部署仍延期。

桌面打包初次受 cargo-packager 的 SOCKS 不支持、上游 create-dmg 对旧挂载卷选择错误及临时卷占用影响；仅调整本次打包进程代理、正常卸载本项目旧只读安装镜像后重试。最终官方入口退出 0；没有修改全局代理、强制卸载其他卷或替换 `/Applications` 中的程序。完整日志、前次 DMG 备份与发布证据保存在 `target/norma-ios-testflight/background-build29/`。

## v133 · TestFlight 0.1.0（28）发布（2026-09-12）

2026-09-12T11:30:11+08:00 确认构建28已VALID / IN_BETA_TESTING、未过期，原「Norma 内测」组及中文说明回读一致。Build/Delivery ID `684f833d-f604-4026-ba34-fa661d3db5c3`。本轮按用户“完成后发布一下 TestFlight”授权发布：修复Claude实时输出已连接但为空时持续加载，合入v131任务读取冗余提示移除和v132项目选择/搜索合并；真实用户卡片的唯一触发原因仍未确认，更新说明要求重点复测。

相对构建27只有LiveAgentOutput、TasksView、WorkspaceStyle和两份版本配置变化；103项生产输入冻结。原生3项聊天回归通过、Swift Core62项通过、聊天网络4项通过；Computer Use实际点击Claude并验证空屏→正文自动更新。桌面全量回归初次受代理502等影响13项失败，进程内去代理并串行后214通过、3项桌面会话恢复失败、60忽略；该模块单独运行仍1通过3失败，已记录，未将完整桌面回归称为通过。测试fixture使用随机身份，正式配对/忘记及退出0，未操作用户配对。

Release归档/导出、严格分发签名、Apple校验与单次上传通过，上传后源码与IPA哈希一致。IPA：`target/norma-ios-testflight/v133/export/Norma.ipa`，5673146字节，SHA256 `6a4de666329e66f6900de3183326ce4ac4194b4260171f325fa948aca90ceafa`。沿用既有加密分类与不在法国分发范围，未新增邀请、外部测试或公开App Store发布。桌面未重启，APNs仍延期；证据：`target/norma-ios-evidence/v133/summary.json`、`target/norma-ios-testflight/v133/`。

## v130 · TestFlight 0.1.0（27）发布（2026-09-11）

2026-09-11T21:18:40+08:00确认构建27已VALID / IN_BETA_TESTING、未过期，原「Norma 内测」组及中文说明回读一致。Build/Delivery ID `6c7a25de-f9f9-4a46-8dfa-f0fc39079334`。按用户明确要求发布v128键盘退出/锁屏恢复底栏位置与v129就绪卡明确拒绝后恢复发送；草稿保留及未知结果防重发保护维持。相对构建26仅RootView、ChatComposer、CardCommandSheet与两份版本配置变化，103项生产输入冻结。

Release归档、导出、严格分发签名、Apple校验及单次上传通过，上传后源码和IPA哈希一致。IPA：`target/norma-ios-testflight/v130/export/Norma.ipa`，5663876字节，SHA256 `fc6527687616f474d354abee35f2f67d47245543a697f109827c2483ec443a74`。复用v128四项原生回归与CUA系统锁屏配合XCTest恢复测量、v129原生拒绝/未知/主动重试回归及完整E2E241通过、0失败、69忽略的证据；本轮仅发布，没有扩大为真机验收通过。

沿用已确认的加密实现及不在法国分发范围，没有新增邀请、外部测试或公开App Store发布。中文测试说明明确Claude自绘硬换行/框线手机排版仍待优化，真实iPhone及LAN/Tailscale由用户继续验收，APNs仍延期；未重启或打包桌面。证据：`target/norma-ios-evidence/v130/summary.json`及`target/norma-ios-testflight/v130/`。

## v127 · TestFlight 0.1.0（26）发布（2026-09-11）

2026-09-11T19:39:31+08:00确认构建26已VALID / IN_BETA_TESTING，现有「Norma 内测」组及中文说明回读一致。Build/Delivery ID `15961c37-e419-4cb3-9a05-3136cee85d23`。本轮仅包含v126决策页重复电脑状态/数量的精简、留白与分段读数改动，另更新构建号；相对构建25的103项发布输入仅CommandCenterView与两份版本配置变化。复用v126已通过的原生流程、隔离TLS/WSS配对及清理证据。

Release归档、导出、严格签名、Apple校验和单次上传通过；上传后输入与IPA哈希一致。IPA：`target/norma-ios-testflight/v127/export/Norma.ipa`，5660385字节，SHA256 `1511e4f8cca7ceed708777436b442363ebe8479453027e1be9eb659eef072c94`。沿用既有加密实现及不在法国分发范围，没有新增邀请或公开发布。测试说明已明确锁屏恢复底栏问题仍未修复；Computer Use因Mac锁定未完成，物理iPhone由用户验收。未重启或替换桌面，APNs仍延期。证据：`target/norma-ios-evidence/v127/summary.json`。

## v118发布结果（历史，0.1.0（24））

- 用户明确要求发布后，重新归档最新源码为0.1.0（24）；包含图片发送、运行卡片动画优化、键盘收起、离线电脑移除及聊天/导航排版更新。此前叫停的构建23仍未关联原内测组。
- App ID `6810038463`；Build ID `42c0cb94-7274-48bd-a844-d31fb3b32ca3`；原内测组 `1e9af1ec-bf35-4a49-b7fd-01169c421384`。中文说明、组关联、VALID / IN_BETA_TESTING及未过期状态回读通过；没有新增邀请、外部Beta或公开App Store发布。
- Release归档/导出、严格分发签名、production entitlement、Apple校验及单次上传通过。上传后103项生产输入和IPA SHA256仍一致。沿用既有加密分类及不在法国分发范围。
- IPA：`target/norma-ios-testflight/v118/export/Norma.ipa`，5662669字节，SHA256 `3540b664ff3706fcc29e05bb40a83128122bf74381ba98f97bc37ddfbd646520`。复用v114–v117的聚焦测试、原生UI与Computer Use证据；v116 Swift Core61项/E2E241项通过、69忽略，v117动效2项通过。物理iPhone流畅度仍待用户验收。
- 图片发送需要配套包含图片传输功能的新Mac构建；本次未打包或替换运行中的桌面。真实Codex读图通过；当前Claude配置的`ark-code-latest`仅支持文字，需视觉模型才能理解图片。中文内测说明已列明这些限制；APNs部署仍延期。

Apple回读与产物：`target/norma-ios-testflight/v118/{build-final.json,group-builds-final.json,localizations-final.json,artifact.json}`；完整汇总：`target/norma-ios-evidence/v118/summary.json`。

## v107发布结果（历史，0.1.0（22））

- App ID `6810038463`，Bundle ID `com.norma-ai.os.ios`；Build ID `f1e19ac7-ccd2-410f-84c8-758183e8fa15`；原内测组 `1e9af1ec-bf35-4a49-b7fd-01169c421384`。中文测试说明、组关联、VALID / IN_BETA_TESTING及未过期状态均回读通过。
- iOS按用户明确要求重新构建为0.1.0（22），相对21仅构建版本配置变化。96项输入hash、Release归档/导出、严格签名、production entitlement、Apple校验及上传全部通过；沿用既有加密分类，没有新增邀请、外部测试或公开App Store发布。
- IPA：`target/norma-ios-testflight/v107/export-final/Norma.ipa`，5553296字节，SHA256 `a49dbba1dedeb3bf566998931d4964aa0b3888ccab2d532649c6679b1cf13962`。
- 桌面：`build/Norma OS_0.1.71_aarch64.dmg`，192791700字节，SHA256 `6f30c9ebae558dc5401ac294ad5242c054091118a2b195784ec5833dc314c8ce`；版本仍0.1.71，build由20260909.155747更新为20260910.115829。Developer ID签名、未公证；严格验签、Finder布局、hdiutil校验和相邻SHA256文件一致性通过。
- 桌面包含v106 Codex紧凑首屏/桌面回看就绪修复，5项修复输入hash保持一致；复用Runtime175通过/3忽略、完整E2E240通过/67忽略和真实Codex首条输入10.74秒通过的证据。未替换运行中的验证实例、迁移数据库或把真实iPhone验收标记完成；APNs继续延期。验收需配套更新桌面。

Apple回读与产物：`target/norma-ios-testflight/v107/{build-final.json,group-builds-final.json,localizations-final.json,artifact.json}`；桌面签名、构建日志、旧DMG备份和汇总：`target/norma-ios-evidence/v107/`。

更新时间：2026-09-10T17:56:16+08:00。当时状态：**0.1.0（21）已进入现有「Norma 内测」组，可安装**。

## v101发布结果（历史）

- App ID `6810038463`，Bundle ID `com.norma-ai.os.ios`；Build ID `f7ba7443-1929-4ed9-a6ae-5c9181311682`；原组 `1e9af1ec-bf35-4a49-b7fd-01169c421384`。
- 独立黑色方向编辑器、当前目标预填、优先事项、历史入口与保存后返回；模型接收真实原待办，结果稳定排序，另一端先发布时拒绝迟到建议。暂停隐藏页面订阅，修复方向提交后回查连接失败。
- Swift58、模型协议4、正式Mac计划流程5及方向UI3条通过；跨项目切换、三主页层级和创建权限通过。CUA打开/提交/回执/返回通过，原生菜单选项点击仍受工具限制；全仓1792通过/96忽略，排除既有音频调度失败，新增2条正式优先项E2E随后另跑通过。
- IPA：`target/norma-ios-testflight/v101/export-final/Norma.ipa`，5553297字节，SHA256 `d3e537a627ae34a36b11d822b1afcb2ce74d3721a6ac5703feb9b6c5e85292bb`。96生产输入、严格签名、Apple校验/上传及原组关联通过；未新增邀请、外部测试或公开发布。
- Mac已从v92切到v102并保留配对；v106追加修复Codex紧凑首屏的就绪误判，真实首条输入、完整Runtime/E2E与打包验签通过，等待正常退出v102后启用。iOS沿用构建21；真实iPhone/双Mac/LAN/Tailscale性能及完整验收尚未结束，APNs服务按用户决定延期。

Apple回读与产物：`target/norma-ios-testflight/v101/{build-final.json,group-builds-final.json,localizations-final.json,artifact.json}`；验收边界及初始失败记录：`target/norma-ios-evidence/v101/summary.json`。

更新时间：2026-09-10T16:55:51+08:00。当时状态：**0.1.0（20）已进入现有「Norma 内测」组，可安装**。Apple API回读确认VALID / IN_BETA_TESTING。

## v100发布结果（历史）

- App ID `6810038463`，Bundle ID `com.norma-ai.os.ios`；Build ID `ebef2af5-e1b6-4dc0-b14c-c8315f3b1ca0`；原组 `1e9af1ec-bf35-4a49-b7fd-01169c421384`。
- 卡片选项恢复原型黑色面板、查看／安排任务、删除确认；未决结果按原卡匹配，不再误锁其他卡；保留类型／可选名称创建及原卡聊天。
- 57项Swift测试、5条原生UI流程、Computer Use实际选项→删除确认→取消→原卡聊天通过；所有隔离配对正式清理通过。物理iPhone、双Mac/Tailscale与完整界面仍待验收。
- IPA：`target/norma-ios-testflight/v100/export-final/Norma.ipa`，5479429字节，SHA256 `0149fd177d4f016307daabf3b16af297fadecb013f077d49c51cc813cc0a3dcf`。96个生产输入冻结、严格签名、Apple校验及上传通过。
- Apple回读：`target/norma-ios-testflight/v100/build-final.json`、`group-builds-final.json`、`localizations-final.json`。中文测试说明一致，未新增邀请、外部测试或公开发布；加密实现和分发范围不变。

Mac仍运行v92；v99配套修复包等待既有切换确认，Codex就绪／Claude首次提交／真实摘要修复需要该Mac包。APNs服务按用户决定延期。本轮没有修改Rust生产代码，没有重新构建Mac。

更新时间：2026-09-10T09:48:43+08:00。当时状态：**0.1.0 (12) 已进入现有「Norma 内测」组，可安装**。Apple API回读确认VALID及IN_BETA_TESTING；用户负责发布后验收。

## v91发布结果（历史）

- App ID `6810038463`，Bundle ID `com.norma-ai.os.ios`；Build ID `61b61f0f-9f2b-429d-8806-90e5079877b1`。
- 沿用现有内测组`1e9af1ec-bf35-4a49-b7fd-01169c421384`，没有新增邀请、外部Beta或公开App Store发布。
- IPA：`target/norma-ios-testflight/v91/export-icon/Norma.ipa`，4914749字节，SHA256 `77ea375bf7131c68f8440d943e79a60a661ad19576cf0eec0a724e9897018613`。严格签名、Apple校验与上传成功；相同加密用途和不在法国分发范围沿用已确认分类。
- 配套[Mac DMG](../../build/Norma%20OS_0.1.71_aarch64.dmg)：0.1.71（build20260909.155747），192705407字节，Developer ID签名、未公证；SHA256 `b1c853e17b5f8df029b432a7e555c31f37f23474fb61c2786df20f841580f260`。本轮沿用已有包，运行桌面未重启。
- 本次统一三页空态/失败提示与主次按钮，并补全iOS图标白色方块及全出血背景。7项原生UI、完整E2E237通过/0失败/64忽略；隔离TLS/WSS、清理和最终IPA内图标检查通过。Computer Use因Mac锁屏未完成，真实手机效果由用户验收；APNs继续延期。

最终Apple证据：`target/norma-ios-testflight/v91/build-final.json`、`group-builds-final.json`与`localizations-final.json`。中文测试说明已保存并回读一致；后续按用户验收反馈推进。

## 首次准备记录（历史，已被v66替代）

| 项目 | 值 |
|---|---|
| 版本 | 0.1.0 (1) |
| Team | 2BF8G9AN4L |
| Bundle ID | com.norma-ai.os.ios |
| IPA | target/norma-ios-testflight/export/Norma.ipa |
| SHA-256 | 9bf630da3551fa323f20e2eae45a48b2e26c2eab9fe32ccdada90c417c3446fd |
| 签名 | Apple Distribution / App Store分发描述文件，严格校验通过 |
| 通知环境 | production |
| 上传状态 | 未上传；没有App Store Connect应用记录 |

App Store Connect入口：https://appstoreconnect.apple.com/apps 。首次创建填写iOS、名称Norma、简体中文、上述Bundle ID、SKU `norma-ai-os-ios`；不要创建另一Bundle ID。API无CREATE权限是该资源本身不支持此操作，不是缺少上传证书。Mac锁屏阻断了网页操作，已请求用户解锁或告知手动创建结果。

创建后继续使用环境中已配置的`APP_STORE_CONNECT_API_KEY_ID`、`APP_STORE_CONNECT_API_ISSUER_ID`、`APP_STORE_CONNECT_API_KEY_PATH`；不要输出私钥或将私钥拷入项目。API调用辅助脚本为`target/norma-ios-testflight/asc.py`，先按Bundle ID查询应用和已有build，避免重复上传。归档、导出、Apple校验日志分别为`archive-final.log`、`export.log`、`apple-validation.log`。Apple校验日志ERROR12表示缺少应用记录，尽管altool退出码为0，不能记为校验通过。

IPA含隐私清单及无透明通道图标。尚未声明出口合规：配对使用SPAKE2/ring AES-GCM，不能填“只有系统加密”；应按真实用途完成Apple加密问卷。上传后等待处理完成，再核验TestFlight状态；内部测试和外部Beta审核区分记录。未邀请测试人员，也未发布公开App Store。

本次仅改发布资源、重新归档和导出；未修改iOS业务逻辑。原用户桌面与模拟器保持原样。全Goal的手机归档聊天、APNs服务配置和统一真机/界面验收仍继续保留。

## 上传前复核

2026-09-08T22:33:11+08:00：收取先前应用查询结果，仍为零条；Computer Use再次报告Mac锁屏。IPA SHA-256与冻结记录一致，解包App的严格签名校验通过。当前Xcode自带altool帮助没有创建App记录命令；不重复调用已确认不支持的API创建操作。未重新构建或上传，先前解锁请求仍待回复。

已核对加密用途：`crates/mobile_pairing/src/lib.rs`使用SPAKE2的Ed25519群进行六位验证码交换，使用ring AES-256-GCM认证解密最多4096字节的配对信息（电脑身份、地址、证书pin和一次性secret）；任务正文通道使用URLSession系统TLS。不能将该内置Rust库描述为仅系统加密。上述是技术事实，并未提交豁免分类；根据[Apple出口合规说明](https://developer.apple.com/help/app-store-connect/manage-app-information/overview-of-export-compliance)，后续按后台实际问卷和计划分发地区填写。

用户最新决定：不再发布TestFlight，继续其他实施工作。撤销本次创建App记录、上传及处理TestFlight的待办；冻结安装包与已有证据保留，解锁后也不自动恢复发布。

## 2026-09-09 恢复发布（v66）

用户重新授权“可以尝试发布到TestFlight”，要求由Agent操作并指定Chrome；用户完成Apple登录后，已创建应用记录Norma，App ID `6810038463`，Bundle ID仍为`com.norma-ai.os.ios`，SKU `norma-ai-os-ios`、简体中文。初始访问选择有限范围，未邀请测试人员。

最新源码完成Release归档、App Store分发签名、导出与Apple校验，并成功上传0.1.0(1)。IPA：`target/norma-ios-testflight/v66/export/Norma.ipa`；SHA-256：`29913583c16b2bbe9d74b848202275c73178f6901bce5c1095a3c2a4046f07ab`。Apple Delivery/Build ID：`6a1df2de-b2d6-4e38-ac0c-da06aaa0f668`；API确认`processingState=VALID`；初次处理提示缺少出口合规，完成下述问卷后内部状态成为`IN_BETA_TESTING`。

加密问卷按真实内置SPAKE2/ring AES-GCM选择“标准加密算法，与Apple系统加密同时使用”，用户明确本次不在法国分发，已选否并保存。Apple保存`usesNonExemptEncryption=false`。没有改Info.plist声称仅系统加密。用户随后明确邀请当前登录账号，已通过Chrome发送一次邀请，界面与API均确认为`INVITED`；中文测试说明已保存。

相关代码和自动化结果见v66 Goal与证据；完整真机/跨网、APNs和界面验收未完成。新增计划实时订阅需配合本轮Mac构建，用户正在运行的桌面没有被重启。构建日志、签名证据、Apple校验/上传和API状态保存在`target/norma-ios-testflight/v66/`。

## 2026-09-09 连接修复版（v67）

0.1.0 (2)，Build ID `64d55d3b-2b57-4f01-abdd-4ab72669e8d9`，Apple `VALID`，内部 `IN_BETA_TESTING`；未提交外部 Beta 或公开 App Store。IPA：`target/norma-ios-testflight/v67/export/Norma.ipa`；SHA-256：`64cd0ad2bdfa7ffb7b19a46499e2ad8308f4e6ce2991fb7be29f7d8262e36d33`。中文说明已更新，沿用用户先前确认的相同加密分类和不向法国分发范围；没有修改加密算法。

iOS 改为并发验证已有的候选连接，仅在选出证书验证通过的连接后发送一次业务请求。当前 Mac 原为 v41 且漏带审批 Hook；按用户授权已备份并切换，保留配对身份。电脑直接输入的追加修正继续在 Mac 端完成，无需再次上传相同 iOS 构建。APNs 服务按用户最新决定暂不部署。


## 2026-09-09 原卡输入版（v69 iOS / v70 Mac）

手机直接将内容输入电脑原有Codex/Claude卡片，并读取同一会话回复；不另建执行会话、不再要求旧卡转为受控卡。首条消息/计划草稿允许run存在而session尚未建立，仍拒绝相反的不合法身份组合。停止入口按Mac能力显示；原生工具审批与停止回执尚未接通，中文测试说明明确目前需在电脑处理。APNs服务按用户决定暂不部署。

Release archive/export、分发签名、Apple validate/upload均成功；build3不重复上传。沿用此前确认的相同加密分类与不在法国分发范围，保存usesNonExemptEncryption=false；更新中文测试说明后加入同一内部测试组，API核实VALID及IN_BETA_TESTING。

Swift42项通过；Mac最后旧Hook兼容补丁不涉及iOS二进制，因此沿用已上传build3。全仓1755通过/73忽略；最新原生Codex/Claude各两回合经macOS Swift TLS/WSS通过。完整SwiftUI/物理iPhone与跨网验收仍未完成，不能将本地回环测试等同于用户设备效果。

## 2026-09-09 v84 · 直接交付构建5供用户验收

用户已再次明确：功能开发完成后直接发布到现有TestFlight，由用户安装后统一验收。0.1.0(5)完成Release归档、分发签名、Apple校验与上传，并进入现有内测组。Apple回读确认VALID和IN_BETA_TESTING。

- 创建卡片直接选择Codex/Claude；移除名称与二次确认；保留空卡不派任务。指挥台左右滑动决策和准确原会话入口已接入。
- 配套Mac修复Codex长Bash批准回执，并增加Claude原生Write完整预览审批。两种原卡审批中停止、Codex原卡Todo队列、撤回和重排已由真实CLI与Swift传输验证。未知或裁切原生菜单仍由电脑处理；不扩大授权或另建执行会话。
- 验证：Rust全仓1776通过/91忽略，Swift49通过；iOS原生设备测试57通过/4个需外部fixture测试跳过。真实Provider检查另外显式运行。Mac锁屏导致完整Computer Use未完成，物理iPhone、跨网、模型规划质量及完整界面由用户验收，不把模拟器证据当作真机。
- IPA：`target/norma-ios-testflight/v84/export-final/Norma.ipa`，4551866字节，SHA256 `85a5876b573df0ff8bd066f80b4548dd5e2f693d2031a38e4143ffc44372e785`。
- 配套Mac：`build/Norma OS_0.1.71_aarch64.dmg`，192663410字节，SHA256 `0c50fecacb4b90caf64a507154e39cdbd906cf97cd6aa5f733e31e00296ec3ee`；Developer ID签名、未公证，签名/DMG/Finder布局检查通过。App build为20260909.151202，打包未改manifest版本。首次打包缺少Sparkle公开验签key失败，复用已有正式App plist的同一公开key后成功，未创建或更换密钥。

运行中的v70桌面与会话保持原状。验收新增原卡能力须在现有任务结束后安装并打开配套Mac包；不会自动结束用户任务或重复启动争用同一手机服务。APNs服务仍不部署，没有新增邀请或公开App Store发布。


## 2026-09-10 v85 · 原终端与构建6交付

构建6于2026-09-10T00:12:14+08:00确认进入现有内部测试组。Build/Delivery ID `c0c9fdf0-2892-4093-8813-91f2f78a5390`，版本0.1.0、未过期、Apple VALID、内部IN_BETA_TESTING。中文说明已保存并回读一致。没有更换加密算法或分发范围，沿用此前用户确认的用途与不在法国分发，Apple保存usesNonExemptEncryption=false。

终端页复用电脑原卡PTY与现有命令账本。文字先填入、Enter再确认，支持Tab、Esc、退格、方向键和Ctrl-C；独立历史分页不移动电脑viewport，持续WSS只传变化行。原屏幕/输入序号保护双端并发，旧运行身份或只读授权不能写入；回执仅说明已写入，不冒充任务完成。纯PTY输出接回原事件唤醒后通过500ms同主机传输回归，物理网络效果仍待用户验收。

最终Rust全仓1778通过/93忽略、Swift50通过、隔离iOS58通过/4个fixture项跳过。真实Codex/Claude原终端在macOS Swift及隔离iOS分别2项通过。旧浏览器测试因33ms重绘预算早读渲染树失败，仅增加最多3×16ms等待，保留全部断言和生产浏览器代码后聚焦及全仓通过。归档后227个生产输入hash一致，所有失败和修复证据保留。

新Mac包替换此前同名构建产物，当前build20260909.155747；Developer ID签名、未公证。运行中的v70及其会话未中断，用户应在现有任务结束后安装配套Mac再验收新终端。Computer Use仍受Mac锁屏限制，物理iPhone、多电脑/Tailscale、云规划质量和完整页面验收由用户接手；APNs服务按决定不部署，Goal保持active。

## 2026-09-10 v86 · 工具卡片与构建7交付

2026-09-10T00:51:25+08:00回读确认构建7可供现有「Norma 内测」组安装。Build/Delivery ID `b092d15d-1a8d-45a1-8f0d-70c0bcb31093`，0.1.0(7)、VALID、IN_BETA_TESTING、未过期；组关联和中文说明准确。沿用已确认的相同加密用途及不在法国分发范围，保存usesNonExemptEncryption=false；算法未改，不声明只有系统加密。没有新增邀请、外部Beta或公开App Store发布。

本轮只改iOS工具消息渲染，支持Claude与Codex原有工具记录，默认紧凑、可展开完整输入/输出，保持原始记录和准确失败状态。调用与结果分别显示，不猜测跨消息配对。复用已有TextKit及协议，正常/最大辅助字号与700行输出在两项原生UI测试通过；Swift53通过，隔离iOS61通过/4个fixture跳过，Rust E2E237通过/64忽略。Computer Use因锁屏未完成，合成工具fixture与XCTest不代替用户真机验收。

最终归档为`target/norma-ios-testflight/v86/Norma-release.xcarchive`，IPA `target/norma-ios-testflight/v86/export-final/Norma.ipa`，4772518字节，SHA256 `9fc755ebb9ab2aabb99d24b009d20be0774642d85ab0dfe12967992f78be205a`；229个生产输入hash一致。Release归档/导出、严格分发签名、Apple validate/upload均成功。早期布局候选产物仅保留作历史，不是本次上传文件。

证据汇总`target/norma-ios-evidence/v86/summary.json`。沿用v85配套Mac0.1.71（20260909.155747），本轮未重建或替换运行中的v70，未操作用户原模拟器。APNs仍延期，后续由用户更新TestFlight验收。

## 2026-09-10 v87 · 项目名称置顶与构建8

2026-09-10T01:13:00+08:00确认0.1.0(8)已进入同一「Norma 内测」组，Build/Delivery ID `cf77c6ef-fe0e-412d-b95b-7bb0cee86923`，VALID / IN_BETA_TESTING、未过期，中文说明一致。沿用用户已确认加密用途与不在法国分发范围；无新增邀请、外部Beta或公开App Store发布。

项目名移到顶部，仍可点击切换，正文不再重复显示；普通字号聊天输入控件补44pt触控高度，修复本轮连续两次复现的无障碍告警。原生UI三项（含AX5）、TLS/WSS fixture及配对清理通过，真实Computer Use仍因Mac锁屏未完成。最终归档、签名、Apple校验与上传通过；IPA 4776794字节，SHA256 `c681e6dc29f44590719f59827ec907601ef76ec9314b0d37434aeb3f60aa76eb`，229个生产输入一致。旧before-composer候选仅作历史，未上传。

产物：`target/norma-ios-testflight/v87/export-final/Norma.ipa`；汇总：`target/norma-ios-evidence/v87/summary.json`。配套Mac沿用0.1.71（20260909.155747），未替换运行v70，APNs仍延期；实际手机效果交用户验收。


## 2026-09-10 v88 · 移除结果大按钮与构建9

项目卡片上方删除“查看操作结果”，原结果查询复用折叠区“记录与成果 → 操作记录”；保留防重复提交、查询回执和终态清理恢复创建。Apple设计原则落实为次要操作渐进展示、复用原生DisclosureGroup及44pt触控高度。

聚焦原生UI验收：Codex/Claude各创建1张空卡→第三次明确拒绝且无103卡→大按钮不存在→折叠/展开查询入口→读取真实拒绝→完成后恢复创建；成功创建记录始终只有2条。Rust隔离TLS/WSS、Runtime/SQLite fixture和设备清理通过。红测准确捕获旧大按钮与缺少折叠入口；首轮绿测遇到原生菜单点击时机问题，补充明确可点击等待后通过。Release归档/导出、签名、229个生产输入hash、Apple校验和上传均通过。

TestFlight0.1.0(9)已VALID/IN_BETA_TESTING并关联原内测组；中文说明回读一致。产物与Apple证据：`target/norma-ios-testflight/v88/`；验收汇总：`target/norma-ios-evidence/v88/summary.json`。Computer Use仍因Mac锁屏未完成，自动化和截图仅作补充；完整Goal保留阻塞状态，APNs延期，运行中的桌面未重启。


## 2026-09-10 v89 · 电脑信息置顶与构建10

电脑名称移到项目标题第二行，左上角复用ComputerAccessButton打开现有电脑列表；正文删除独立电脑信息行。长名称单行中间省略，VoiceOver保留完整项目/电脑身份；最近同步时间收进刷新按钮的无障碍值。Apple Design落实为原生导航分组、内容优先和清晰的主次字体，保留44pt点击目标。

旧页面红测准确捕获4条顶部/正文定位失败。修改后3项原生UI通过：顶部电脑选择、双项目切换/关闭态/指挥台上下文，普通和最大辅助字号下项目与聊天的文字裁切/触控/描述审计及五行输入；隔离TLS/WSS和清理通过。普通及AX5截图已检查，Computer Use实时读取仍返回Mac locked，未完成人工式点击验收。

TestFlight0.1.0(10)已VALID/IN_BETA_TESTING并关联原内测组，中文说明回读一致。产物：`target/norma-ios-testflight/v89/export-final/Norma.ipa`；汇总：`target/norma-ios-evidence/v89/summary.json`。相对构建9仅ProjectCardsView与版本配置变化，229个生产输入hash一致，签名/Apple校验/上传通过。运行桌面未重启，APNs延期，整体Goal仍保留未完成验收。

## 2026-09-10 v90 · 三页顶部统一与构建11

2026-09-10T08:55:24+08:00确认0.1.0(11)可供现有内测组安装。Build/Delivery ID `b13b1cbd-642f-415c-99df-d320d3308c65`，VALID / IN_BETA_TESTING、未过期，组关联及中文说明回读一致；沿用此前用户确认的相同加密用途与不在法国分发范围。

项目、指挥台、任务共用双行顶部标题和左侧电脑入口；任务“所有项目”及指挥台项目切换移到顶部，右侧保留对应操作。全局决策继续汇总全部电脑。5项最终原生UI覆盖普通/AX5布局及触控审计、项目范围、计划/Todo、任务实时筛选和原会话返回；隔离TLS/WSS与清理通过。修复任务裁切与方向文字触控区域，红测和诊断失败日志均保留。

Release归档/导出、严格签名、Apple校验/上传及229个生产输入hash一致性通过。IPA：`target/norma-ios-testflight/v90/export-final/Norma.ipa`，4785000字节，SHA256 `1027f24cdbda4acfb12d62a0e3587ca4a1b28f2688aba84dc5846302dc19dec2`。汇总：`target/norma-ios-evidence/v90/summary.json`。Computer Use因Mac锁屏未完成，完整物理设备/跨网/界面验收由用户接手；桌面未重启，APNs延期，整体Goal仍保留阻塞状态。

## 2026-09-10 v91 · 提示布局、图标补全与构建12

2026-09-10T09:48:43+08:00确认0.1.0(12)进入原内测组，Build/Delivery ID `61b61f0f-9f2b-429d-8806-90e5079877b1`，VALID / IN_BETA_TESTING、未过期，组关联及中文说明回读一致。沿用此前确认的相同加密用途和不在法国分发范围。

三页统一空态/故障提示、相邻主次按钮及恢复路径；普通字号居中，AX5可滚动触达。内置image_gen补全iOS品牌图标的两个白色方块、四角背景，移除预制圆角和外侧黑边。母版保存在assets/app-icon-ios-source.png；正式图标为1024×1024不透明PNG，已查看最终IPA内120px图标。没有修改Mac图标。

最终原生UI7项通过，包含普通/AX5状态恢复、顶部、项目范围、计划、任务和会话返回；隔离TLS/WSS及清理通过。完整E2E237通过/0失败/64忽略。图标替换前后230个冻结输入仅AppIcon.png变化，沿用已通过的Swift流程证据；含新版图标的最终包重新归档、签名、Apple校验和上传均通过。无新图标的旧候选未上传，保留作历史。

IPA `target/norma-ios-testflight/v91/export-icon/Norma.ipa`，4914749字节，SHA256 `77ea375bf7131c68f8440d943e79a60a661ad19576cf0eec0a724e9897018613`。汇总`target/norma-ios-evidence/v91/summary.json`，提示词`target/norma-ios-evidence/v91/icon/prompt.txt`。Computer Use锁屏限制和完整Goal验收未完成项保留；桌面及用户原模拟器未操作，APNs延期。

## 2026-09-10T11:58:22+08:00 v92 · 聊天修复与构建13

TestFlight0.1.0(13)已VALID / IN_BETA_TESTING，Build/Delivery ID `ab689efb-2998-4ffa-a8ff-f74bd94883ae`，原内测组和中文说明回读一致。沿用用户已确认的相同加密用途和不在法国分发范围，没有新增邀请或公开App Store发布。

聊天/终端原生收键盘，准确成功回执清空已发送草稿；相同文字回写不再产生新版本，发送不重建订阅。聊天自动跟随同ID增长，用户向上阅读暂停跟随，可一键回到最新。真实原TUI长回复先更新屏幕、后写历史，因此生成期间以原PTY的持续WSS增量显示最近24行预览，完成后呈现正式消息/工具卡，保留原执行器和完整记录，不伪造结构化token流。

Swift54通过；完整E2E237通过/0失败/64忽略；4项聚焦原生UI通过；真实Claude/Codex长回复时序各1项通过。Computer Use成功打开原卡并看到实时输出替换/最终正式消息；键盘输入仍受工具限制，未声明完整CUA键盘验收。失败日志和隔离配对正式清理证据保留。

Release归档/导出、严格签名、Apple校验/单次上传及231个生产输入hash一致性通过。IPA `target/norma-ios-testflight/v92/export-final/Norma.ipa`，4978592字节，SHA256 `e5c34c6f9d806f20ca3a4267130305d4bc5d24b48e48f3f7d1ed38d226d6756c`。汇总 `target/norma-ios-evidence/v92/summary.json`。

用户明确同意后，Mac从v70切到配套v92验证包，PID37820，原数据目录和配对记录保留、切换前后一致性备份。用户已开启手机服务并打开项目；LAN/Tailscale监听经同Mac TLS握手确认使用相同证书。物理手机的实际联网效果由用户更新后验收，APNs仍延期，整体Goal保持active。


## v94 · v28 原型还原与构建14交付

TestFlight **0.1.0(14)** 已由Apple处理为VALID / IN_BETA_TESTING，Build/Delivery ID `64119766-58d6-4011-90e9-15c756ab5176`，原「Norma 内测」组关联和中文测试说明均回读一致。沿用此前确认的加密实现和不在法国分发范围，没有新增邀请、外部Beta或公开上架。

项目、指挥台和任务恢复v28黑色视觉、norma/电脑品牌头部、居中N悬浮导航、极简Agent卡与简报/任务卡布局；Todo从简洁列表进入单项详情，保留真实决定及错误反馈。Codex/Claude矢量资产转PDF，保留运行动效和Reduced Motion。任务接口尚无耗时/阶段计数，未以原型固定示例充当实际进度。

12项受影响UI流程通过，包括三页层级、普通/最大辅助字号导航、项目切换、Todo安排/撤回/重排/完成/拒绝、直接创建、任务筛选、键盘、实时输出与自动跟随；Swift54通过，完整Rust E2E237通过/0失败/64忽略。UI服务使用隔离真实TLS/WSS与存储，Agent内容为显式测试fixture；5轮正式流程的配对清理全部通过。早期失败和原生菜单点击诊断保留，语义tap偶发未选中，改用观察到的菜单行中心点击且断言实际项目值变化后通过，未降低选择/权限断言。

Release归档/导出、91个iOS生产输入一致性、严格签名、Apple校验与单次上传通过。IPA `/Users/alan/Documents/mybook/norma-ai-os/target/norma-ios-testflight/v94/export-final/Norma.ipa`，5229540字节，SHA256 `61d03d6cd45ea4a726839a1067925c3c6c7a4aab9d387ff3b05af513353227ba`。证据 `target/norma-ios-evidence/v94/summary.json`，最终截图 `projects-final.png` / `command-final.png` / `tasks-final.png`。

Computer Use两次返回Mac locked，已请求解锁，本轮真实点击未完成；XCTest和截图不替代该验收。按用户持续授权交付TestFlight用于真机验收，运行Mac v92与用户原模拟器未操作，APNs仍延期，整体Goal保持active。

## 2026-09-10 · v99 · 构建19

- v99发布完成：TestFlight 0.1.0（19），build 633471ea-0bd7-4624-997d-bccc489987a6，Apple回读VALID / IN_BETA_TESTING，原内测组已关联且中文说明一致。IPA 5429361字节，SHA256 eaba89f3b5830e7940d4ac633bb8d8a14c2b70f6225c002322f31ec0fb27b0ad；未新增邀请/外部测试/公开发布，既有加密与法国分发范围不变。Mac v99已准备但未切换，当前v92 PID37820保留。Goal继续active，真机/跨网多机与完整原型验收未结束，APNs仍按用户要求延期。

验证及已知音频测试失败详见 `target/norma-ios-evidence/v99/summary.json`。发布回读：`target/norma-ios-testflight/v99/build-final.json`、`group-builds-final.json`、`localizations-final.json`。


## v102 · 配套Mac连接释放修复（iOS沿用构建21）

本轮修改服务端watch关闭/Ping与事件等待，不修改iOS生产代码。已逐一核对构建21的96项生产输入，hash完全一致，证据`target/norma-ios-evidence/v102/ios-production-comparison.json`；不上传内容不变的构建22。

配套本地验证包：`target/norma-mobile-desktop/v102/Norma Mobile Validation.app`，build102，ad hoc本地签名，严格验签通过，未启动。它在既有隔离验证bundle上更新两个Release可执行文件并重新签名，保留验证bundle身份/资源/数据路径；不是对外发布的DMG。二进制hash与验签证据在同目录`prepared.json`和`codesign-verify.log`。

仍运行v92（PID37820）；既有切换确认未答复，不中断Agent。iOS原生双正常地址＋一条停滞地址、8条订阅满额下24次取消/重建通过，P95 26.13ms、最大28.32ms，仅模拟器回环。单/双地址方向页面提交/回执/返回及正式清理通过；物理多机/跨网/完整界面/空闲开销验收仍未完成，APNs继续延期。

## v124 · TestFlight 0.1.0（25）发布（2026-09-11）

2026-09-11T18:22:31+08:00确认构建25已由Apple处理为VALID / IN_BETA_TESTING，原「Norma 内测」组关联与中文说明回读一致。Build/Delivery ID `796e428a-34b5-4a41-9577-027f79530f34`。本轮包含v120–v123的加载居中、终端软折行/竖屏按键、底栏距底边8pt。沿用已有加密实现和不向法国分发的确认，未新增邀请、外部测试或公开App Store发布。

Release归档、导出、严格签名、Apple校验/上传通过；103项发布输入及IPA上传后hash一致。IPA：`target/norma-ios-testflight/v124/export/Norma.ipa`，5679727字节，SHA256 `5bee930651634070b9fa2e76e10539dfd6f192a565354dc202cabd5e4d063400`。复用v121加载布局、v122 Rust536/Swift62及原生竖横屏/AX5/键盘、v123 Computer Use三页导航验证；未把v122工具点击故障当作CUA通过。

测试说明已注明完整终端软折行重排需配套新版Mac；旧Mac仍按原行显示，本次未打包/替换运行中的桌面。APNs仍延期，真机/LAN/Tailscale由用户更新后验收。完整证据`target/norma-ios-evidence/v124/summary.json`及`target/norma-ios-testflight/v124/`。

## 2026-09-15 · 聊天输入恢复与 TestFlight 构建40

TestFlight **0.1.0（40）** 已回读为 VALID / IN_BETA_TESTING，Build/Delivery ID `e021e8a9-1812-487e-ba1a-509fc4793511`；原「Norma 内测」组与中文说明均核对通过。沿用已确认的加密用途及分发范围。构建39为上传后的候选版本，最终交付以40为准。

相对已发布构建38，生产改动仅在 `ConversationView.swift`：项目失联保留卡片身份，避免草稿切回旧run、输入框重建与焦点丢失；项目和历史分别处理恢复，历史重新验证期间保留已读取内容，收到有效首屏后再替换；加载时允许主动重读，重试耗尽时清除恢复提示。发送仍须取得新快照并验证权限，未知发送结果仍禁止重复提交。Apple设计原则落实为保留原生编辑焦点、立即呈现输入、区分编辑能力和发送权限。

Swift Core 81项通过；原生TLS/WSS回归覆盖冷卡恢复运行、断线继续输入、短暂恢复的新历史、再次断线至重试耗尽、主动重读、草稿/键盘及发送权限，61.74s通过。原生明确拒绝/未知回执/重启/用户重试流程45.89s通过。Computer Use实际点击原卡、输入、失联继续编辑、核对耗尽后无恢复提示，并通过系统主页返回验证草稿、新正文及发送恢复；全部隔离配对正式清理通过。相关红测保留，未将模拟器证据描述为物理手机或公网网络测试。

Release归档、导出、严格签名、Apple校验/上传及130项生产输入hash一致性通过。IPA `target/norma-ios-testflight/rt-build40/export/Norma.ipa`，82577451字节，SHA256 `5f0df3968183d59e59016f7b2d39bb70615f3d993c3d06fdd389683e556677ac`。发布回读、测试日志、Computer Use状态与截图见同目录 `summary.json`、`acceptance.json`、`cua-recovered.png`。未更换运行中的Mac或语音服务器。

可重复执行本次回归：在既有隔离模拟器上设置 `NORMA_IOS_SIMULATOR`、`NORMA_NOTIFICATION_UI=1`、`NORMA_DECISION_UI=1` 与 `NORMA_SWIFT_PROBE=$PWD/scripts/check-norma-ios-chat-recovery.sh`，运行 `cargo test -p e2e --test mobile_notification_native -- --ignored --nocapture`。


---
