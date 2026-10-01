import XCTest

final class NotificationFlowUITests: XCTestCase {
    /// 独立已配对模拟器从系统授权进入按电脑订阅，不能把点击授权当作APNs登记成功。
    ///
    /// 数据构造：Rust notification-ui fixture建立唯一「通知验收 Mac」；5s启动/导航预算，20s系统回调预算。
    /// 执行过程：打开电脑页 → 任务提醒 → 用户允许系统通知 → 等系统回调 → 开启/关闭此电脑提醒。
    /// 预期结果：取得真实系统token才允许开启；Mac确认后显示登记，关闭后显示未开启；未触达真实APNs投递服务。
    @MainActor func testPairedComputerSystemRegistrationAndSubscription() throws {
        let app = XCUIApplication(); app.launch()
        XCTAssertTrue(app.buttons["manage-computers"].waitForExistence(timeout: 5), "需要公共电脑入口")
        app.buttons["manage-computers"].tap()
        guard app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch.waitForExistence(timeout: 5) else {
            throw XCTSkip("需要先启动Rust notification-ui fixture；不能操作其他已配对电脑")
        }
        app.buttons["computer-more-actions"].tap()
        app.buttons["notification-settings"].tap()
        XCTAssertTrue(app.staticTexts["通知验收 Mac"].waitForExistence(timeout: 5), "配置必须标明准确电脑")
        if app.buttons["enable-notifications"].exists {
            app.buttons["enable-notifications"].tap()
            let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
            let allow = springboard.alerts.buttons.matching(NSPredicate(format: "label IN %@", ["Allow", "允许"])).firstMatch
            if allow.waitForExistence(timeout: 5) { allow.tap() }
        } else if app.buttons["登记此 iPhone 的通知"].exists {
            app.buttons["登记此 iPhone 的通知"].tap()
        }
        XCTAssertTrue(app.staticTexts["iPhone 通知已就绪"].waitForExistence(timeout: 20), "必须实际收到系统token回调，不能永久停在登记中：\(app.debugDescription)")
        let enable = app.buttons["开启这台电脑的提醒"]
        XCTAssertTrue(enable.isEnabled, "系统登记后才能开启电脑提醒")
        guard enable.isEnabled else { return }
        enable.tap()
        XCTAssertTrue(app.staticTexts["电脑已登记此 iPhone"].waitForExistence(timeout: 5), "需要Mac返回已登记状态")
        XCTAssertFalse(app.staticTexts["任务提醒未开启"].exists, "开启后不能残留关闭状态")
        app.buttons["关闭这台电脑的提醒"].tap()
        XCTAssertTrue(app.staticTexts["任务提醒未开启"].waitForExistence(timeout: 5), "关闭必须获得Mac确认")
        XCTAssertFalse(app.buttons["关闭这台电脑的提醒"].exists, "已关闭不能显示仍可关闭")
        app.buttons["notification-settings-close"].tap()
    }
}

