import XCTest

final class ThreadMentionFlowTests: XCTestCase {
    /// 从真实 Mac 目录配置成员，@草稿跨重启保留，发送后显示真实待发任务且详情可返回。
    /// 数据构造：隔离 TLS fixture 的 1 个项目/Agent；独立 Workspace 库，只有 1 条明确发送的消息。
    ///   8 秒 = 原生转场约 3 秒 + 本地目录/存储余量 5 秒；fixture 不提供云中继，不伪造接收或完成。
    /// 执行过程：建空间 → @空态进设置 → 关联项目/成员 → 选择成员 → 保存草稿/重启 → 发送 → 查看任务详情。
    /// 预期结果：项目与成员具名展示；重启后草稿可发送且仅 1 张待发卡，无 Agent 已接收或任务完成状态。
    @MainActor func testMentionDraftCreatesPendingTaskFromConfiguredMember() throws {
        try runMentionFlow(largeText: false)
    }

    /// 最大辅助字号下，成员选择/返回/发送/详情仍可操作，不丢目标或草稿。
    /// 数据构造：同一真实 TLS fixture、全新隔离库；Accessibility XXXL 为系统最大辅助字号。
    /// 执行过程：按正常字号的同一完整流程配置成员、跨重启发送并回看冻结目标。
    /// 预期结果：关键操作和目标可达，只有一个待发任务，无截断导致的身份丢失或假成功。
    @MainActor func testMentionFlowWithAccessibilityText() throws {
        try runMentionFlow(largeText: true)
    }

    @MainActor private func runMentionFlow(largeText: Bool) throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui" else {
            throw XCTSkip("需要正式 TLS 配对的隔离 Mac fixture")
        }
        let app = XCUIApplication()
        if largeText { app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = UUID().uuidString
        app.launch()
        app.buttons["workspace-tab-projects"].tap()
        app.buttons["create-workspace"].tap()
        app.textFields["workspace-name"].tap(); app.textFields["workspace-name"].typeText("移动端体验")
        app.buttons["workspace-create-save"].tap()
        let workspace = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-card-")).firstMatch
        XCTAssertTrue(workspace.waitForExistence(timeout: 8), "应创建隔离工作空间")
        workspace.tap()
        app.buttons["create-work-thread"].tap()
        app.textFields["thread-title"].tap(); app.textFields["thread-title"].typeText("改进首屏交接")
        app.buttons["thread-create-save"].tap()
        app.buttons["thread-work-brief"].tap()
        app.buttons["brief-edit-goal"].tap()
        app.textViews["brief-fact-editor"].tap(); app.textViews["brief-fact-editor"].typeText("让首屏只突出下一步")
        app.buttons["brief-fact-save"].tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.buttons["thread-mention"].tap()
        app.buttons["thread-mention-settings"].tap()
        XCTAssertTrue(app.buttons["workspace-add-project"].waitForExistence(timeout: 8), "空态应直接进入本工作空间设置")
        app.buttons["workspace-add-project"].tap()
        let project = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-project-option-")).firstMatch
        XCTAssertTrue(project.waitForExistence(timeout: 8), "应读取真实授权目录")
        project.tap(); app.buttons["workspace-picker-done"].tap()
        app.buttons["workspace-add-agent"].tap()
        let agent = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-agent-option-")).firstMatch
        XCTAssertTrue(agent.waitForExistence(timeout: 8), "成员须来自已关联项目目录")
        agent.tap(); app.buttons["workspace-picker-done"].tap()
        app.buttons["workspace-settings-back"].tap()
        app.buttons["thread-mention"].tap()
        let choice = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-mention-agent-")).firstMatch
        XCTAssertTrue(choice.waitForExistence(timeout: 8), "刚配置的成员应立即可选")
        XCTAssertTrue(choice.label.contains("界面助手"), "选择器须显示目录中的真实成员名称")
        attach(app, "P05-mention-project-picker")
        choice.tap()
        let input = app.textFields["thread-message-input"]
        XCTAssertTrue(input.waitForExistence(timeout: 8), "选择后应返回原输入框")
        input.typeText("核对首屏导航与信息层级")
        XCTAssertEqual(input.value as? String, "@界面助手 核对首屏导航与信息层级", "选择应插入完整具名提及并恢复输入焦点")
        let selected = app.staticTexts.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-selected-agent-")).firstMatch
        XCTAssertTrue(selected.label.contains("界面助手 · 通知验收项目 · 通知验收 Mac"), "草稿应明确显示选中成员的项目与电脑")
        app.buttons["thread-mention"].tap()
        XCTAssertEqual(choice.value as? String, "已选择", "选择器须显示真实已选状态")
        attach(app, "P05-mention-selected-member")
        app.buttons["thread-mention-cancel"].tap()
        XCTAssertTrue(app.staticTexts["草稿已保存"].waitForExistence(timeout: 8), "重启前应有真实持久化反馈")
        app.terminate(); app.launch()
        app.buttons["workspace-tab-projects"].tap(); workspace.tap()
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "work-thread-row-")).firstMatch.tap()
        XCTAssertEqual(input.value as? String, "@界面助手 核对首屏导航与信息层级", "重启后正文与选择必须一起恢复")
        XCTAssertTrue(selected.label.contains("通知验收项目"), "重启后应恢复稳定目标的可见反馈")
        app.buttons["thread-send"].tap()
        let cards = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-"))
        XCTAssertTrue(cards.firstMatch.waitForExistence(timeout: 8), "明确呼叫应原子生成任务卡")
        XCTAssertEqual(cards.count, 1, "只有一次明确呼叫，不能因重启或刷新重复派工")
        XCTAssertTrue(cards.firstMatch.label.contains("界面助手"), "任务要保留具名目标")
        XCTAssertTrue(cards.firstMatch.label.contains("待发送"), "本地保存不能冒充 Agent 接收")
        XCTAssertFalse(cards.firstMatch.label.contains("Agent 已接收"), "没有回执时不能展示接收状态")
        attach(app, "P05-mentioned-task-pending")
        cards.firstMatch.tap()
        XCTAssertTrue(app.staticTexts["thread-task-status"].waitForExistence(timeout: 8), "任务卡必须打开真实详情")
        XCTAssertTrue(app.staticTexts["任务已保存在这部 iPhone，来源消息送达 Mac 后会继续派工。"].exists, "详情需准确解释待发条件")
        app.buttons["thread-task-context"].tap()
        XCTAssertTrue(app.staticTexts["让首屏只突出下一步"].waitForExistence(timeout: 8), "任务详情应可回看发送时共享的真实目标")
        attach(app, "P05-task-detail-pending")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(cards.firstMatch.waitForExistence(timeout: 8), "详情返回应保留原线程与任务")
        input.tap(); input.typeText("@不存在 请检查")
        app.buttons["thread-send"].tap()
        XCTAssertEqual(input.value as? String, "@不存在 请检查", "无效目标不能吞掉输入")
        XCTAssertEqual(cards.count, 1, "无效目标不能生成新任务")
    }

    @MainActor private func attach(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }
}
