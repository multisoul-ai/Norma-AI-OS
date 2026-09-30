# 浅色启动首页与 Norma 单聊主屏：实施与验收记录

更新：2026-09-29。分支：`codex/norma-ios-companion`。本记录覆盖本轮首页替换及单聊入口纠正，**不代表 #54–#58 Goal 完成**。

## 已落地

- 冷启动直接显示 P01 浅色想法库；底部为「工作空间 / Norma / 收件箱」。已配对电脑的后台连接失败不会遮挡本机想法首页。电脑管理从更多模块进入。
- Norma 单聊以 W07 角色主屏为入口；键盘按钮在同屏切换私聊历史和输入，不再提供独立文字模块。通知目标与邀请深链复用同一私聊模型。
- Workspace 有独立的本机配置、创建、搜索、改名、归档和重开；设置中提供项目与 Agent 关联入口。09:29 的已运行版本中卡片进入设置；后续已改为进入线程列表，另提供明确设置入口。后续改动的状态见[线程接续验收记录](2026-09-29-norma-ios-thread-acceptance.md)，下表旧运行结果不能代替新流程验收。
- 收件箱暂承接既有真实待处理事项与任务提醒设置。旧深色任务首页入口已移除，完整注意力聚合仍属于 #60。电脑内的旧项目详情保留显式关闭操作，尚未完成该详情的浅色迁移。

## 原生测试证据

执行环境：iPhone 17 Pro Max Simulator，iOS 26.2，Xcode 原生 App。不是网页或 SwiftUI Preview。

以下公共参数配合表内 `-only-testing` 范围执行：

```sh
xcodebuild -project ios/Norma.xcodeproj -scheme Norma \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro Max' \
  -derivedDataPath ios/DerivedData \
  -packageCachePath /Users/alan/Documents/mybook/norma-ai-os/target/xcode-packages \
  -only-testing:<下表测试范围> test
```

| 范围 | 结果 | 原始证据 |
|---|---|---|
| `NormaUITests/CloudChatFlowTests`、`IdeaLibraryFlowTests/testAppLaunchesIntoIdeaLibrary`、`WorkspaceLibraryFlowTests`、`ConnectedComputersFlowTests/testConnectedComputersUsesLightStatusBarBackground` | 4 项通过 | `ios/DerivedData/Logs/Test/Test-Norma-2026.09.29_09-18-37-+0800.xcresult` |
| 修正 W07 导航卡、P04 按钮色后的 `CloudChatFlowTests`、`WorkspaceLibraryFlowTests` | 2 项通过 | `ios/DerivedData/Logs/Test/Test-Norma-2026.09.29_09-26-15-+0800.xcresult` |
| `NormaDeviceTests/CredentialVaultTests` | 同一 Keychain 读写隔离测试在 iOS 通过 | 同上 |
| `NormaUITests/IdeaLibraryFlowTests/testTextIdeaIsOfflineEditableSearchableAndPersistent` | 单独复测通过；新增、编辑、搜索、重启恢复 | `ios/DerivedData/Logs/Test/Test-Norma-2026.09.29_09-28-15-+0800.xcresult` |
| 最终组合：`CloudChatFlowTests`、`IdeaLibraryFlowTests/testAppLaunchesIntoIdeaLibrary`、`IdeaLibraryFlowTests/testTextIdeaIsOfflineEditableSearchableAndPersistent` | 3 项通过；复跑原顺序未再出现意外切页 | `ios/DerivedData/Logs/Test/Test-Norma-2026.09.29_09-29-27-+0800.xcresult` |
| `swift test --disable-sandbox --package-path ios --cache-path target/swift-cache` | 97 项中 96 项通过；Mac 测试进程的 `CredentialVaultTests` 抛出 `keychain(-67674)` | `target/norma-ios-evidence/goal-01-05/new-shell-20260929/core-tests.log` |
| `git diff --check` | 通过 | 本轮终端检查 |

09:26 组合测试中，文字想法用例先检测到记录入口，随后页面切到 Norma 文字模式而失败。单独复测及随后组合复测均通过，未修改代码或降低断言来重试；该次切页原因未确认，保留失败录像与元素树，不能把该次运行记为全绿。Keychain 在 iOS 的通过也不改写 Mac 测试的失败结果；未修改凭据保护策略来让测试通过。

