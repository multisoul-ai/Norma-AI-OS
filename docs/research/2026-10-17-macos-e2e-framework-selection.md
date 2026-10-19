# Norma OS macOS 真 App E2E 框架选型

> 调研日期：2026-10-17。仓库基准：`origin/dev` 的 `8ca7be72`（隔离 worktree）；以下为方案判断，尚未运行框架 PoC。

## 决定因素

Norma OS 是 GPUI 0.2.2 + Metal 的 macOS 原生 `.app`，核心交互在无限画布和 Terminal 卡片。旧 `crates/e2e` 已移除；`TestAppContext` 不能证明真实 `.app` 的窗口、系统菜单、输入、PTY 和像素行为。仓库要求页面与视觉状态仍由 Computer Use 操作真实 App 验收。[R1] 仓库已有 26 个 iOS `UITests/*.swift` 文件，说明 Swift/XCTest 有局部经验，但它们不是 macOS GPUI 测试宿主。[R5]

当前最关键的限制是**可访问性树**：仓库的 L3 脚本写明 Launchpad 的 Open Project 按钮没有作为 AX button 暴露，因而使用 CGEvent 坐标点击；对 vendored GPUI 0.2.2 与 app/ui 源码的 `NSAccessibility`、`AXUIElement`、`accessibility` 检索没有发现画布语义节点实现。后者只是源码静态证据，须用 Accessibility Inspector 或 AX tree dump 检查运行中的 `.app`。GPUI 的 `debug_selector` 在非 `test-support` 构建是 noop，不能当作真实 `.app` 的定位 ID。[R2][R3] Apple 明确要求自绘控件提供可访问性属性、动作和通知；这是产品可访问性需求，也是稳定语义定位的前置条件。[A1]

## 方案比较

| 方案 | 适合本项目的能力 | 成本与当前限制 | 判断 |
| --- | --- | --- | --- |
| **XCTest / XCUIAutomation（Swift）** | `XCUIApplication(url:)` 可指向指定 `.app`，支持启动、等待、菜单/窗口元素查询、键鼠、相对元素坐标、拖拽和截图；`xcodebuild test` 产出 `.xcresult`，便于调试。[A2][A3][A4] | 需维护一个小 Xcode UI test 宿主和 Swift 用例；画布缺 AX 节点时只能退回窗口相对坐标，文字/焦点无法直接做稳定元素断言。TCC 与 CI 桌面会话须实测。 | **首选 PoC**：Apple 原生工具，少一层服务；尤其适合菜单、窗口、快捷键和少量画布冒烟。 |
| **Appium Mac2** | 官方维护的 macOS driver，底层仍是 Apple XCTest；支持 `.app` 路径、bundle ID、W3C 客户端、元素定位、坐标点击/拖拽。2026-10-11 发布 v4.4.0，仓库未归档；3.x 起需 Appium 3。[M1][M2][M3][M4] | 还需 Node/Appium server、Mac2 driver、Xcode Helper 的 Accessibility 授权、WebDriverAgent 与客户端；Mac2 文档明确不鼓励并行，当前只支持单 session。底层仍读 XCTest 的元素树，不能绕过 GPUI AX 缺口。[M2][M5] | 已有 WebDriver 技术栈或需要跨语言远程驱动时再选；本仓库单一 macOS App 暂无足够收益。 |
| **Swift AXUIElement + CGEvent / 截图的轻量 runner** | 可按 PID 获取应用 AX 根节点、读属性/执行动作；对不可见画布节点用 CGEvent 坐标和键盘兜底。现有 L3 脚本已证明部分菜单/窗口与坐标操作可行。[A5][R2] | 要自己写等待、定位、结果收集、截图、失败诊断与进程清理；AX client 要 Accessibility 信任。缺 AX 节点时同样不能读画布语义。[A5] | 用于**首个 AX 诊断 PoC**和极少量已有 L3 冒烟，不宜先建设完整框架。 |
| **Computer Use** | 已是仓库页面/样式真实 App 验收的指定方式，可检查动画、主题和视觉状态。[R1] | 适合交互检查与视觉判断；作为持续回归套件的确定性、机器可重复断言、CI 接入能力仍需另行验证。 | 保留为视觉验收；与自动回归互补。 |
| **现有 AppleScript / System Events + CGEvent** | 已有菜单、窗口、PTY 输入、截图的 L3 脚本，可直接扩充一次性烟测。[R2] | 固定布局偏移和 sleep 易随窗口尺寸、主题、异步加载变化；调试/断言能力需自建。 | 过渡方案，不作为新框架主线。 |

**主推 XCTest / XCUIAutomation**。Apple 的 `XCUIApplication(url:)` API 接受文件系统中的应用 URL，因此可设计独立 Xcode UI test target 去测试 Rust 打包的 `.app`；本机 Xcode 的 `XCUIApplication.h` 还明确将此初始化方法限定于 macOS / Mac Catalyst。[A2] 但**本仓库的打包产物、签名、同 Bundle ID 副本、TCC 与 CI 是否能完整跑通仍属 PoC**，不能据 API 文档宣称已验证。推荐顺序是先对打包 `.app` 做 AX tree PoC；再建最小 XCUITest 宿主，验证按**绝对 `.app` 路径**启动一个隔离实例，并跑“菜单/快捷键建 Terminal → 输入确定性命令 → 检查文件副作用和可见画面 → 关闭/重开窗口”的一条纵向用例。若 AX tree 只有窗口/菜单，则画布动作只能暂用窗口相对坐标，画布文字/焦点也无法靠元素查询稳定断言；应优先补 GPUI 可访问性。换到 Appium 不会消除这项限制。[A2][A3][A1][M1]

