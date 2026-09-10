import XCTest

final class DeliveryInboxFlowTests: XCTestCase {
    /// 指挥台显示真实待验收数量，进入实时列表后其他端验收立即移出，切项目不能沿用旧数量。
    /// 数据构造：隔离只读双项目；第一项目冻结2份成果，第二项目无成果无计划；8s=2s心跳+6s原生导航余量。
    /// 执行过程：指挥台2份→待验收列表→另一端验收第二份→剩第一份→全部记录→返回→切空项目。
    /// 预期结果：2→1→0准确且不需刷新，已验收只在全部成果可见；没有计划也可进入成果列表。
    @MainActor func testCommandInboxUpdatesAndStaysScoped() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("仅用于隔离成果fixture")
        }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-delivery-seed").path) else {
            throw XCTSkip("需要显式准备成果数据")
        }
        let app = XCUIApplication(); app.launch()
        app.buttons["manage-computers"].tap()
        let machine = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        XCTAssertTrue(machine.waitForExistence(timeout: 8), "只选择隔离电脑")
        machine.tap(); app.buttons["workspace-tab-command"].tap()
        let inbox = app.buttons["command-delivery-inbox"]
        XCTAssertTrue(inbox.waitForExistence(timeout: 8), "计划页应有直达验收的入口")
        guard inbox.exists else { return }
        waitValue(inbox, "2", "总数必须来自当前项目完整收件箱")
        let decisions = app.buttons["command-decision-inbox"]
        waitValue(decisions, "待核验", "fixture未提供审批源，不能伪造零审批")
        decisions.tap()
        XCTAssertTrue(app.staticTexts["数量待核验"].waitForExistence(timeout: 8), "审批源异常时必须说明数量未知")
        XCTAssertFalse(app.staticTexts["0 项待确认"].exists, "未知数量不能显示成零")
        app.buttons["command-mode-plan"].tap()
        XCTAssertTrue(inbox.waitForExistence(timeout: 8), "切回计划保留成果入口")
        inbox.tap()
        let first = app.buttons["task-delivery-delivery-first"]
        let second = app.buttons["task-delivery-delivery-second"]
        XCTAssertTrue(second.waitForExistence(timeout: 8), "待验收默认包含最近成果")
        XCTAssertTrue(first.exists, "不能遗漏更早未验收成果")
        try Data().write(to: directory.appendingPathComponent("ui-delivery-accept"), options: .atomic)
        let removed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: second)
        XCTAssertEqual(XCTWaiter.wait(for: [removed], timeout: 8), .completed, "另一端验收后必须自动移出")
        XCTAssertTrue(first.exists, "另一份未验收成果仍应保留")
        app.segmentedControls["delivery-filter"].buttons["全部成果"].tap()
        XCTAssertTrue(second.waitForExistence(timeout: 8), "全部成果保留验收历史")
        XCTAssertTrue(app.staticTexts["delivery-accepted-delivery-second"].exists, "已验收记录需要明确标识")
        app.buttons["delivery-inbox-close"].tap()
        waitValue(inbox, "1", "返回指挥台重新订阅后总数也应更新")
        app.buttons["command-plan-project"].tap()
        app.buttons["第二验收项目"].tap()
        XCTAssertTrue(app.staticTexts["先定一个方向"].waitForExistence(timeout: 8), "第二项目保持真实计划空态")
        waitValue(inbox, "0", "空项目不能使用首项目数量")
        inbox.tap()
        XCTAssertTrue(app.staticTexts["没有待验收成果"].waitForExistence(timeout: 8), "没有计划也应能打开成果空态")
        XCTAssertFalse(first.exists, "第二项目不能泄漏第一项目成果")
        app.buttons["delivery-inbox-close"].tap()
    }

    @MainActor private func waitValue(_ element: XCUIElement, _ value: String, _ message: String) {
        let exact = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", value), object: element)
        XCTAssertEqual(XCTWaiter.wait(for: [exact], timeout: 8), .completed, message)
    }
}
