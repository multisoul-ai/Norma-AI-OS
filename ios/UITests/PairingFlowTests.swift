import XCTest

final class PairingFlowTests: XCTestCase {
    /// 原型主导航保持项目/指挥台/任务，电脑管理从公共入口打开且关闭后返回原Tab。
    ///
    /// 数据构造：独立空模拟器；3个主Tab；5s容纳启动和系统导航，不代表网络延迟。
    /// 执行过程：核对Tab顺序 → 进入任务 → 管理电脑 → 关闭 → 检查仍在任务页。
    /// 预期结果：指挥台位于中间，任务可达；电脑不能占用主Tab，空任务不虚构Agent执行。
    @MainActor func testPrimaryNavigationMatchesPrototype() {
        let app = XCUIApplication(); app.launch()
        XCTAssertTrue(app.buttons["workspace-tab-tasks"].waitForExistence(timeout: 5), "第三个主Tab必须是任务")
        XCTAssertFalse(app.tabBars.buttons["电脑"].exists, "电脑管理不能替代任务Tab")
        XCTAssertEqual(app.tabBars.buttons.element(boundBy: 0).label, "项目", "项目应位于左侧")
        XCTAssertEqual(app.tabBars.buttons.element(boundBy: 1).label, "指挥台", "指挥台应位于正中间")
        app.buttons["workspace-tab-tasks"].tap()
        XCTAssertTrue(app.staticTexts["连接电脑后查看任务"].waitForExistence(timeout: 5), "空态不能伪造任务")
        app.buttons["manage-computers"].tap()
        XCTAssertTrue(app.buttons["add-computer"].waitForExistence(timeout: 5), "公共入口必须保留配对能力")
        app.buttons["computers-close"].tap()
        XCTAssertTrue(app.buttons["workspace-tab-tasks"].isSelected, "管理电脑返回后必须保留任务导航")
        XCTAssertFalse(app.buttons["computers-close"].exists, "电脑管理Sheet应完全关闭")
    }

    /// 通知设置在无配对时可达，进入页面不自动申请系统授权或伪造订阅。
    ///
    /// 数据构造：独立空数据模拟器；5s=启动和原生导航的最大等待预算。
    /// 执行过程：电脑页 → 通知设置 → 读取未连接说明 → 关闭。
    /// 预期结果：无连接不能启用任务提醒，系统授权弹窗不能由浏览页面触发。
    @MainActor func testNotificationSettingsRequiresAnExplicitOptIn() {
        let app = XCUIApplication(); app.launch()
        XCTAssertTrue(app.buttons["manage-computers"].waitForExistence(timeout: 5), "需要公共电脑入口")
        app.buttons["manage-computers"].tap()
        app.buttons["notification-settings"].tap()
        XCTAssertTrue(app.staticTexts["连接电脑后开启任务提醒"].waitForExistence(timeout: 5), "应说明按电脑订阅")
        XCTAssertFalse(app.buttons["enable-notifications"].exists, "未配对不得触发系统授权")
        XCTAssertFalse(app.staticTexts["任务提醒已开启"].exists, "空状态不能伪造订阅成功")
        app.buttons["notification-settings-close"].tap()
        XCTAssertTrue(app.buttons["add-computer"].waitForExistence(timeout: 5), "关闭设置回到原电脑页")
    }

    /// 指挥台居中，跨电脑审批和本机决定记录在未连接时仍可访问。
    ///
    /// 数据构造：无配对记录的原生 App；5s 为启动与页面转换的等待上限。
    /// 执行过程：点击中间 Tab → 切换待你决策 → 显示全部电脑范围 → 查看决定记录 → 关闭。
    /// 预期结果：无连接显示明确空态；不能伪造待审批或工具执行成功，也不能依赖当前选中电脑。
    @MainActor func testCommandCenterAndDecisionRecords() {
        let app = XCUIApplication(); app.launch()
        let command = app.buttons["workspace-tab-command"]
        XCTAssertTrue(command.waitForExistence(timeout: 5), "中间 Tab 必须保留指挥台")
        command.tap()
        let decisions = app.buttons["command-mode-decisions"]
        XCTAssertTrue(decisions.waitForExistence(timeout: 5), "决策应由指挥台分段直接进入")
        decisions.tap()
        XCTAssertTrue(app.staticTexts["全部电脑"].waitForExistence(timeout: 5), "指挥台范围不能由项目电脑选择器决定")
        XCTAssertTrue(app.staticTexts["先连接你的电脑"].exists, "没有配对时必须说明前置条件")
        app.buttons["approval-records"].tap()
        XCTAssertTrue(app.staticTexts["没有决定记录"].waitForExistence(timeout: 5), "离线时也能进入本机记录")
        XCTAssertFalse(app.staticTexts["工具执行成功"].exists, "没有真实执行证据不能伪造成功")
        app.buttons["approval-records-close"].tap()
        XCTAssertTrue(app.buttons["approval-records"].waitForExistence(timeout: 5), "关闭后返回指挥台")
        XCTAssertFalse(app.staticTexts["没有决定记录"].exists, "Sheet 不应残留")
    }

