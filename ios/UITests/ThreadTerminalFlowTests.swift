import XCTest

final class ThreadTerminalFlowTests: XCTestCase {
    /// 原生任务页通过正式中继接管同一终端，系统键盘提交文字后交还。
    /// 数据构造：隔离fixture中1个任务/200行80列输出；光标列70=21字符前缀+49字符输入；draft=5字节，Return再产生1个Enter。
    /// 执行过程：原线程任务→终端→接管→输入draft/回车→查询确认→交还→返回。
    /// 预期结果：未接管不能输入；Mac确认后显示真实状态/右侧光标；交还后无可输入键盘，不新增任务。
    @MainActor func testTerminalControlAndSystemKeyboard() throws { try runTerminal(largeText: false) }

    /// 最大辅助字号下先输入3字节草稿，再切后台；恢复必须只读，明确重接管后补2字节/回车。
    /// 数据构造：同一任务，dra(3)+ft(2)=draft(5字节)，Mac只允许这5字节和1个Enter各一次。
    /// 执行过程：接管→保存dra→后台/恢复→确认旧接管已结束→重新接管→ft/Return→交还。
    /// 预期结果：不恢复旧输入权、不丢草稿，系统键盘与真实光标仍可见，交还按钮可操作。
    @MainActor func testBackgroundDraftAndLargeKeyboard() throws { try runTerminal(largeText: true) }

    @MainActor private func runTerminal(largeText: Bool) throws {
        continueAfterFailure = false
        guard let path = ProcessInfo.processInfo.environment["NORMA_THREAD_REVIEW_FIXTURE"],
              let fixture = try JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path))) as? [String: Any],
              let namespace = fixture["namespace"] as? String, let port = fixture["port"] as? Int else {
            throw XCTSkip("需要正式本机中继终端UI fixture")
        }
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = namespace
        app.launchEnvironment["NORMA_CLOUD_TEST_NAMESPACE"] = namespace
        app.launchEnvironment["NORMA_CLOUD_LOCAL_PORT"] = String(port)
        if largeText { app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
        app.launch(); app.buttons["workspace-tab-projects"].tap()
        let workspace = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-card-")).firstMatch
        //10秒=原生转场3秒+本机加载和测试调度7秒；不等待外部模型。
        XCTAssertTrue(workspace.waitForExistence(timeout: 10), "必须有本次隔离工作空间")
        workspace.tap()
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "work-thread-")).firstMatch.tap()
        let task = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-")).firstMatch
        XCTAssertTrue(task.waitForExistence(timeout: 10), "必须保留原任务")
        // 最多6次半屏滚动覆盖AX5线程卡片和任务头的3屏内容。
        for _ in 0..<6 where !task.isHittable { app.swipeUp() }
        task.tap()
        let terminal = app.buttons["task-mode-terminal"]
        XCTAssertTrue(terminal.waitForExistence(timeout: 10), "W06必须提供终端切换")
        for _ in 0..<6 where !terminal.isHittable { app.swipeUp() }
        terminal.tap()
        let claim = app.buttons["thread-terminal-claim"]
        XCTAssertTrue(claim.waitForExistence(timeout: 10), "查看终端不应自动接管")
        XCTAssertFalse(app.keyboards.firstMatch.exists, "未接管不能提前唤出可输入键盘")
        let readOnlyShot = XCTAttachment(screenshot: app.screenshot())
        readOnlyShot.name = "W06a-read-only-\(largeText ? "AX5" : "standard")"; readOnlyShot.lifetime = .keepAlways; add(readOnlyShot)
        claim.tap()
        let field = app.textFields["thread-terminal-input"]
        XCTAssertTrue(field.waitForExistence(timeout: 10), "接管完成后才能出现原生输入")
        field.tap()
        if largeText {
            field.typeText("dra")
            XCUIDevice.shared.press(.home)
            app.activate()
            XCTAssertTrue(claim.waitForExistence(timeout: 10), "后台返回只能读取，不得恢复旧接管")
            XCTAssertFalse(app.staticTexts["已接管，Agent 自动输入已暂停。"].exists, "后台返回不能残留已接管提示")
            claim.tap()
            XCTAssertTrue(field.waitForExistence(timeout: 10), "显式再次接管后才能输入")
            XCTAssertEqual(field.value as? String, "dra", "后台恢复必须保留未发送草稿")
            field.tap(); field.typeText("ft\n")
        } else { field.typeText("draft\n") }
        let returned = app.buttons["thread-terminal-release"]
        XCTAssertTrue(returned.waitForExistence(timeout: 10), "交还入口必须持续可访问")
        let confirmed = NSPredicate(format: "label CONTAINS %@", "已写入")
        expectation(for: confirmed, evaluatedWith: app.staticTexts["thread-terminal-notice"])
        waitForExpectations(timeout: 10)
        if largeText { app.scrollViews["thread-terminal-identity"].swipeLeft() }
        XCTAssertTrue(app.keyboards.firstMatch.exists, "Return提交后系统键盘必须保持，方便连续控制")
        let cursor = app.descendants(matching: .any)["thread-terminal-cursor"].firstMatch
        XCTAssertTrue(cursor.exists, "最新页必须显示Mac提供的真实光标")
        let viewport = app.scrollViews["thread-terminal-viewport"].firstMatch
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "W06a-controlled-keyboard-\(largeText ? "AX5" : "standard")"; shot.lifetime = .keepAlways; add(shot)
        let geometry = XCTAttachment(string: "viewport=\(viewport.frame); cursor=\(cursor.frame)")
        geometry.name = "W06a-output-geometry"; geometry.lifetime = .keepAlways; add(geometry)
        // 默认3行×20pt=60pt；AX5约2行×45pt=90pt，不能只有键盘而看不到终端。
        let minimumOutput = largeText ? 90.0 : 60.0
        XCTAssertGreaterThanOrEqual(viewport.frame.height, minimumOutput, "键盘展开仍需保留可读的终端输出空间")
        XCTAssertTrue(viewport.frame.contains(cursor.frame), "整个光标\(cursor.frame)必须在视口\(viewport.frame)内，不能只露出一角或留在屏幕外")
        XCTAssertLessThan(cursor.frame.maxY, app.keyboards.firstMatch.frame.minY, "真实光标不能被系统键盘遮住")
        XCTAssertTrue(returned.isHittable, "系统键盘展开时交还入口仍必须可操作")
        expectation(for: NSPredicate(format: "enabled == true"), evaluatedWith: returned)
        waitForExpectations(timeout: 10)
        returned.tap()
        XCTAssertTrue(claim.waitForExistence(timeout: 10), "交还须回到只读状态")
        XCTAssertFalse(app.textFields["thread-terminal-input"].exists, "旧输入权不可继续编辑终端")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertEqual(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-")).count, 1, "终端控制不能创建第二项任务")
    }
}
