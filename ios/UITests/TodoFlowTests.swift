import XCTest

final class TodoFlowTests: XCTestCase {
    /// 指挥台完整安排、撤回、重新安排和不采纳流程必须修改同一电脑的真实待办记录。
    ///
    /// 数据构造：隔离TLS/WSS项目，两项固定建议指向node1同一run/session；Agent初始忙碌。
    ///   8s = 2s服务心跳 + 6s原生导航调度余量，不作为网络性能指标。
    /// 执行过程：确认入队→撤回→重新确认入队→Agent空闲→同一订阅收到完成→拒绝第二项→检查全部记录。
    /// 预期结果：撤回/拒绝没有发送；只有第一项原文送达一次；已完成/拒绝不再占待办，原决定可回查。
    @MainActor func testQueueCancelRetryAndDeclineFromCommandCenter() throws {
        try checkFlow(category: "UICTContentSizeCategoryL")
    }

    /// 最大辅助字号仍可完成同一待办闭环，按钮与状态不能因大字被挤出。
    /// 数据构造：与标准字号同样的两项建议，AX5为系统最大辅助字号，使用全新隔离配对/数据库。
    /// 执行过程：以AX5重复安排、撤回、重排、完成和拒绝，再检查持久化结果与无障碍。
    /// 预期结果：全部操作可触达，拒绝不执行、重排只发送一次，文本及触控审计通过。
    @MainActor func testAccessibleTodoDecisions() throws {
        try checkFlow(category: "UICTContentSizeCategoryAccessibilityXXXL")
    }