    /// 指挥台默认直接显示行动计划；未连接时只展示连接指引，不伪造Todo。
    ///
    /// 数据构造：无配对的App；5s=启动和原生导航动画的等待上限。
    /// 执行过程：进入指挥台 → 检查默认行动计划和空态 → 核对决定记录仍在同一页。
    /// 预期结果：无需先进入项目目录才能看到行动计划，无虚构Agent任务，无多余返回层级。
    @MainActor func testProjectPlansEntryWithoutPairedComputer() {
        let app = XCUIApplication(); app.launch()
        let command = app.buttons["workspace-tab-command"]
        XCTAssertTrue(command.waitForExistence(timeout: 5), "需要原生指挥台")
        command.tap()
        let plans = app.buttons["command-mode-plan"]
        XCTAssertTrue(plans.waitForExistence(timeout: 5), "指挥台需直接显示行动计划分段")
        XCTAssertTrue(plans.isSelected, "默认应呈现行动计划")
        XCTAssertTrue(app.staticTexts["连接电脑后梳理下一步"].waitForExistence(timeout: 5), "空态应说明如何开始")
        XCTAssertFalse(app.buttons["generate-project-plan"].exists, "无项目不得发起计划生成")
        XCTAssertTrue(app.buttons["approval-records"].exists, "行动计划保留决定记录入口")
        XCTAssertFalse(app.buttons["project-plans"].exists, "不能仍用多一级下一步入口替代直接呈现")
    }

    /// 未连接电脑时也能打开本机队列记录，关闭后回到电脑页。
    ///
    /// 数据构造：没有队列的模拟器 App；5s 只容纳启动动画和系统调度。
    /// 执行过程：电脑 Tab → 消息队列 → 空态 → 关闭。
    /// 预期结果：入口独立于授权项目；关闭后不残留 Sheet，不凭空显示已发送。
    @MainActor func testLocalQueueRecordsRemainReachable() {
        let app = XCUIApplication(); app.launch()
        let devices = app.buttons["manage-computers"]
        XCTAssertTrue(devices.waitForExistence(timeout: 5), "需要原生电脑页")
        devices.tap()
        let queue = app.buttons["local-message-queue"]
        XCTAssertTrue(queue.waitForExistence(timeout: 5), "断线时也须能找回消息原文")
        queue.tap()
        XCTAssertTrue(app.staticTexts["没有排队消息"].waitForExistence(timeout: 5), "无记录时显示真实空态")
        XCTAssertFalse(app.staticTexts["Agent 已收到"].exists, "没有消息不能伪造成功")
        app.buttons["queue-close"].tap()
        XCTAssertTrue(app.buttons["add-computer"].waitForExistence(timeout: 5), "关闭后回到电脑页")
        XCTAssertFalse(app.staticTexts["没有排队消息"].exists, "队列 Sheet 应已关闭")
    }

    /// 原生配对支持六位验证码与扫码；无效地址和无相机均不能伪造配对成功。
    ///
    /// 数据构造：全新安装的模拟器 App，无电脑记录；5s 等待仅容纳启动动画和系统调度。
    /// 执行过程：电脑tab → 添加电脑 → 输入六位码和无效地址并拒绝 → 返回 → 相机/图片入口 → 取消 → 本地草稿 → 关闭。
    /// 预期结果：扫码不可用提示可见；取消不伪造配对；本地草稿空态可访问，关闭后保留电脑列表。
    @MainActor func testNativePairingGuideAndScannerFallback() {
        let app = XCUIApplication()
        app.launch()
        let devices = app.buttons["manage-computers"]
        XCTAssertTrue(devices.waitForExistence(timeout: 5), "公共导航必须有电脑入口")
        devices.tap()
        app.buttons["add-computer"].tap()
        XCTAssertTrue(app.staticTexts["连接你的电脑"].waitForExistence(timeout: 5), "必须先说明在哪里生成配对信息")
        app.buttons["enter-pairing-code"].tap()
        let input = app.textFields["pairing-code-input"]
        let address = app.textFields["pairing-address-input"]
        XCTAssertTrue(input.waitForExistence(timeout: 5), "不使用相机也能输入六位验证码")
        XCTAssertTrue(address.exists, "首次连接必须有明确电脑地址")
        input.tap(); input.typeText("012345")
        XCTAssertFalse(app.buttons["verify-pairing-code"].isEnabled, "无地址不能猜测目标或发送验证码")
        address.tap(); address.typeText("ws://192.168.1.2:9443")
        app.buttons["verify-pairing-code"].tap()
        XCTAssertTrue(app.staticTexts["请输入电脑上显示的局域网或 Tailscale 地址。"].waitForExistence(timeout: 5), "协议降级必须在网络发送前拒绝")
        XCTAssertFalse(app.staticTexts["等待电脑确认"].exists, "无效地址不得产生配对状态")
        app.buttons["pairing-code-close"].tap()
        app.buttons["scan-qr"].tap()
        XCTAssertTrue(app.staticTexts["此设备暂不可扫码"].waitForExistence(timeout: 5), "模拟器不能伪造相机扫描结果")
        XCTAssertTrue(app.buttons["scan-qr-photo"].exists, "无相机时仍须允许选择真实二维码图片")
        app.buttons["scanner-close"].tap()
        app.buttons["pairing-cancel"].tap()
        XCTAssertTrue(app.buttons["add-computer"].waitForExistence(timeout: 5), "取消必须回到原页面")
        XCTAssertFalse(app.staticTexts["核对码"].exists, "取消后不保留未完成配对状态")
        app.buttons["local-drafts"].tap()
        XCTAssertTrue(app.staticTexts["没有未发送的草稿"].waitForExistence(timeout: 5), "尚未连接电脑也能访问本地草稿")
        app.buttons["drafts-close"].tap()
        XCTAssertTrue(app.buttons["add-computer"].waitForExistence(timeout: 5), "草稿页关闭不能丢失原导航")
        XCTAssertFalse(app.staticTexts["没有未发送的草稿"].exists, "关闭后不应残留草稿Sheet")
    }
}
