import XCTest

final class WorkspaceTerminalFlowTests: XCTestCase {
    /// 场景：已有长消息和草稿时，直接打开执行详情仍能完成原生Sheet转场。
    /// 数据构造：隔离空间已有两条约140字的准备消息；8秒=原生转场3秒+连接余量5秒。
    /// 执行过程：启动已配对App → 打开隔离群聊 → 点最后一张Bella执行卡，不编辑草稿。
    /// 预期结果：详情及网格在8秒内显示；没有键盘；完成按钮可关闭Sheet。
    @MainActor func testOpenExecutionSheetWithLongMessages() throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_FIXTURE"], path.hasPrefix("/") else { throw XCTSkip("需要隔离fixture") }
        struct Fixture: Decodable { let workspaceID: String }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let app = XCUIApplication(); app.launch()
        XCTAssertTrue(app.buttons["workspace-tab-projects"].waitForExistence(timeout: 15), "必须启动正式App")
        app.buttons["workspace-tab-projects"].tap()
        let workspace = app.buttons["workspace-card-\(fixture.workspaceID)"]
        XCTAssertTrue(workspace.waitForExistence(timeout: 15), "必须打开隔离工作空间")
        workspace.tap()
        let card = app.buttons["group-receipt-24"]
        XCTAssertTrue(card.waitForExistence(timeout: 15), "显式验收空间的原执行卡必须存在")
        card.tap()
        XCTAssertTrue(app.navigationBars["执行详情"].waitForExistence(timeout: 8), "群聊文本布局不能阻塞原生详情转场")
        XCTAssertTrue(app.descendants(matching: .any)["workspace-terminal-grid"].firstMatch.waitForExistence(timeout: 8), "只读终端必须可见")
        XCTAssertFalse(app.keyboards.firstMatch.exists, "未编辑草稿不能唤起键盘")
        app.buttons["完成"].tap()
        XCTAssertFalse(app.navigationBars["执行详情"].exists, "完成应关闭详情")
    }

    /// 场景：群聊成果默认进入原生实时终端，阅读/复制/切换/旋转/返回保持草稿及原卡位置。
    /// 数据构造：显式隔离Mac当前执行卡cursor=24；未发送草稿；8秒=转场3秒+同步余量5秒。
    /// 执行过程：打开空间 → 输入草稿 → 打开原卡 → 上滑暂停 → 复制 → 切结果 → 返回终端 → 旋转 → 完成。
    /// 预期结果：两标签可用，终端为只读；回实时有明确入口；横屏不截断控件；草稿和原卡保持。
    @MainActor func testTerminalSheetReadingAndChatState() throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_FIXTURE"], path.hasPrefix("/") else { throw XCTSkip("需要隔离Mac终端验收fixture") }
        struct Fixture: Decodable { let workspaceID: String }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        XCUIDevice.shared.orientation = .portrait
        let app = XCUIApplication(); app.launch()
        XCTAssertTrue(app.buttons["workspace-tab-projects"].waitForExistence(timeout: 15), "底栏必须可进入工作空间")
        app.buttons["workspace-tab-projects"].tap()
        let workspace = app.buttons["workspace-card-\(fixture.workspaceID)"]
        XCTAssertTrue(workspace.waitForExistence(timeout: 15), "必须使用真实共享目录")
        workspace.tap()
        // The earlier result (cursor 19) belongs to a previous run and must not silently follow this run.
        let task = app.buttons["group-receipt-24"]
        XCTAssertTrue(task.waitForExistence(timeout: 15), "当前运行的原执行卡必须出现在群聊")
        let input = app.descendants(matching: .any)["group-message-input"].firstMatch
        input.tap()
        XCTAssertTrue((input.value as? String)?.contains("Terminal keeps this draft") == true, "先运行明确隔离的固定草稿准备测试")
        input.typeText(" · draft")
        let expectedDraft = input.value as? String
        // The scroll view's frame extends behind the keyboard. Start on its visible left margin,
        // rather than its center (which can hit the keyboard or a selectable message).
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.025, dy: 0.3))
            .press(forDuration: 0.05, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.025, dy: 0.62)))
        // The current execution card is the newest event, below the older preparation messages.
        for _ in 0..<6 where !task.isHittable { app.scrollViews["group-message-scroll"].swipeUp() }
        XCTAssertTrue(task.isHittable, "执行卡必须实际可点击")
        let originalY = task.frame.minY
        task.tap()
        XCTAssertTrue(app.navigationBars["执行详情"].waitForExistence(timeout: 8), "点击Agent卡应打开原生详情Sheet")
        XCTAssertTrue(app.buttons["终端"].isSelected, "详情默认进入终端")
        let grid = app.descendants(matching: .any)["workspace-terminal-grid"].firstMatch
        XCTAssertTrue(grid.waitForExistence(timeout: 15), "应显示Mac富终端网格")
        XCTAssertFalse(app.textFields["terminal-input"].exists, "只读终端不得添加输入入口")
        XCTAssertTrue(app.buttons["workspace-terminal-history"].isEnabled, "180行隔离PTY输出必须提供真实历史分页")
        app.buttons["workspace-terminal-history"].tap()
        XCTAssertTrue(app.buttons["workspace-terminal-live"].waitForExistence(timeout: 8), "读取历史后应能回到实时")
        let historyShot = XCTAttachment(screenshot: app.screenshot()); historyShot.name = "Workspace-terminal-history"; historyShot.lifetime = .keepAlways; add(historyShot)
        app.buttons["workspace-terminal-live"].tap()
        grid.swipeDown()
        XCTAssertTrue(app.buttons["workspace-terminal-live"].waitForExistence(timeout: 8), "向上阅读旧输出应暂停跟随")
        app.buttons["workspace-terminal-copy"].tap()
        XCTAssertEqual(app.buttons["workspace-terminal-copy"].label, "已复制终端输出", "复制应有即时状态反馈")
        app.buttons["结果"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["workspace-execution-result"].waitForExistence(timeout: 8), "结果正文仍可读取")
        app.buttons["终端"].tap()
        app.buttons["workspace-terminal-live"].tap()
        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue(app.buttons["workspace-terminal-copy"].isHittable, "横屏仍能复制输出")
        // 100pt约可显示5行（字号13的单行约18pt），横屏不能只剩工具栏下的两行。
        XCTAssertGreaterThanOrEqual(grid.frame.height, 100, "横屏应保留至少5行终端阅读区域")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "Workspace-terminal-landscape"; shot.lifetime = .keepAlways; add(shot)
        XCUIDevice.shared.orientation = .portrait
        app.buttons["完成"].tap()
        XCTAssertEqual(input.value as? String, expectedDraft, "关闭Sheet必须完整保留未发送草稿")
        XCTAssertEqual(task.frame.minY, originalY, accuracy: 3, "返回位置允许3pt系统取整，不能跳回聊天底部")
        XCTAssertFalse(app.navigationBars["执行详情"].exists, "完成后Sheet必须关闭")
    }

    /// 场景：旧成果只关联原运行；会话失效时结果仍能阅读，不能显示替代会话画面。
    /// 数据构造：隔离成果cursor=19使用已失效运行；当前执行卡cursor=24使用新的运行。
    /// 执行过程：进入群聊 → 向上找到旧成果 → 打开详情 → 等原会话拒绝 → 切换结果。
    /// 预期结果：提示会话已变化；不出现终端网格；原「终端验收完成」正文仍存在。
    @MainActor func testOldResultRetainsOriginalSessionAndReadableBody() throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_FIXTURE"], path.hasPrefix("/") else { throw XCTSkip("需要隔离fixture") }
        struct Fixture: Decodable { let workspaceID: String }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let app = XCUIApplication(); app.launch()
        XCTAssertTrue(app.buttons["workspace-tab-projects"].waitForExistence(timeout: 15), "必须启动正式App")
        app.buttons["workspace-tab-projects"].tap()
        let workspace = app.buttons["workspace-card-\(fixture.workspaceID)"]
        XCTAssertTrue(workspace.waitForExistence(timeout: 15), "原空间必须存在")
        workspace.tap()
        let old = app.buttons["group-task-19"]
        for _ in 0..<6 where !old.isHittable { app.scrollViews["group-message-scroll"].swipeDown() }
        XCTAssertTrue(old.isHittable, "应能向上阅读原成果")
        old.tap()
        let notice = app.staticTexts["workspace-terminal-notice"]
        XCTAssertTrue(notice.waitForExistence(timeout: 8), "原会话失效必须提供明确说明")
        XCTAssertTrue(notice.label.contains("原 Agent 会话已变化"), "不能用当前同名Agent替代原会话")
        XCTAssertFalse(app.descendants(matching: .any)["workspace-terminal-grid"].firstMatch.exists, "失效旧会话不得返回新网格")
        app.buttons["结果"].tap()
        XCTAssertTrue(app.staticTexts["终端验收完成"].waitForExistence(timeout: 8), "旧结果正文仍应可读")
        app.buttons["完成"].tap()
    }
}
