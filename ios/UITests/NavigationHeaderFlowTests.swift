import XCTest

final class NavigationHeaderFlowTests: XCTestCase {
    /// 三个Tab统一顶部电脑入口和项目选择，任务范围切换不会改变指挥台项目或扩大授权。
    /// 数据构造：隔离双项目；首项目有完成任务，第二项目关闭；Large为系统标准字号。
    /// 执行过程：项目顶部→指挥台顶部切项目→任务所有/指定项目→恢复所有→原生无障碍审计。
    /// 预期结果：项目选择不重复，任务范围合并到搜索框左侧；关闭项目不串任务，所有项目恢复首项。
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

    /// 合并搜索框在普通与最大辅助字号下都能切换项目，输入不被项目名挤出。
    /// 数据构造：同一隔离双项目，L与AX5两种原生字号；第二项目关闭，第一项目有已完成任务。
    /// 执行过程：直接进入任务页 → 检查同一行与44pt触控区域 → 切到第二项目 → 恢复所有项目。
    /// 预期结果：项目只有一个选择入口；辅助字号允许两行，关闭项目不串任务，文字裁切/触控/描述审计通过。
    @MainActor func testCombinedTaskSearchSizesAndProjectScopes() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"],
              FileManager.default.fileExists(atPath: URL(fileURLWithPath: path).deletingLastPathComponent().appendingPathComponent("ui-project-navigation").path) else {
            throw XCTSkip("仅显式隔离双项目fixture")
        }
        for (category, name) in [("UICTContentSizeCategoryL", "standard"), ("UICTContentSizeCategoryAccessibilityXXXL", "AX5")] {
            let app = XCUIApplication()
            app.launchArguments += ["-UIPreferredContentSizeCategoryName", category]
            app.launch()
            try checkTaskScope(app, name: name)
            app.terminate()
        }
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
        XCTAssertFalse(app.buttons["切换项目所在电脑"].exists, "任何字号下都不重复显示电脑切换行")
        XCTAssertTrue(app.buttons["create-card-toolbar"].isHittable, "辅助字号也应保留可点击的创建入口")
        try checkHeader(app, selector: project, below: app.buttons["card-filters"], name: "项目-\(name)")

        app.buttons["workspace-tab-command"].tap()
        let plan = app.buttons["realtime-project"]
        XCTAssertTrue(plan.waitForExistence(timeout: 8), "陪伴页必须持续显示本次通话项目")
        XCTAssertTrue(app.buttons["realtime-settings"].isHittable, "通话设置可直接打开")
        choose("第二验收项目", from: plan, in: app)
        XCTAssertEqual(plan.value as? String, "第二验收项目", "未开聊时显式切换目标项目")
        XCTAssertFalse(app.buttons["realtime-call-end"].exists, "浏览和切换项目不能自动开始通话")
        choose("通知验收项目", from: plan, in: app)
        XCTAssertEqual(plan.value as? String, "通知验收项目", "切回首项目仍可准确显示")
        app.buttons["command-decision-inbox"].tap()
        XCTAssertTrue(app.buttons["command-decisions-close"].waitForExistence(timeout: 3), "全部电脑的审批改从原生弹层进入")
        XCTAssertFalse(app.buttons["command-mode-plan"].exists, "弹层不能复活旧版布局")
        app.buttons["command-decisions-close"].tap()

        try checkTaskScope(app, name: name)
    }

    @MainActor private func checkTaskScope(_ app: XCUIApplication, name: String) throws {
        app.buttons["workspace-tab-tasks"].tap()
        let scope = app.buttons["task-project-filter"]
        XCTAssertTrue(scope.waitForExistence(timeout: 8), "所有项目选择必须移到任务顶部")
        guard scope.exists else { return }
        XCTAssertEqual(scope.value as? String, "所有项目", "任务初始保留所有项目范围")
        let search = app.textFields["task-search"]
        try checkHeader(app, selector: scope, below: app.buttons["task-filter-all"], name: "任务-\(name)")
        XCTAssertEqual(scope.frame.midY, search.frame.midY, accuracy: 2, "项目选择和搜索须在同一行；2pt为布局取整余量")
        XCTAssertLessThan(scope.frame.maxX, search.frame.minX, "项目菜单不得覆盖搜索输入")
        choose("第二验收项目", from: scope, in: app)
        XCTAssertEqual(scope.value as? String, "第二验收项目", "顶部需要显示已选择范围")
        XCTAssertGreaterThanOrEqual(scope.frame.height, 44, "项目菜单保留Apple最小44pt触控高度")
        XCTAssertGreaterThanOrEqual(search.frame.width, 44, "长项目名与辅助字号下输入仍须保留至少44pt触控宽度")
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