extension NotificationFlowUITests {
    /// 关闭提醒后可从原生电脑列表移除自己的验收配对，不能误删其他电脑。
    ///
    /// 数据构造：唯一「通知验收 Mac」，已完成真实WSS订阅开关；5s原生导航预算。
    /// 执行过程：识别准确电脑 → 打开管理菜单 → 忘记确认 → 等待Keychain/元数据删除。
    /// 预期结果：回到无电脑空态，原电脑行不再存在，没有留下连接错误。
    @MainActor func testForgetAcceptanceComputer() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui" else {
            throw XCTSkip("仅用于清理当前隔离通知fixture")
        }
        let app = XCUIApplication(); app.launch()
        app.buttons["manage-computers"].tap()
        let row = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 5), "只操作自己的验收电脑")
        app.buttons["管理 通知验收 Mac"].tap()
        app.buttons["忘记这台电脑"].tap()
        let confirm = app.buttons["忘记 通知验收 Mac"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 5), "确认必须写明准确电脑")
        confirm.tap()
        XCTAssertTrue(app.staticTexts["你的电脑，随手可达"].waitForExistence(timeout: 5), "清理后回到真实无电脑空态")
        XCTAssertFalse(row.exists, "不能残留测试电脑行")
        XCTAssertFalse(app.alerts["连接提示"].exists, "正式清理流程不能静默失败")
    }
    /// 点击通知须越过已有弹窗并向已配对Mac回源，冷启动和旧窗口拒绝仍然有效。
    ///
    /// 数据构造：已允许系统通知的独立模拟器；5条本地通知=设置+重复事件ID但不同目标+嵌套输入+冷启动+旧窗口；10s系统导航预算。
    /// 执行过程：设置弹窗切后台 → 通知回源 → 切语音/键盘 → 前台点击不同目标的通知 → 关闭恢复 → 嵌套输入/冷启动/旧窗口检查。
    /// 预期结果：有效通知立即进入准确卡片对话，底层弹窗不遮挡；返回保留订阅与地址；旧窗口不能打开新任务。
    @MainActor func testNotificationColdLaunchAndStaleWindow() throws {
        let env = ProcessInfo.processInfo.environment
        guard let path = env["NORMA_TEST_QR"], path.hasPrefix("/"), env["NORMA_TEST_MODE"] == "notification-ui" else {
            throw XCTSkip("需要scripts/check-norma-ios-notification-clicks.py与独立fixture")
        }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        let app = XCUIApplication()
        // 此处只检查独立通知窗口的环境注入；真实系统授权由 ChatVoiceInputFlowTests 覆盖。
        app.launchEnvironment["NORMA_CHAT_VOICE_FIXTURE"] = "denied"
        app.launch()
        app.buttons["manage-computers"].tap()
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch.waitForExistence(timeout: 5), "不能在用户其他电脑上验收")
        app.buttons["computer-more-actions"].tap()
        app.buttons["notification-settings"].tap()
        if app.buttons["登记此 iPhone 的通知"].waitForExistence(timeout: 2) { app.buttons["登记此 iPhone 的通知"].tap() }
        XCTAssertTrue(app.staticTexts["iPhone 通知已就绪"].waitForExistence(timeout: 20), "需要系统实际登记")
        if app.buttons["开启这台电脑的提醒"].exists { app.buttons["开启这台电脑的提醒"].tap() }
        XCTAssertTrue(app.staticTexts["电脑已登记此 iPhone"].waitForExistence(timeout: 5), "需先确认订阅成功")
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        XCUIDevice.shared.press(.home)
        try Data().write(to: directory.appendingPathComponent("warm-push-ready"), options: .atomic)
        let warm = springboard.staticTexts["设置中的验收任务已完成"]
        XCTAssertTrue(warm.waitForExistence(timeout: 10), "暖启动通知必须实际显示")
        warm.tap()
        guard app.segmentedControls["chat-content-tabs"].waitForExistence(timeout: 10) else {
            XCTFail("通知不能被原任务提醒设置弹窗遮挡：\(app.debugDescription)"); return
        }
        XCTAssertTrue(app.navigationBars["通知回源验收卡片"].exists, "必须直接打开准确卡片")
        XCTAssertFalse(app.buttons["notification-settings-close"].isHittable, "底层设置不能拦截通知页操作")
        app.buttons["chat-input-mode"].tap()
        XCTAssertTrue(app.buttons["chat-voice-hold"].waitForExistence(timeout: 10), "独立通知窗口必须提供共享通话状态，切语音不能崩溃")
        app.buttons["chat-input-mode"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["chat-message-input"].firstMatch.exists, "通知中的聊天必须能切回键盘")
        XCTAssertFalse(app.buttons["chat-voice-hold"].exists, "通知窗口切回键盘后不应残留语音按钮")
        try Data().write(to: directory.appendingPathComponent("replacement-push-ready"), options: .atomic)
        let replacement = springboard.staticTexts["另一项任务的提醒"]
        XCTAssertTrue(replacement.waitForExistence(timeout: 10), "第二次点击必须来自新的系统通知")
        replacement.tap()
        XCTAssertTrue(app.staticTexts["暂时无法打开这项提醒"].waitForExistence(timeout: 10), "已打开的通知页也必须按完整新目标重新核验，不能只信event_id")
        XCTAssertFalse(app.segmentedControls["chat-content-tabs"].exists, "新目标无效时不能残留上一项任务入口")
        app.buttons["notification-destination-close"].tap()
        XCTAssertTrue(app.staticTexts["电脑已登记此 iPhone"].waitForExistence(timeout: 5), "返回必须保留此前订阅和设置上下文")
        app.buttons["notification-settings-close"].tap()
        app.buttons["add-computer"].tap()
        app.buttons["enter-pairing-code"].tap()
        let address = app.textFields["pairing-address-input"]
        XCTAssertTrue(address.waitForExistence(timeout: 5), "需要真实嵌套输入弹窗")
        address.tap(); address.typeText("acceptance.invalid")
        XCUIDevice.shared.press(.home)
        try Data().write(to: directory.appendingPathComponent("nested-push-ready"), options: .atomic)
        let nested = springboard.staticTexts["输入中的验收任务已完成"]
        XCTAssertTrue(nested.waitForExistence(timeout: 10), "嵌套弹窗通知必须由系统显示")
        nested.tap()
        XCTAssertTrue(app.segmentedControls["chat-content-tabs"].waitForExistence(timeout: 10), "通知必须跨过任意深度弹窗")
        XCTAssertFalse(address.isHittable, "输入页不能遮挡任务入口")
        app.buttons["notification-destination-close"].tap()
        XCTAssertTrue(address.waitForExistence(timeout: 5), "关闭通知必须返回原输入页")
        XCTAssertEqual(address.value as? String, "acceptance.invalid", "未发送的地址输入不能因通知丢失")
        app.buttons["pairing-code-close"].tap()
        app.buttons["pairing-cancel"].tap()
        app.terminate()
        XCUIDevice.shared.press(.home)
        try Data().write(to: directory.appendingPathComponent("push-ready"), options: .atomic)
        let first = springboard.staticTexts["一项验收任务已完成"]
        XCTAssertTrue(first.waitForExistence(timeout: 10), "本地注入通知必须实际出现在系统界面")
        first.tap()
        XCTAssertTrue(app.navigationBars["通知回源验收卡片"].waitForExistence(timeout: 10), "点击通知应冷启动Norma并直接打开原卡片")
        XCTAssertTrue(app.staticTexts["通知验收 Mac"].waitForExistence(timeout: 10), "必须显示保存的正确电脑")
        XCTAssertFalse(app.buttons["打开任务对话"].exists, "完成通知不应要求再点一次中间入口")
        XCTAssertTrue(app.segmentedControls["chat-content-tabs"].exists, "准确任务必须直接显示对话")
        XCTAssertFalse(app.staticTexts["暂时无法打开这项提醒"].exists, "正确目标不能误判过期")
        let valid = XCTAttachment(screenshot: app.screenshot()); valid.name = "notification-current-window"; valid.lifetime = .keepAlways; add(valid)
        app.buttons["notification-destination-close"].tap()
        try Data().write(to: directory.appendingPathComponent("ui-stale"), options: .atomic)
        app.terminate(); XCUIDevice.shared.press(.home)
        try Data().write(to: directory.appendingPathComponent("stale-push-ready"), options: .atomic)
        let stale = springboard.staticTexts["一项旧窗口任务已完成"]
        XCTAssertTrue(stale.waitForExistence(timeout: 10), "第二条通知必须由系统显示")
        stale.tap()
        XCTAssertTrue(app.staticTexts["暂时无法打开这项提醒"].waitForExistence(timeout: 10), "窗口变化后旧通知必须拒绝")
        XCTAssertFalse(app.segmentedControls["chat-content-tabs"].exists, "不能把旧通知映射到新窗口任务")
        let rejected = XCTAttachment(screenshot: app.screenshot()); rejected.name = "notification-stale-window"; rejected.lifetime = .keepAlways; add(rejected)
        app.buttons["notification-destination-close"].tap()
        try FileManager.default.removeItem(at: directory.appendingPathComponent("ui-stale"))
    }
}


