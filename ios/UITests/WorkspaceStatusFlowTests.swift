import XCTest

final class WorkspaceStatusFlowTests: XCTestCase {
    /// 三个Tab的读取失败与空项目提示使用相邻、居中的操作，重连后恢复原卡。
    /// 数据构造：隔离只读电脑；fixture关闭目录响应，再返回空目录，最后恢复真实项目。
    /// 执行过程：选择测试电脑→三Tab检查失败提示/按钮→切换电脑→空目录→重新读取恢复。
    /// 预期结果：操作紧跟说明，无底部漂浮按钮；未知不显示零任务；恢复后仍是同一原卡。
    @MainActor func testStandardStatusActionsAndRecovery() throws {
        try checkStatuses(category: "UICTContentSizeCategoryL", name: "standard")
    }

    /// 最大辅助字号下提示和按钮不裁切，三个页面仍可完成连接恢复。
    /// 数据构造：同一隔离目录故障/空目录，AX5为系统最大辅助字号；不操作用户电脑。
    /// 执行过程：复用三Tab故障与恢复流程→审计文字、触控和描述→保存原生截图。
    /// 预期结果：按钮靠近提示且可点击，项目恢复后无旧失败提示和错误空任务结论。
    @MainActor func testAccessibleStatusActionsAndRecovery() throws {
        try checkStatuses(category: "UICTContentSizeCategoryAccessibilityXXXL", name: "AX5")
    }

