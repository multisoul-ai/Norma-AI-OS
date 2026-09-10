import XCTest

final class NavigationHeaderFlowTests: XCTestCase {
    /// 三个Tab统一顶部电脑入口和项目选择，任务范围切换不会改变指挥台项目或扩大授权。
    /// 数据构造：隔离双项目；首项目有完成任务，第二项目关闭；Large为系统标准字号。
    /// 执行过程：项目顶部→指挥台顶部切项目→任务所有/指定项目→恢复所有→原生无障碍审计。
    /// 预期结果：选择器只在导航栏，电脑按钮始终在左；关闭项目不串任务，所有项目恢复首项。
    @MainActor func testStandardHeadersAndProjectScopes() throws {
        try checkHeaders(category: "UICTContentSizeCategoryL", name: "standard")
    }

    /// 最大辅助字号下，三个页面顶部仍能切换电脑和项目，正文继续使用用户字号。
    /// 数据构造：同一只读双项目fixture，AX5为系统最大辅助字号；无用户真实电脑。
    /// 执行过程：按标准字号同一路径切换三个Tab与项目→检查顶部位置和点击→审计裁切与触控。
    /// 预期结果：顶部选择不被正文挤出或重复，任务范围准确，所有原生导航控件可触达。
    @MainActor func testAccessibleHeadersAndProjectScopes() throws {
        try checkHeaders(category: "UICTContentSizeCategoryAccessibilityXXXL", name: "AX5")
    }

    @MainActor private func checkHeaders(category: String, name: String) throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"],
              FileManager.default.fileExists(atPath: URL(fileURLWithPath: path).deletingLastPathComponent().appendingPathComponent("ui-project-navigation").path) else {
            throw XCTSkip("仅显式隔离双项目fixture")
        }
        let app = XCUIApplication()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", category]
        app.launch(); app.buttons["manage-computers"].tap()
        let computer = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        // 8s = 2s服务心跳 + 6s原生导航调度余量；不作为真实手机性能指标。
        XCTAssertTrue(computer.waitForExistence(timeout: 8), "仅选择隔离电脑")
        computer.tap(); app.buttons["workspace-tab-projects"].tap()
        let project = app.buttons["project-switcher"]
        XCTAssertTrue(project.waitForExistence(timeout: 8), "项目页的项目选择必须在导航栏")
        if project.label != "通知验收项目" {
            project.tap()
            XCTAssertTrue(app.searchFields["搜索项目"].waitForExistence(timeout: 8), "先等待项目面板打开")
            app.cells.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收项目")).firstMatch.tap()
        }
        XCTAssertTrue(app.buttons["agent-card-1"].waitForExistence(timeout: 8), "返回首项目原卡")
        try checkHeader(app, selector: project, below: app.staticTexts["project-page-title"], name: "项目-\(name)")

        app.buttons["workspace-tab-command"].tap()
        app.buttons["command-mode-plan"].tap()
        let plan = app.buttons["command-plan-project"]
        XCTAssertTrue(plan.waitForExistence(timeout: 8), "指挥台项目选择也必须在导航栏")
        guard plan.exists else { return }
        try checkHeader(app, selector: plan, below: app.buttons["command-mode-plan"], name: "指挥台-\(name)")
        choose("第二验收项目", from: plan, in: app)
        XCTAssertTrue(app.staticTexts["先定一个方向"].waitForExistence(timeout: 8), "顶部切换应读取第二项目的真实空计划")
        XCTAssertFalse(app.staticTexts["优先解决手机连接延迟"].exists, "切项目不能残留首项目方向")
        try checkHeader(app, selector: plan, below: app.buttons["command-mode-plan"], name: "指挥台空项目-\(name)")
        choose("通知验收项目", from: plan, in: app)
        XCTAssertTrue(app.staticTexts["优先解决手机连接延迟"].waitForExistence(timeout: 8), "切回首项目恢复其计划")
        app.buttons["command-mode-decisions"].tap()
        XCTAssertTrue(app.staticTexts["command-decision-scope"].exists, "全部电脑的决策范围也应明确放在顶部")
        XCTAssertFalse(app.buttons["command-plan-project"].exists, "全局决策不能伪装成当前项目筛选")
        app.buttons["command-mode-plan"].tap()

        app.buttons["workspace-tab-tasks"].tap()
        let scope = app.buttons["task-project-filter"]
        XCTAssertTrue(scope.waitForExistence(timeout: 8), "所有项目选择必须移到任务顶部")
        guard scope.exists else { return }
        XCTAssertEqual(scope.value as? String, "所有项目", "任务初始保留所有项目范围")
        try checkHeader(app, selector: scope, below: app.textFields["task-search"], name: "任务-\(name)")
        choose("第二验收项目", from: scope, in: app)
        XCTAssertEqual(scope.value as? String, "第二验收项目", "顶部需要显示已选择范围")
        XCTAssertFalse(app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "本地通知定位验收")).firstMatch.exists, "关闭的第二项目不能显示首项目任务")
        choose("所有项目", from: scope, in: app)
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "本地通知定位验收")).firstMatch.waitForExistence(timeout: 8), "恢复所有项目后首项目任务仍在")
        XCTAssertEqual(scope.value as? String, "所有项目", "清除项目限制后顶部范围同步恢复")
    }

    @MainActor private func choose(_ name: String, from menu: XCUIElement, in app: XCUIApplication) {
        menu.tap()
        let choice = app.buttons[name]
        XCTAssertTrue(choice.waitForExistence(timeout: 3), "原生菜单应呈现\(name)") // 3s仅覆盖菜单呈现，不等待网络。
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "hittable == true"), object: choice)], timeout: 3), .completed, "等待菜单项可点击后再选择")
        // Tap the observed menu row's center. Semantic tap intermittently leaves iOS 26's
        // context menu open despite reporting a successful event; still require real selection.
        choice.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", name), object: menu)], timeout: 3), .completed, "菜单选择必须实际更新当前项目")
    }

    @MainActor private func checkHeader(_ app: XCUIApplication, selector: XCUIElement, below content: XCUIElement, name: String) throws {
        let computer = app.buttons["manage-computers"]
        XCTAssertTrue(computer.isHittable, "\(name)：顶部电脑入口必须可点击")
        XCTAssertLessThanOrEqual(app.buttons["workspace-machine-switch"].frame.maxY, selector.frame.minY, "\(name)：品牌和电脑切换固定在页面标题上方")
        XCTAssertLessThanOrEqual(selector.frame.maxY, content.frame.minY, "\(name)：项目选择应在正文内容上方")
        XCTAssertEqual(app.buttons.matching(identifier: selector.identifier).count, 1, "\(name)：正文不能重复项目选择器")
        try app.performAccessibilityAudit(for: [.textClipped, .hitRegion, .sufficientElementDescription]) { issue in
            print("HEADER_AUDIT_\(name): \(issue.element?.debugDescription ?? "unknown element")")
            if issue.element == nil { print(app.debugDescription) }
            return false
        }
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }
}