extension NotificationFlowUITests {
    /// 无法连接的电脑也能确认移除，取消不删数据；其他电脑与重启后的列表保持正确。
    ///
    /// 数据构造：forget-ui模式，预先在独立模拟器存入离线验收A/B；3s为本地删除与动画预算，不等待网络超时。
    /// 执行过程：关闭连接错误 → 左滑A并取消 → 再确认删除A → 核对B保留 → 重启 → 通过可见菜单删除B。
    /// 预期结果：取消保留A；确认后A消失且B保留；重启不恢复A；删除B后显示空态。
    @MainActor func testForgetOfflineComputers() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "forget-ui" else {
            throw XCTSkip("仅操作显式构造的离线验收电脑")
        }
        let app = XCUIApplication(); app.launch()
        func openComputers() {
            if app.alerts["连接提示"].waitForExistence(timeout: 3) { app.alerts.buttons["好"].tap() }
            app.buttons["manage-computers"].tap()
        }
        func row(_ name: String) -> XCUIElement {
            app.buttons.matching(NSPredicate(format: "label CONTAINS %@", name)).firstMatch
        }
        openComputers()
        let a = row("离线验收 Mac A"), b = row("离线验收 Mac B")
        XCTAssertTrue(a.waitForExistence(timeout: 5), "仅删除隔离A")
        XCTAssertTrue(b.exists, "B必须在初始列表中")
        a.swipeLeft(); app.buttons["忘记"].tap()
        XCTAssertTrue(app.buttons["忘记 离线验收 Mac A"].waitForExistence(timeout: 3), "确认框应写明机器名称")
        app.buttons.matching(NSPredicate(format: "label IN %@", ["取消", "Cancel"])).firstMatch.tap()
        XCTAssertTrue(a.exists, "取消必须保留A")
        a.swipeLeft(); app.buttons["忘记"].tap()
        app.buttons["忘记 离线验收 Mac A"].tap()
        XCTAssertTrue(a.waitForNonExistence(timeout: 3), "离线A必须本地移除，不能被关闭提醒的网络请求阻塞")
        XCTAssertTrue(b.waitForExistence(timeout: 3), "移除A后必须留在列表并保留B：\(app.debugDescription)")
        XCTAssertFalse(app.alerts["连接提示"].exists, "自动切到另一台离线电脑不能弹错打断删除")
        app.terminate(); app.launch(); openComputers()
        XCTAssertFalse(a.exists, "重启后A不能重新出现")
        XCTAssertTrue(b.exists, "重启后必须保留B")
        app.buttons["computer-actions-mac-" + String(repeating: "b", count: 64)].tap()
        app.buttons["忘记这台电脑"].tap()
        app.buttons["忘记 离线验收 Mac B"].tap()
        XCTAssertTrue(app.staticTexts["你的电脑，随手可达"].waitForExistence(timeout: 3), "最后一台离线电脑也能删除")
        XCTAssertFalse(b.exists, "不能残留B")
    }
}
