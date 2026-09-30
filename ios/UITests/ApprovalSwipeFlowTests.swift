import XCTest

final class ApprovalSwipeFlowTests: XCTestCase {
    /// 决策可打开原会话，查看不批准；打开前会话或窗口被替换时明确拒绝。
    /// 数据构造：只读Claude原卡7/run-7/session-7/turn-7；审批与聊天经过同一TLS服务。
    /// 执行过程：从决策打开原回复和输入框→返回→替换会话→拒绝→替换窗口→拒绝。
    /// 预期结果：准确原文可见且只读不能发送；错误目标不显示聊天，没有写命令。
    @MainActor func testOpenOriginalTaskAndRejectChangedTarget() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else { throw XCTSkip("仅隔离审批fixture") }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-decision-deck").path) else { throw XCTSkip("未启用审批fixture") }
        defer {
            try? FileManager.default.removeItem(at: dir.appendingPathComponent("ui-decision-session-replaced"))
            try? FileManager.default.removeItem(at: dir.appendingPathComponent("ui-decision-window-replaced"))
        }
        let app = XCUIApplication(); app.launch(); app.buttons["workspace-tab-command"].tap()
        openCommandDecisions(in: app)
        // 8s = 2s现有快照上限 + 6s导航/真实WSS余量，不作为性能结论。
        let detail = app.buttons["查看原任务"]
        XCTAssertTrue(detail.waitForExistence(timeout: 8), "决策应提供原型任务详情入口")
        guard detail.exists else { return }
        detail.tap()
        XCTAssertTrue(app.staticTexts["来自原会话 session-7 的完整回复"].waitForExistence(timeout: 8), "必须读取原卡完整历史")
        let input = app.descendants(matching: .any)["chat-message-input"].firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 3), "必须复用正常聊天输入界面")
        XCTAssertFalse(app.buttons["chat-send-message"].isEnabled, "查看不能扩大只读权限")
        app.buttons["approval-task-close"].tap()
        XCTAssertTrue(app.staticTexts["检查登录测试"].waitForExistence(timeout: 3), "返回应保留原决策位置")
        try Data().write(to: dir.appendingPathComponent("ui-decision-session-replaced"))
        detail.tap()
        XCTAssertTrue(app.staticTexts["无法打开原任务"].waitForExistence(timeout: 8), "旧审批不能打开替换会话")
        XCTAssertFalse(input.exists, "会话不匹配时不能展示可输入的错误对话")
        app.buttons["approval-task-close"].tap()
        try FileManager.default.removeItem(at: dir.appendingPathComponent("ui-decision-session-replaced"))
        try Data().write(to: dir.appendingPathComponent("ui-decision-window-replaced"))
        detail.tap()
        XCTAssertTrue(app.staticTexts["无法打开原任务"].waitForExistence(timeout: 8), "相同项目的新窗口也不能认领旧请求")
        XCTAssertFalse(input.exists, "窗口变化必须拒绝进入")
        app.buttons["approval-task-close"].tap()
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-decisions.json").path), "查看详情不能产生决定")
    }

    /// 最大辅助字号下决策内容可以竖向滚动，选项保持可触达且只读不能批准。
    /// 数据构造：隔离fixture首项read权限；AX5为最大系统辅助字号；8s=2s心跳+6s导航余量。
    /// 执行过程：打开待决策→审计首屏→最多3次竖向滚动到操作区→再次审计。
    /// 预期结果：标题和工具内容不裁切，按钮仍可触达；滚动无审批命令，不扩大授权。
    @MainActor func testAccessibleDecisionCard() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("仅用于隔离审批fixture")
        }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-decision-deck").path) else {
            throw XCTSkip("未显式启用审批fixture")
        }
        let app = XCUIApplication()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        app.launch(); app.buttons["workspace-tab-command"].tap()
        openCommandDecisions(in: app)
        XCTAssertTrue(app.staticTexts["检查登录测试"].waitForExistence(timeout: 8), "最大字号仍应显示首项")
        try app.performAccessibilityAudit(for: [.textClipped, .hitRegion, .sufficientElementDescription])
        let approve = app.buttons["approval-allow"]
        // 3次覆盖本fixture来源、标题与完整输入；有界滚动避免掩盖按钮丢失。
        for _ in 0..<3 where !approve.isHittable { app.scrollViews.firstMatch.swipeUp() }
        XCTAssertTrue(approve.isHittable, "批准按钮必须可滚动到达")
        XCTAssertFalse(approve.isEnabled, "大字号适配不能提升只读权限")
        try app.performAccessibilityAudit(for: [.textClipped, .hitRegion, .sufficientElementDescription])
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-decisions.json").path), "竖向阅读不能发送决定")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "decision-card-AX5"; shot.lifetime = .keepAlways; add(shot)
    }

    /// 原生决定弹层只提交当前请求；阅读、下一项和来源更新均不能授权。
    /// 数据构造：两项普通审批和一项电脑确认；read 升为 control 后提交两次。
    ///   8s = 2s订阅心跳 + 6s模拟器导航余量；内容更新只等待真实订阅条件。
    /// 执行过程：只读阅读 → 升级 → 下一项 → 另一端更新输入 → 批准第二项 → 拒绝第一项。
    /// 预期结果：只有 second/allow 和 first/deny；电脑确认不能批准；记录保留两次准确决定。
    @MainActor func testButtonsSubmitOnlyTheCurrentDecision() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("仅用于显式隔离审批fixture")
        }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-decision-deck").path) else {
            throw XCTSkip("需显式启用审批fixture")
        }
        let app = XCUIApplication(); app.launch()
        app.buttons["workspace-tab-command"].tap(); openCommandDecisions(in: app)
        XCTAssertTrue(app.staticTexts["检查登录测试"].waitForExistence(timeout: 8), "应先显示第一个真实请求")
        let approve = app.buttons["approval-allow"]
        XCTAssertFalse(approve.isEnabled, "只读项目不得批准")
        XCTAssertFalse(app.otherElements["approval-swipe-surface"].exists, "旧滑卡交互必须删除")
        app.staticTexts["approval-tool-input"].swipeRight()
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-decisions.json").path), "阅读和滑动不能产生审批")
        try Data().write(to: dir.appendingPathComponent("ui-card-management"))
        let enabled = XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"), object: approve)
        XCTAssertEqual(XCTWaiter.wait(for: [enabled], timeout: 8), .completed, "授权升级后原请求才可操作")
        app.buttons["下一项"].tap()
        XCTAssertTrue(app.staticTexts["核对连接路径"].waitForExistence(timeout: 3), "下一项呈现另一请求")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-decisions.json").path), "下一项不是拒绝")
        try Data().write(to: dir.appendingPathComponent("ui-decision-change"))
        let changed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label CONTAINS %@", "changed-input"), object: app.staticTexts["approval-tool-input"])
        XCTAssertEqual(XCTWaiter.wait(for: [changed], timeout: 8), .completed, "应显示另一端更新后的准确输入")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-decisions.json").path), "内容更新不能自动授权")
        approve.tap(); verifyReceipt(app); app.buttons["关闭"].tap()
        XCTAssertTrue(app.staticTexts["检查登录测试"].waitForExistence(timeout: 8), "处理第二项后第一项仍待确认")
        app.buttons["approval-deny"].tap(); verifyReceipt(app); app.buttons["关闭"].tap()
        XCTAssertTrue(app.staticTexts["电脑扩展确认"].waitForExistence(timeout: 8), "电脑确认仍应保留")
        XCTAssertFalse(approve.exists, "电脑负责的确认不得提供手机批准")
        let received = try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("ui-decisions.json"))) as? [[String: String]]
        XCTAssertEqual(received, [["request":"request-second", "decision":"allow"], ["request":"request-first", "decision":"deny"]], "只能发送两次准确决定")
        app.buttons["approval-records"].tap()
        XCTAssertTrue(app.staticTexts["检查登录测试"].waitForExistence(timeout: 3), "拒绝记录必须保留")
        XCTAssertTrue(app.staticTexts["核对连接路径"].exists, "批准记录必须保留")
        app.buttons["approval-records-close"].tap()
    }

    @MainActor private func verifyReceipt(_ app: XCUIApplication) {
        let receipt = app.staticTexts["approval-status"]
        XCTAssertTrue(receipt.waitForExistence(timeout: 8), "必须展示可查询的正式回执")
        let submitted = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label == %@", "决定已提交，执行结果请查看任务"), object: receipt)
        XCTAssertEqual(XCTWaiter.wait(for: [submitted], timeout: 8), .completed, "决定提交不能冒充工具执行成功")
    }
}
