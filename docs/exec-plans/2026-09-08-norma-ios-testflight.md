# Norma iOS TestFlight发布记录

更新时间：2026-09-13。当前状态：**0.1.0（30）已进入现有「Norma 内测」组，可安装**；Apple 回读确认 VALID / IN_BETA_TESTING、未过期，中文说明一致。

配套桌面已重新打包：`build/Norma OS_0.1.71_aarch64.dmg`，版本0.1.71，新构建20260912.154818，包含手机后台项目功能。192855370字节，SHA256 `576d05168af1021f2820891e572ebd63b950473d606a6e34549c13dbe6d23046`；Developer ID签名、未公证，严格签名/DMG完整性/Finder布局/相邻校验和通过。manifest 版本未改；未安装或启动新包。证据见 `target/norma-ios-testflight/background-build29/desktop-artifact.json`。

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
