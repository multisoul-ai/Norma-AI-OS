import XCTest

final class WorkspaceAssociationFlowTests: XCTestCase {
    /// 通过正式 TLS 目录把项目与 Agent 关联到 A，重启保留，B 不继承；移除只改变关联。
    ///
    /// 数据构造：独立 Rust fixture 的 1 台 Mac、1 个项目和 Agent 定义 7；本机 Workspace A/B 各 1 个。
    ///   8 秒 = 原生转场约 3 秒 + 本地 TLS 目录读取余量 5 秒；不调用 Provider 或派发任务。
    /// 执行过程：新建 A/B → 给 A 勾选项目和 Agent → 重启查看 A/B → 移除 A 项目 → 回电脑目录打开原项目。
    /// 预期结果：A 配置跨重启保留，B 无关联；移除 A 项目同时移除其成员；Mac 项目仍存在且能打开并返回。
    @MainActor func testProjectAndAgentAssociationsAreScopedAndPersistent() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui" else {
            throw XCTSkip("必须使用已配对的独立 Rust Mac fixture")
        }
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = UUID().uuidString
        app.launch()
        app.buttons["workspace-tab-projects"].tap()
        create("关联验收 A", in: app)
        create("关联验收 B", in: app)
        settings("关联验收 A", in: app).tap()
        app.buttons["workspace-add-project"].tap()
        let project = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-project-option-")).firstMatch
        XCTAssertTrue(project.waitForExistence(timeout: 8), "项目选择必须从已授权 Mac 取得真实目录")
        XCTAssertTrue(app.staticTexts["通知验收 Mac"].exists, "项目选择必须按准确电脑分组")
        project.tap()
        let pickerShot = XCTAttachment(screenshot: app.screenshot())
        pickerShot.name = "W03-live-project-picker"; pickerShot.lifetime = .keepAlways; add(pickerShot)
        app.buttons["workspace-picker-done"].tap()
        XCTAssertTrue(app.buttons["移除项目 通知验收项目"].waitForExistence(timeout: 8), "项目应关联到当前空间")
        app.buttons["workspace-add-agent"].tap()
        let agent = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-agent-option-")).firstMatch
        XCTAssertTrue(agent.waitForExistence(timeout: 8), "Agent 必须来自关联项目的真实定义目录")
        XCTAssertTrue(agent.label.contains("界面助手"), "不能用卡片名称或原型头像冒充 Agent 定义")
        agent.tap()
        app.buttons["workspace-picker-done"].tap()
        XCTAssertTrue(app.buttons["移除 Agent 界面助手"].waitForExistence(timeout: 8), "设置应保存被选中的项目 Agent")
        let settingsShot = XCTAttachment(screenshot: app.screenshot())
        settingsShot.name = "P04-live-project-agent"; settingsShot.lifetime = .keepAlways; add(settingsShot)
        app.terminate(); app.launch()
        app.buttons["workspace-tab-projects"].tap()
        settings("关联验收 B", in: app).tap()
        XCTAssertFalse(app.buttons["移除项目 通知验收项目"].exists, "B 不能继承 A 的项目")
        XCTAssertFalse(app.buttons["移除 Agent 界面助手"].exists, "B 不能继承 A 的 Agent")
        app.buttons["workspace-settings-back"].tap()
        settings("关联验收 A", in: app).tap()
        XCTAssertTrue(app.buttons["移除项目 通知验收项目"].waitForExistence(timeout: 8), "A 的关联须跨重启保留")
        XCTAssertTrue(app.buttons["移除 Agent 界面助手"].exists, "重启后 A 的 Agent 不能丢失")
        app.buttons["移除项目 通知验收项目"].tap()
        XCTAssertTrue(app.buttons["移除项目 通知验收项目"].waitForNonExistence(timeout: 8), "移除后不应保留项目关联")
        XCTAssertFalse(app.buttons["移除 Agent 界面助手"].exists, "移除项目不能留下悬空 Agent")
        app.buttons["workspace-settings-back"].tap()
        app.buttons["workspace-more-modules"].tap()
        app.buttons["已连接电脑"].tap()
        let original = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "computer-project-")).firstMatch
        XCTAssertTrue(original.waitForExistence(timeout: 8), "移除关联不能删除 Mac 项目")
        original.tap()
        XCTAssertTrue(app.buttons["computer-project-close"].waitForExistence(timeout: 8), "新导航必须仍能打开原项目，并有明确返回")
        app.buttons["computer-project-close"].tap()
        XCTAssertTrue(original.waitForExistence(timeout: 8), "关闭项目须返回同一电脑目录")
    }

    @MainActor private func create(_ name: String, in app: XCUIApplication) {
        app.buttons["create-workspace"].tap()
        app.textFields["workspace-name"].tap()
        app.textFields["workspace-name"].typeText(name)
        app.buttons["workspace-create-save"].tap()
        XCTAssertTrue(card(name, in: app).waitForExistence(timeout: 8), "创建后必须显示准确名称的 Workspace")
    }

    @MainActor private func card(_ name: String, in app: XCUIApplication) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@ AND label CONTAINS %@", "workspace-card-", name)).firstMatch
    }
    @MainActor private func settings(_ name: String, in app: XCUIApplication) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@ AND label CONTAINS %@", "workspace-configure-", name)).firstMatch
    }
}
