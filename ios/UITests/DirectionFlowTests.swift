import XCTest

final class DirectionFlowTests: XCTestCase {
    /// 调整方向按原型预填当前目标，选择优先事项后发布真实有序建议，不派发Agent任务。
    /// 数据构造：只读项目两项待办，index1=整理界面；升级控制后提交1次；8s=2s心跳+6s原生导航余量。
    ///   原生菜单长按1s大于约0.5s触发阈值；3s覆盖菜单/键盘动画，8次滚动仅用于AX5长表单。
    /// 执行过程：打开只读编辑器 → 关闭 → 升级 → 选第二项 → 改目标/收键盘 → 提交 → 核对新方向和首项。
    /// 预期结果：权限不能绕过，选择/编辑不提交；原文和结构化优先项保留，新建议按所选顺序展示。
    @MainActor func testDirectionPrioritySubmissionAndReadOnly() throws {
        let (app, dir) = try setup()
        let entry = app.buttons["adjust-project-direction"]
        scrollToWorkspaceRow(entry, in: app); entry.tap()
        let field = app.descendants(matching: .any)["plan-direction"].firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 8), "调整方向应打开独立编辑面板")
        XCTAssertEqual(field.value as? String, "先核对连接，再整理界面", "必须预填当前方向，不给空框")
        let picker = app.buttons["plan-priority"]
        XCTAssertTrue(picker.exists, "原型要求显式选择优先推进的事项")
        XCTAssertFalse(app.buttons["generate-project-plan"].isEnabled, "只读不能提交方向")
        app.buttons["direction-close"].tap()
        try Data().write(to: dir.appendingPathComponent("ui-card-management"))
        entry.tap()
        let submit = app.buttons["generate-project-plan"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"), object: submit)], timeout: 8), .completed, "控制授权更新后才可提交")
        picker.tap(); app.buttons["plan-priority-1"].tap()
        field.tap()
        field.press(forDuration: 1)
        let selectAll = app.menuItems["Select All"]
        XCTAssertTrue(selectAll.waitForExistence(timeout: 3), "应显示原生全选菜单")
        selectAll.tap()
        field.typeText("先稳定连接，再验证界面")
        XCTAssertEqual(field.value as? String, "先稳定连接，再验证界面", "提交前应已完整替换目标文字")
        app.buttons["direction-dismiss-keyboard"].tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "编辑方向后可收起键盘")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-direction-requests.json").path), "选择或输入不能提前提交")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "direction-editor"; shot.lifetime = .keepAlways; add(shot)
        submit.tap()
        guard app.buttons["direction-close"].waitForNonExistence(timeout: 8) else {
            XCTFail("保存确认后应返回指挥台：" + app.debugDescription)
            return
        }
        XCTAssertTrue(app.staticTexts["先稳定连接，再验证界面"].waitForExistence(timeout: 8), "指挥台显示真实新方向")
        let first = app.buttons["todo-open-0"]
        scrollToWorkspaceRow(first, in: app)
        XCTAssertTrue(first.label.contains("整理界面布局"), "选中的第二项必须实际排到首位")
        XCTAssertFalse(first.label.contains("核对连接路径"), "不能只改文本但保持原顺序")
        let data = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("ui-direction-requests.json"))) as? [[String: Any]])
        XCTAssertEqual(data.count, 1, "一次确认只提交一次")
        XCTAssertEqual(data[0]["direction"] as? String, "先稳定连接，再验证界面", "保留目标原文")
        let priority = try XCTUnwrap(data[0]["priority"] as? [String: Any])
        XCTAssertEqual(priority["plan_id"] as? String, "ui-direction-plan", "必须绑定原计划")
        XCTAssertEqual(priority["todo_index"] as? Int, 1, "必须是明确选中的第二项")
    }

    /// 最大辅助字号下方向控件可滚动，窗口变化不能使用旧编辑状态提交。
    /// 数据构造：两项原计划、AX5字号、1次控制升级；8s=2s心跳+6s导航余量。
    /// 执行过程：打开方向 → 检查优先项与提交按钮可触达 → 替换窗口 → 核对禁用和草稿。
    /// 预期结果：控件不超出屏幕，旧窗口不能提交；目标文字保留，未生成任何命令。
    @MainActor func testAccessibleDirectionAndWindowChange() throws {
        let (app, dir) = try setup(accessibility: true)
        try Data().write(to: dir.appendingPathComponent("ui-card-management"))
        let entry = app.buttons["adjust-project-direction"]
        scrollToWorkspaceRow(entry, in: app); entry.tap()
        let picker = app.buttons["plan-priority"]
        XCTAssertTrue(picker.waitForExistence(timeout: 8), "辅助字号仍提供优先事项选择")
        let submit = app.buttons["generate-project-plan"]
        for _ in 0..<8 where !submit.isHittable { app.scrollViews.firstMatch.swipeUp() }
        XCTAssertTrue(submit.isHittable, "大字号确认按钮必须可滚动触达")
        XCTAssertLessThanOrEqual(submit.frame.maxX, app.frame.maxX, "按钮不能横向裁切")
        XCTAssertGreaterThanOrEqual(submit.frame.minX, app.frame.minX, "按钮左边界在屏幕内")
        let stale = dir.appendingPathComponent("ui-stale")
        defer { try? FileManager.default.removeItem(at: stale) }
        try Data().write(to: stale)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == false"), object: submit)], timeout: 8), .completed, "换窗口后旧方向提交必须禁用")
        let field = app.descendants(matching: .any)["plan-direction"].firstMatch
        XCTAssertEqual(field.value as? String, "先核对连接，再整理界面", "窗口变化不能清空用户目标")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-direction-requests.json").path), "仅查看和窗口变化不生成计划")
    }

    /// 两端同时调整时，迟到结果不得覆盖新计划，拒绝后仍保留手机输入。
    /// 数据构造：来源ui-direction-plan；模型返回前另一端发布peer-direction；手机仅提交1次。
    /// 执行过程：升级控制 → 打开方向 → 触发竞争并提交 → 读取拒绝 → 清理已确认查询 → 核对草稿与重载入口。
    /// 预期结果：明确显示未保存，优先项过期时禁止提交；没有自动重试或Agent任务。
    @MainActor func testDirectionRejectsConcurrentPublish() throws {
        let (app, dir) = try setup()
        try Data().write(to: dir.appendingPathComponent("ui-card-management"))
        let entry = app.buttons["adjust-project-direction"]
        scrollToWorkspaceRow(entry, in: app); entry.tap()
        let submit = app.buttons["generate-project-plan"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"), object: submit)], timeout: 8), .completed, "必须先核对控制授权")
        try Data().write(to: dir.appendingPathComponent("ui-direction-race"))
        submit.tap()
        XCTAssertTrue(app.staticTexts["另一端已更新方向，这次建议未保存。你的输入仍保留，请核对最新优先事项。"].waitForExistence(timeout: 8), "必须报告真实并发拒绝")
        app.buttons["direction-clear-result"].tap()
        let reload = app.buttons["direction-reload-priority"]
        XCTAssertTrue(reload.waitForExistence(timeout: 8), "不能静默替换用户选择的来源计划")
        XCTAssertFalse(submit.isEnabled, "旧优先事项必须重新核对")
        XCTAssertEqual(app.descendants(matching: .any)["plan-direction"].firstMatch.value as? String, "先核对连接，再整理界面", "拒绝后目标草稿必须保留")
        let data = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("ui-direction-requests.json"))) as? [[String: Any]])
        XCTAssertEqual(data.count, 1, "读取拒绝和清理查询均不得重发")
        reload.tap()
        XCTAssertTrue(submit.isEnabled, "用户显式核对最新计划后可再次编辑")
    }

    @MainActor private func setup(accessibility: Bool = false) throws -> (XCUIApplication, URL) {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else { throw XCTSkip("仅隔离方向fixture") }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-direction-flow").path) else { throw XCTSkip("方向模式未启用") }
        let app = XCUIApplication()
        if accessibility { app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
        app.launch(); app.buttons["workspace-tab-command"].tap()
        XCTAssertTrue(app.staticTexts["先核对连接，再整理界面"].waitForExistence(timeout: 8), "应读取两项真实存储建议")
        return (app, dir)
    }
}