    @MainActor private func checkStatuses(category: String, name: String) throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("仅运行在显式隔离的通知fixture")
        }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        let offline = directory.appendingPathComponent("ui-catalog-offline")
        let empty = directory.appendingPathComponent("ui-catalog-empty")
        defer { try? FileManager.default.removeItem(at: offline); try? FileManager.default.removeItem(at: empty) }
        try Data().write(to: offline, options: .atomic)
        let app = XCUIApplication()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", category]
        app.launch(); selectFixture(app)
        dismissConnectionAlert(app)
        // 8s = 2s快照预算 + 6s原生界面调度余量，不作为物理网络性能指标。
        XCTAssertTrue(app.staticTexts["暂时无法读取项目"].waitForExistence(timeout: 8), "目录失败必须可见")
        try checkStatus(app, title: "暂时无法读取项目", action: "重新连接", secondary: "切换电脑", name: "项目失败-\(name)")
        app.buttons["workspace-tab-command"].tap()
        XCTAssertTrue(app.buttons["realtime-call-start"].waitForExistence(timeout: 8), "Mac不可用仍可进入独立语音入口")
        XCTAssertEqual(app.buttons["realtime-project"].value as? String, "未绑定项目", "目录失败不能伪造绑定项目")
        XCTAssertFalse(app.staticTexts["任务已完成"].exists, "没有回执不能伪造任务完成")
        app.buttons["workspace-tab-tasks"].tap()
        try checkStatus(app, title: "暂时无法读取任务", action: "重新连接", secondary: "切换电脑", name: "任务失败-\(name)")
        XCTAssertFalse(app.staticTexts["当前没有任务"].exists, "读取失败不能显示为没有任务")
        app.buttons["切换电脑"].tap()
        XCTAssertTrue(app.navigationBars["电脑"].exists, "次要操作须打开现有电脑管理")
        app.buttons["computers-close"].tap()

        try Data().write(to: empty, options: .atomic)
        waitForEmptyScope(true, directory: directory)
        try FileManager.default.removeItem(at: offline)
        app.buttons["重新连接"].tap()
        XCTAssertTrue(app.staticTexts["还没有可访问的项目"].waitForExistence(timeout: 8), "重连须读取真实空目录")
        guard app.staticTexts["还没有可访问的项目"].exists else { return }
        try checkStatus(app, title: "还没有可访问的项目", action: "重新读取", secondary: nil, name: "任务空目录-\(name)")
        app.buttons["workspace-tab-command"].tap()
        XCTAssertEqual(app.buttons["realtime-project"].value as? String, "未绑定项目", "空授权范围不能沿用其他项目")
        XCTAssertTrue(app.buttons["command-decision-inbox"].exists, "空项目仍可访问本机决定记录")
        app.buttons["workspace-tab-projects"].tap()
        try checkStatus(app, title: "还没有可访问的项目", action: "重新读取", secondary: nil, name: "项目空目录-\(name)")
        try FileManager.default.removeItem(at: empty)
        waitForEmptyScope(false, directory: directory)
        app.buttons["重新读取"].tap()
        XCTAssertTrue(app.buttons["agent-card-1"].waitForExistence(timeout: 8), "恢复后须显示原项目卡片")
        XCTAssertFalse(app.staticTexts["还没有可访问的项目"].exists, "成功读取后不能保留空态")
    }

    @MainActor private func waitForEmptyScope(_ empty: Bool, directory: URL) {
        let applied = directory.appendingPathComponent("ui-catalog-empty-applied")
        let expectation = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            FileManager.default.fileExists(atPath: applied.path) == empty
        }, object: nil)
        // 3s = 20ms fixture轮询 + 数据库授权写入/原生调度余量；先等正式授权变更再刷新。
        XCTAssertEqual(XCTWaiter.wait(for: [expectation], timeout: 3), .completed, "测试授权必须先切换到所需范围")
    }

    @MainActor private func selectFixture(_ app: XCUIApplication) {
        app.buttons["manage-computers"].tap()
        let computer = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        XCTAssertTrue(computer.waitForExistence(timeout: 8), "只能选择隔离电脑")
        computer.tap()
    }

    @MainActor private func dismissConnectionAlert(_ app: XCUIApplication) {
        // 3s覆盖失败回调与系统alert呈现，不修改生产超时。
        if app.alerts["连接提示"].waitForExistence(timeout: 3) { app.alerts.buttons["好"].tap() }
    }

    @MainActor private func checkStatus(_ app: XCUIApplication, title: String, action: String, secondary: String?, name: String) throws {
        let heading = app.staticTexts[title]
        let button = app.buttons[action]
        XCTAssertTrue(heading.waitForExistence(timeout: 8), "\(name)：提示应出现")
        XCTAssertTrue(button.exists, "\(name)：必须提供一致的恢复操作")
        guard button.exists else { return }
        XCTAssertTrue(button.isHittable, "\(name)：主按钮须可点击")
        let description = app.staticTexts["workspace-status-message"]
        XCTAssertTrue(description.exists, "\(name)：说明必须可读")
        // 64pt = 原生description/actions间距最多32pt + 32pt布局容差；不把多行正文计为空白。
        XCTAssertLessThan(button.frame.minY - description.frame.maxY, 64, "\(name)：操作必须紧跟说明")
        XCTAssertEqual(button.frame.midX, app.frame.midX, accuracy: 2, "\(name)：操作应居中，不能成为左对齐列表行")
        XCTAssertGreaterThanOrEqual(button.frame.height, 44, "\(name)：按钮至少44pt")
        if let secondary {
            let second = app.buttons[secondary]
            XCTAssertTrue(second.exists, "\(name)：保留切换电脑的恢复路径")
            if second.exists {
                // 最大辅助字号允许正常滚动；操作仍紧随正文，不要求压缩文字塞进一屏。
                if !second.isHittable { app.swipeUp() }
                XCTAssertTrue(second.isHittable, "\(name)：次要按钮可触达")
                XCTAssertLessThan(second.frame.minY - button.frame.maxY, 32, "\(name)：主次按钮属于同一组")
                XCTAssertEqual(second.frame.midX, button.frame.midX, accuracy: 2, "\(name)：主次按钮对齐")
            }
        }
        try app.performAccessibilityAudit(for: [.textClipped, .hitRegion, .sufficientElementDescription]) { issue in
            print("STATUS_AUDIT_\(name): \(issue.element?.debugDescription ?? app.debugDescription)")
            return false
        }
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }
}