## 截图与独立复核

全部原始截图为 1320 × 2868。对照时只比较参考图的屏幕内容；原型外壳、系统状态栏和空库/通话中的真实状态差异分别记录，不把演示文案作为执行结果。

| 页面 | 截图 | 只读 UI Agent 结论 |
|---|---|---|
| P01 空首页 | `target/norma-ios-evidence/goal-01-05/new-shell-20260929/1AE1E209-03E5-4874-B996-B79DEFE113D3.png` | 对照本轮用户图二，构图、空态与三项底栏 PASS |
| W07 角色主屏 | `target/norma-ios-evidence/goal-01-05/new-shell-review-20260929/FCC548AB-C028-44BC-879D-918F8E46A6E2.png` | 单聊结构 PASS；初审要求恢复上下文导航卡，修正后该项 PASS |
| W07 同屏文字模式 | `target/norma-ios-evidence/goal-01-05/new-shell-review-20260929/9D261816-9542-49AD-AB6E-6FB873A1F982.png` | 同屏输入结构 PASS；当前为未受邀状态，不作为真实云服务调用证据 |
| P04 空配置 | `target/norma-ios-evidence/goal-01-05/new-shell-review-20260929/8A0FD1E3-DAF0-4AD6-9C06-E8B85BF6CD0B.png` | 添加按钮操作蓝修正后 PASS；真实项目/Agent 关联流程尚待验收 |
| P01 文字记录 | `target/norma-ios-evidence/goal-01-05/new-shell-ideas-20260929/F15E9771-ED1C-4CD8-886C-DEADD1E54AB5.png` | 新增后真实截图；本轮未单独复核该填充状态 |

只读复核者：`/root/new_shell_ui_review`。前三项代码问题已修：通知独立 hosting 的模型传递、收件箱进入旧任务页的无返回路径、后台连接错误弹窗打断首页。复核结论按覆盖范围记录，不能扩写为全产品 PASS。

Apple 设计原则的落实：系统字体与安全区；正文白底；导航卡及底栏使用 iOS 26 系统 `glassEffect`，旧系统采用系统材质；减少透明度时改为实底；保留 44pt 以上主要操作区域与可见按下反馈。动态字号、VoiceOver、键盘出现后的全部布局以及真实手势仍需后续完整复核。

## Norma 素材来源

- 生成方式：本轮内置 `imagegen`。不替换真实通话中的原有角色驱动；新增静态素材用于未通话的 W07 主视觉。
- 身份参考：`ios/App/Assets.xcassets/NormaPortrait.imageset/norma-portrait.png` 与 `docs/brand/norma-os-brand-guidelines.md`。
- 构图参考：`docs/product-specs/norma-ios-prototypes-2026-09-26/W07-norma-call.png`。
- 原始输出：`/Users/alan/.codex/generated_images/01a0d8e3-9017-7133-956d-b993337d2b24/exec-9c3dca49-0fc9-4174-9b9e-34866b3f5b88.png`。
- App 资源：`ios/App/Assets.xcassets/NormaCompanion.imageset/norma-companion.png`。

生成需求的复现提示词（按本轮要求整理，非工具调用原文逐字转录）：

> 为 Norma iOS 浅色角色主屏生成透明背景的半身人物素材。保持 Norma 已获批的紫色头发、白色 X 发夹、黑色蝴蝶结及深蓝服装身份；参考 W07 半身构图，角色面向用户，伸出一只手，表情亲切自然。人物轮廓清晰，适合白色或极浅灰背景，头顶、手和发梢保留空间。仅绘制人物，不生成文字、按钮、手机外壳或界面。不要沿用 W07 示例中的黑色头发来替代品牌人物。

## 尚未通过的门槛

Computer Use 尝试选择 Simulator 返回 `Computer Use was not approved to use Simulator`，本轮没有绕过该权限；XCTest 的操作、录屏和截图不等价于 Computer Use PASS。真机 Apple Speech、扫码与双端授权、真实云服务、跨项目关联、P05/W04 工作线程均未在本轮闭环；Goal 保持进行中。