## PoC 验收门槛

1. **定位能力**：在真实 `.app` 上记录 AX tree：窗口、系统菜单、Open Project、画布节点、Terminal 输入区分别有哪些 role、label、identifier、bounds；XCUITest 的 `debugDescription` 与 AX dump 对照。必须区分“可见像素”和“可查询元素”。[A1][A3][R2]
2. **隔离与实例**：使用非 `production-data` 构建设置 `NORMA_TEST_DATA_ROOT` 指向临时绝对路径；验证数据库和真实用户数据分离。用 `XCUIApplication(url:)` 指向目标 `.app`，验证同 Bundle ID 的其他安装副本不会被选中。[R4][A2]
3. **用户链路与 oracle**：菜单或快捷键创建真实 Terminal，执行向临时 workspace 写唯一 marker 的 shell 命令，以文件存在证明输入经过 PTY；用 Computer Use 检查真实画面中的卡片和文字，并保存截图证据。画布文字若没有 AX 值，不能把文件副作用或截图文件存在冒充渲染断言。[R1][R2][A4]
4. **权限与 CI**：在干净 macOS 用户上分别记录原生 XCUITest runner 的权限提示、Appium 的 Xcode Helper Accessibility 要求，以及 AX runner 的 Accessibility 信任状态；截图/Automation 相关 TCC 也按实际运行结果记录。先在有登录桌面的专用 Mac 跑串行用例，再验证目标 CI runner。不要预设云 runner 的 GUI、TCC 或并行行为。[M2][M5][A5]
5. **调试与稳定性**：记录 `.xcresult`、失败截图、AX tree、App 日志和目标 PID；对启动/异步 UI 用条件等待，对相对坐标在缩放、窗口大小、浅/深色和多屏下做少量验证。[A3][A4]

## 一手来源

- [A1] Apple [`NSAccessibilityProtocol`](https://developer.apple.com/documentation/appkit/nsaccessibilityprotocol.md)：自定义 UI 的属性、动作、通知与 `NSAccessibilityElement`。
- [A2] Apple [`XCUIApplication`](https://developer.apple.com/documentation/xcuiautomation/xcuiapplication.md)：`init(url:)`、启动、激活、等待、状态；本机 Xcode 的 `XCUIAutomation.framework/Headers/XCUIApplication.h` 第 36–45 行明确限定 `initWithURL:` 用于 macOS / Mac Catalyst。
- [A3] Apple [`XCUIElement`](https://developer.apple.com/documentation/xcuiautomation/xcuielement.md) 与 [`XCUICoordinate`](https://developer.apple.com/documentation/xcuiautomation/xcuicoordinate.md)：查询、键鼠、坐标与拖拽。
- [A4] Apple [`XCUIScreenshotProviding`](https://developer.apple.com/documentation/xcuiautomation/xcuiscreenshotproviding.md) 与 [Xcode 测试结果](https://developer.apple.com/documentation/xcode/running-tests-and-interpreting-results.md)：截图、`xcodebuild test`、`.xcresult`。
- [A5] Apple macOS SDK `ApplicationServices.framework/Frameworks/HIServices.framework/Headers/AXUIElement.h`：`AXIsProcessTrustedWithOptions`、`AXUIElementCreateApplication(pid)`、`AXUIElementCopyAttributeValue`、`AXUIElementPerformAction`。可在本机 Xcode SDK 核对。
- [M1] Appium [Mac2 README](https://github.com/appium/appium-mac2-driver/blob/master/README.md)：基于 XCTest；v3 起要求 Appium 3。
- [M2] Appium Mac2 [Getting Started](https://github.com/appium/appium-mac2-driver/blob/master/docs/getting-started/index.md)：macOS/Xcode 版本、安装、Xcode Helper 权限。
- [M3] Appium Mac2 [Capabilities](https://github.com/appium/appium-mac2-driver/blob/master/docs/reference/capabilities.md)、[Locator Strategies](https://github.com/appium/appium-mac2-driver/blob/master/docs/reference/locator-strategies.md)、[Execute Methods](https://github.com/appium/appium-mac2-driver/blob/master/docs/reference/execute-methods.md)。
- [M4] Appium Mac2 [v4.4.0 release](https://github.com/appium/appium-mac2-driver/releases/tag/v4.4.0)（2026-10-11）、[GitHub repository metadata](https://api.github.com/repos/appium/appium-mac2-driver)（2026-10-17 检查：`archived=false`）。
- [M5] Appium Mac2 [Parallel Tests](https://github.com/appium/appium-mac2-driver/blob/master/docs/guides/parallel-tests.md)：AX/HID 排他与单 session 约束。
- [R1] 本仓库 `AGENTS.md` §2、§5（`8ca7be72`）：旧 e2e 移除、真实 App 和 Computer Use 验收规则。
- [R2] 本仓库 `scripts/run-terminal-autosuggestion-l3.sh` 第 42–83 行及其他 `scripts/run-*-l3.sh`（`8ca7be72`）：PID、System Events、CGEvent、截图与 AX 缺口。
- [R3] 本仓库 `vendor/gpui-0.2.2/src/elements/div.rs` 第 705–720 行（`8ca7be72`）：`debug_selector` 非测试构建 noop。
- [R4] 本仓库 `crates/app/src/database.rs` 第 8、39–83 行（`8ca7be72`）：`NORMA_TEST_DATA_ROOT` 的非生产数据隔离逻辑。
- [R5] 本仓库 `rg --files ios/UITests | rg '\.swift$' | wc -l`（`8ca7be72`）：26 个 iOS UI test 文件。