    @MainActor private func checkFlow(category: String) throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("仅在显式隔离Todo服务验收")
        }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-todo-flow").path) else {
            throw XCTSkip("未启用Todo控制fixture")
        }
        let sent = directory.appendingPathComponent("ui-todo-sent.json")
        let app = XCUIApplication()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", category]
        app.launch()
        app.buttons["manage-computers"].tap()
        let computer = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        XCTAssertTrue(computer.waitForExistence(timeout: 8), "只操作隔离电脑")
        computer.tap(); app.buttons["workspace-tab-command"].tap()
        app.buttons["command-mode-plan"].tap()
        XCTAssertTrue(app.staticTexts["验证待办执行闭环"].waitForExistence(timeout: 8), "读取本轮计划而非旧计划")
        try tap("todo-open-0", in: app)
        try tap("todo-accept-0", in: app)
        XCTAssertTrue(app.staticTexts["当前 Agent 正在执行，确认后加入队列。"].waitForExistence(timeout: 8), "确认页说明等待当前Agent")
        try tap("todo-confirm", in: app)
        try status(0, equals: "已安排 · 等待 Agent 空闲", in: app)
        XCTAssertFalse(FileManager.default.fileExists(atPath: sent.path), "入队不能提前发送")
        try tap("todo-cancel-0", in: app)
        try status(0, equals: "已撤回 · 没有发送", in: app)
        XCTAssertFalse(FileManager.default.fileExists(atPath: sent.path), "撤回项不得进入执行器")
        try tap("todo-retry-0", in: app)
        XCTAssertTrue(app.staticTexts["todo-retry-notice"].waitForExistence(timeout: 8), "重新安排必须显示旧决定仍保留")
        try tap("todo-confirm", in: app)
        try status(0, equals: "已安排 · 等待 Agent 空闲", in: app)
        XCTAssertFalse(app.buttons["todo-retry-0"].exists, "新安排待发送时不能重复重排")
        try FileManager.default.removeItem(at: directory.appendingPathComponent("ui-task-running"))
        try status(0, equals: "本轮执行已完成 · 待验收", in: app)
        app.buttons["完成"].tap()
        // 详情保留完成回执；关闭后首页自动过滤已完成项，不手动刷新掩盖订阅问题。
        let completed = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            FileManager.default.fileExists(atPath: sent.path) && !app.buttons["todo-open-0"].exists
        }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [completed], timeout: 8), .completed, "完成项必须自动离开待办")
        try tap("todo-open-1", in: app)
        try tap("todo-reject-1", in: app)
        XCTAssertTrue(app.staticTexts["不采纳这项建议，不会向 Agent 发送任务。"].waitForExistence(timeout: 8), "拒绝预览须说明无任务副作用")
        try tap("todo-confirm", in: app)
        try status(1, equals: "未采纳 · 没有安排任务", in: app)
        app.buttons["完成"].tap()
        XCTAssertTrue(app.staticTexts["这份计划暂无待办"].waitForExistence(timeout: 8), "完成与不采纳后待办应为空")
        XCTAssertFalse(app.buttons["todo-accept-0"].exists, "完成项不可作为新建议重复安排")
        XCTAssertFalse(app.buttons["todo-accept-1"].exists, "拒绝项不能继续出现在待办中")
        try tap("查看全部记录", in: app)
        try tap("todo-open-0", in: app)
        try status(0, equals: "本轮执行已完成 · 待验收", in: app)
        XCTAssertFalse(app.buttons["todo-accept-0"].exists, "完成详情不提供重复安排")
        app.buttons["完成"].tap()
        try tap("todo-open-1", in: app)
        try status(1, equals: "未采纳 · 没有安排任务", in: app)
        XCTAssertFalse(app.buttons["todo-retry-1"].exists, "拒绝建议不冒充发送失败")
        let data = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(contentsOf: sent)) as? [String: String])
        XCTAssertEqual(data["text"], "记录局域网与 Tailscale 使用的连接路径。", "Agent收到的是保存的准确原文")
        XCTAssertNotEqual(data["text"], "这项建议将被拒绝，不得发送。", "拒绝项无发送副作用")
        try app.performAccessibilityAudit(for: [.textClipped, .hitRegion, .sufficientElementDescription])
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "Todo执行与拒绝记录"; shot.lifetime = .keepAlways; add(shot)
    }

    @MainActor private func tap(_ id: String, in app: XCUIApplication) throws {
        let button = app.buttons[id]
        if id.hasPrefix("todo-open-") { scrollToWorkspaceRow(button, in: app) }
        else {
            for _ in 0..<6 where !button.isHittable {
                if button.exists && button.frame.midY < app.frame.midY { app.swipeDown() }
                else { app.swipeUp() }
            }
        }
        XCTAssertTrue(button.waitForExistence(timeout: 8), "应显示操作：\(id)")
        XCTAssertTrue(button.isHittable, "操作不能被列表挤出：\(id)")
        XCTAssertTrue(button.isEnabled, "当前有效授权下操作应可用：\(id)")
        guard button.isHittable && button.isEnabled else { throw NSError(domain: "TodoUI", code: 1) }
        button.tap()
    }

    @MainActor private func status(_ index: Int, equals label: String, in app: XCUIApplication) throws {
        let text = app.staticTexts["todo-status-\(index)"]
        for _ in 0..<6 where !text.isHittable { app.swipeUp() }
        let expected = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label == %@", label), object: text)
        XCTAssertEqual(XCTWaiter.wait(for: [expected], timeout: 8), .completed, "Todo\(index)必须显示电脑确认的状态：\(label)")
    }
}

// A partially visible row can be "hittable" behind the floating tabs. Bring its actual center into
// the content viewport before asking XCTest to tap; never change the app's state in the test helper.
@MainActor func scrollToWorkspaceRow(_ row: XCUIElement, in app: XCUIApplication) {
    // 12 × 25% of a screen covers the full AX5 brief, metrics, direction and first Todo.
    for _ in 0..<12 {
        let bottom = app.buttons["workspace-tab-projects"].frame.minY - 24
        let top = app.buttons["command-mode-plan"].frame.maxY + 24
        if row.exists && row.isHittable && row.frame.midY > top && row.frame.midY < bottom { return }
        let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.7))
        let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.45))
        if row.exists && row.frame.midY < top { end.press(forDuration: 0.05, thenDragTo: start) }
        else { start.press(forDuration: 0.05, thenDragTo: end) }
    }
    XCTFail("待办行必须能滚入标题与悬浮底栏之间：\(row.identifier)")
}
