import XCTest

// 8s = 2s WSS snapshot allowance + 6s native navigation; 3s covers system menu presentation.

@MainActor func openCommandDecisions(in app: XCUIApplication) {
    let entry = app.buttons["command-decision-inbox"]
    scrollToWorkspaceRow(entry, in: app)
    XCTAssertTrue(entry.waitForExistence(timeout: 8), "原生指挥台应提供全部电脑待决定入口")
    entry.tap()
    XCTAssertTrue(app.buttons["command-decisions-close"].waitForExistence(timeout: 8), "应打开独立决定弹层")
}

@MainActor func openCommandPlans(in app: XCUIApplication) {
    let project = app.buttons["realtime-project"]
    XCTAssertTrue(project.waitForExistence(timeout: 8), "指挥台应显示通话项目")
    project.tap()
    let plans = app.buttons["command-project-plans"]
    XCTAssertTrue(plans.waitForExistence(timeout: 3), "项目菜单应保留共享方向与计划记录")
    plans.tap()
    XCTAssertTrue(app.buttons["direction-close"].waitForExistence(timeout: 8), "应打开共享方向页")
}

@MainActor func openCommandPlanRecord(_ direction: String, in app: XCUIApplication) {
    if !app.buttons["direction-close"].exists { openCommandPlans(in: app) }
    let history = app.buttons["direction-history"]
    scrollToWorkspaceRow(history, in: app)
    history.tap()
    let plan = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", direction)).firstMatch
    XCTAssertTrue(plan.waitForExistence(timeout: 8), "计划记录必须来自所选电脑的真实存储")
    scrollToWorkspaceRow(plan, in: app); plan.tap()
    XCTAssertTrue(app.navigationBars["计划与执行"].waitForExistence(timeout: 8), "共享计划详情应继续可访问")
}

@MainActor func closeCommandPlanRecord(in app: XCUIApplication) {
    // Two native back steps: plan detail → history → direction. No application state injection.
    for _ in 0..<2 where !app.buttons["direction-close"].isHittable {
        app.navigationBars.buttons.firstMatch.tap()
    }
    let close = app.buttons["direction-close"]
    XCTAssertTrue(close.isHittable, "共享详情应能沿原生导航返回方向页")
    close.tap()
    XCTAssertTrue(close.waitForNonExistence(timeout: 8), "关闭后不能残留计划弹层")
}
