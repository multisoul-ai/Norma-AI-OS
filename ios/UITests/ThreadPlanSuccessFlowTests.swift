import XCTest

final class ThreadPlanSuccessFlowTests: XCTestCase {
    /// 真实原生输入经正式本地网关回到同一线程；仅供应商回复为测试数据。
    /// 数据构造：1个空空间配置、1位Alice；UI创建新线程并发送1轮，得到1回复/1待发任务。
    /// 执行过程：创建线程→成员选择Norma→发送→核对协调者先于任务→详情/返回→重启。
    /// 预期结果：原回复和任务各1份，项目/Agent准确；未连接Mac不能显示已执行。
    @MainActor func testNormaPlanInThread() throws { try runFlow(largeText: false) }

    /// 最大辅助字号下同一真实HTTP编排流程可读可操作。
    /// 数据构造：同一空间中新建另一线程；系统Accessibility XXXL；10秒=云模拟请求+UI调度余量。
    /// 执行过程：创建线程/选择/发送/回看任务/重启，必要时滚动到历史。
    /// 预期结果：协调者回复、待发状态和返回入口可达，无字号上限或重复任务。
    @MainActor func testNormaPlanWithAccessibilityText() throws { try runFlow(largeText: true) }

    @MainActor private func runFlow(largeText: Bool) throws {
        continueAfterFailure = false
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "thread-plan-ui",
              let path = ProcessInfo.processInfo.environment["NORMA_THREAD_PLAN_FIXTURE"],
              FileManager.default.fileExists(atPath: path) else { throw XCTSkip("需要本机编排UI服务及隔离配置") }
        struct Fixture: Decodable { let port: UInt16; let namespace: String }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_CLOUD_LOCAL_PORT"] = String(fixture.port)
        app.launchEnvironment["NORMA_CLOUD_TEST_NAMESPACE"] = fixture.namespace
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = fixture.namespace
        if largeText { app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
        app.launch(); app.buttons["workspace-tab-projects"].tap()
        let workspace = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-card-")).firstMatch
        XCTAssertTrue(workspace.waitForExistence(timeout: 10), "隔离空间应可读")
        workspace.tap(); app.buttons["create-work-thread"].tap()
        let title = (largeText ? "首屏交接·大字号" : "改进首屏交接") + "·" + UUID().uuidString.prefix(4)
        app.textFields["thread-title"].tap(); app.textFields["thread-title"].typeText(title)
        app.buttons["thread-create-save"].tap(); app.buttons["thread-mention"].tap()
        let norma = app.buttons["thread-mention-norma"]
        XCTAssertTrue(norma.waitForExistence(timeout: 10), "协调者应可从同一成员选择器进入")
        attach(app, largeText ? "P05-norma-picker-AX5" : "P05-norma-picker-standard")
        let close = app.buttons["thread-mention-cancel"]
        XCTAssertTrue(close.isHittable, "成员选择必须有直接可达的退出入口")
        close.tap()
        XCTAssertFalse(norma.exists, "关闭选择器应回到原线程，不自动选择或发送")
        app.buttons["thread-mention"].tap()
        norma.tap()
        let input = app.textFields["thread-message-input"]
        input.typeText("请检查 iOS 首屏")
        app.buttons["thread-send"].tap()
        let reply = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-norma-message-")).firstMatch
        XCTAssertTrue(reply.waitForExistence(timeout: 10), "完整HTTP回复应以Norma身份回原线程")
        XCTAssertTrue(reply.label.contains("由 iOS Alice 检查首屏"), "不能显示默认文案或把回复当用户气泡")
        let cards = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-"))
        let card = cards.firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 10), "合法候选应进入真实待发队列")
        XCTAssertEqual(cards.count, 1, "一个候选只能保存一个任务")
        XCTAssertTrue(card.label.contains("Alice"), "任务目标应来自模型选择的稳定成员")
        XCTAssertTrue(card.label.contains("待发送"), "没有连接Mac时只能等待发送")
        XCTAssertFalse(card.label.contains("Agent 已接收"), "本地模型候选不能冒充Mac执行")
        if !largeText { XCTAssertLessThan(reply.frame.minY, card.frame.minY, "P05中应先呈现Norma分工，再呈现任务卡") }
        attach(app, largeText ? "P05-norma-reply-AX5" : "P05-norma-reply-standard")
        if largeText {
            try revealReply(reply, app: app, input: input)
            attach(app, "P05-norma-body-AX5")
        }
        card.tap()
        XCTAssertTrue(app.staticTexts["thread-task-status"].waitForExistence(timeout: 10), "任务仍复用原生详情入口")
        XCTAssertTrue(app.staticTexts["thread-task-status"].label.contains("待发送"), "详情不能虚构执行进度")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(input.waitForExistence(timeout: 10), "一次返回回到原线程输入")
        XCTAssertEqual(input.value as? String, "发消息或 @Agent", "完整落盘后应清空已发送草稿")
        app.terminate(); app.launch(); app.buttons["workspace-tab-projects"].tap(); workspace.tap()
        let row = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@ AND label CONTAINS %@", "work-thread-row-", title)).firstMatch
        row.tap()
        XCTAssertTrue(reply.waitForExistence(timeout: 10), "重启后Norma回复仍属于原线程")
        XCTAssertEqual(cards.count, 1, "重启不能重复应用模型候选")
        if largeText { try revealReply(reply, app: app, input: input) }
        attach(app, largeText ? "P05-norma-restart-AX5" : "P05-norma-restart-standard")
    }

    @MainActor private func revealReply(_ reply: XCUIElement, app: XCUIApplication, input: XCUIElement) throws {
        // AX5 renders the short reply above the task by roughly two viewports; allow up to eight
        // partial drags to retain context rather than relying on an offscreen accessibility node.
        let scroll = app.scrollViews.firstMatch
        for _ in 0..<8 {
            if reply.frame.minY >= app.navigationBars.firstMatch.frame.maxY && reply.frame.maxY <= input.frame.minY { break }
            let below = reply.frame.maxY > input.frame.minY
            let start = scroll.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: below ? 0.6 : 0.3))
            let end = scroll.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: below ? 0.3 : 0.6))
            start.press(forDuration: 0.05, thenDragTo: end)
        }
        XCTAssertGreaterThanOrEqual(reply.frame.minY, app.navigationBars.firstMatch.frame.maxY, "协调者头像和正文不能藏在导航层后")
        XCTAssertLessThanOrEqual(reply.frame.maxY, input.frame.minY, "协调者正文必须完整出现在输入栏之上")
    }

    @MainActor private func attach(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }
}
