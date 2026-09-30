import XCTest

final class RealMacThreadFlowTests: XCTestCase {
    /// iOS真实页面派工给真实Mac上的Codex，并查看冻结文件、重启恢复原线程。
    /// 数据构造：临时配对已准备，空Workspace库；1个真实项目/Alice，1条UI发送指令，2个文件。
    /// 执行过程：建Workspace/关联项目与Alice→建线程/@Alice→发送→等真实成果→看文件→重启回原线程并重新联网读成果。
    /// 预算：180秒=CLI启动30秒+模型执行120秒+回传30秒；导航10秒=网络5秒+转场5秒。
    /// 预期结果：只一个完成任务和具名回复，2个准确文件，无自动验收；重启不新增任务/回复。
    @MainActor func testRealTaskFromPhoneUIAndRestore() throws {
        continueAfterFailure = false
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "thread-real-ui", let path = env["NORMA_THREAD_REAL_FIXTURE"] else {
            throw XCTSkip("需要显式启动真实Mac App、正式本地中继和隔离准入")
        }
        let config = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path))) as? [String: Any])
        let namespace = try XCTUnwrap(config["namespace"] as? String)
        let port = try XCTUnwrap(config["port"] as? Int)
        let machineID = try XCTUnwrap(config["machineID"] as? String)
        let projectID = try XCTUnwrap(config["projectID"] as? String)
        let definitionID = try XCTUnwrap(config["definitionID"] as? Int)
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = namespace
        app.launchEnvironment["NORMA_CLOUD_TEST_NAMESPACE"] = namespace
        app.launchEnvironment["NORMA_CLOUD_LOCAL_PORT"] = String(port)
        app.launchArguments = ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryL"]
        app.launch()
        app.buttons["workspace-tab-projects"].tap()
        app.buttons["create-workspace"].tap()
        app.textFields["workspace-name"].tap(); app.textFields["workspace-name"].typeText("真实 Mac 派工验收")
        app.buttons["workspace-create-save"].tap()
        let workspace = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-card-")).firstMatch
        XCTAssertTrue(workspace.waitForExistence(timeout: 10), "工作空间必须由页面保存")
        let workspaceID = workspace.identifier
        workspace.tap()
        app.buttons["thread-workspace-settings"].tap()
        app.buttons["workspace-add-project"].tap()
        let project = app.buttons.matching(NSPredicate(format: "identifier == %@", "workspace-project-option-\(machineID):\(projectID)")).firstMatch
        XCTAssertTrue(project.waitForExistence(timeout: 10), "须读取本轮真实Mac的项目，不能选择用户其他项目")
        project.tap(); app.buttons["workspace-picker-done"].tap()
        app.buttons["workspace-add-agent"].tap()
        let agent = app.buttons.matching(NSPredicate(format: "identifier == %@", "workspace-agent-option-\(machineID):\(projectID):\(definitionID)")).firstMatch
        XCTAssertTrue(agent.waitForExistence(timeout: 10), "必须从真实目录选择本轮Alice定义")
        XCTAssertTrue(agent.label.contains("Alice"), "目录应返回具名Codex")
        agent.tap(); app.buttons["workspace-picker-done"].tap()
        attach(app, "P04-real-Mac-association")
        app.buttons["workspace-settings-back"].tap()
        app.buttons["create-work-thread"].tap()
        app.textFields["thread-title"].tap(); app.textFields["thread-title"].typeText("真实文件交付")
        app.buttons["thread-create-save"].tap()
        app.buttons["thread-mention"].tap()
        app.buttons.matching(NSPredicate(format: "identifier == %@", "thread-mention-agent-\(machineID):\(projectID):\(definitionID)")).firstMatch.tap()
        let input = app.textFields["thread-message-input"]
        input.typeText("Write result.txt with exactly norma-real-codex followed by a newline. Append executed and a newline to executions.txt once. Modify only these two files. Reply NORMA_REAL_CODEX_OK. Do not commit git.")
        app.buttons["thread-send"].tap()
        let cards = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-"))
        XCTAssertTrue(cards.firstMatch.waitForExistence(timeout: 10), "真实发送必须生成一个任务")
        XCTAssertEqual(cards.count, 1, "一次发送只能产生一个任务")
        let taskID = cards.firstMatch.identifier
        cards.firstMatch.tap()
        attach(app, "W06-real-task-pending")
        let completed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label CONTAINS %@", "执行完成"), object: app.staticTexts["thread-task-status"])
        XCTAssertEqual(XCTWaiter.wait(for: [completed], timeout: 180), .completed, "必须等到真实Codex完成，接收/提交不是完成")
        XCTAssertTrue(app.staticTexts["thread-task-result-text"].label.contains("NORMA_REAL_CODEX_OK"), "必须展示真实Agent回复")
        let review = app.buttons["thread-task-open-review"]
        XCTAssertTrue(review.waitForExistence(timeout: 10), "真实冻结成果须可打开")
        reveal(review, in: app); attach(app, "W06-real-task-complete"); review.tap()
        let result = app.buttons["review-file-result.txt"]
        XCTAssertTrue(result.waitForExistence(timeout: 10), "必须经正式中继读取真实文件")
        XCTAssertTrue(app.staticTexts["thread-review-file-count"].label.contains("2 个文件"), "只能交付指定的两个文件")
        XCTAssertEqual(app.staticTexts["thread-review-status"].label, "待验收", "执行完成不能自动提交验收决定")
        XCTAssertTrue(app.buttons["thread-review-return"].exists, "用户必须能主动退回修改")
        XCTAssertTrue(app.buttons["thread-review-accept"].exists, "用户必须能主动验收")
        XCTAssertFalse(app.staticTexts["thread-review-accepted"].exists, "执行完成不得自动验收")
        attach(app, "W09-real-files")
        reveal(result, in: app); result.tap()
        app.buttons["review-version"].tap(); app.buttons["本次交付"].tap()
        XCTAssertEqual(app.textViews["review-text"].value as? String, "norma-real-codex\n", "实际文件内容必须通过冻结成果接口准确显示")
        attach(app, "W09-real-result-text")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        let count = app.buttons["review-file-executions.txt"]
        reveal(count, in: app); count.tap()
        app.buttons["review-version"].tap(); app.buttons["本次交付"].tap()
        XCTAssertEqual(app.textViews["review-text"].value as? String, "executed\n", "执行记录只能一行，不可重复执行")
        app.terminate(); app.launch()
        app.buttons["workspace-tab-projects"].tap(); app.buttons[workspaceID].tap()
        let thread = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "work-thread-row-")).firstMatch
        thread.tap()
        let replies = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-agent-message-"))
        XCTAssertTrue(replies.firstMatch.waitForExistence(timeout: 10), "重启后真实回复应长期保留")
        XCTAssertEqual(replies.count, 1, "重启不能重复插入Agent回复")
        XCTAssertTrue(replies.firstMatch.label.contains("Alice") && replies.firstMatch.label.contains("NORMA_REAL_CODEX_OK"), "回复须保留作者与真实内容")
        XCTAssertEqual(cards.count, 1, "重启不得产生新任务")
        XCTAssertTrue(app.buttons[taskID].label.contains("执行完成"), "原任务不能退回待发送")
        app.buttons[taskID].tap()
        reveal(review, in: app); review.tap()
        XCTAssertTrue(count.waitForExistence(timeout: 10), "重启后必须重新联网读取同一份Mac成果，不能仅验证本地缓存")
        XCTAssertEqual(app.staticTexts["thread-review-status"].label, "待验收", "重启后仍由用户决定验收")
        reveal(count, in: app); count.tap()
        app.buttons["review-version"].tap(); app.buttons["本次交付"].tap()
        XCTAssertEqual(app.textViews["review-text"].value as? String, "executed\n", "重启重连后仍应读取原执行记录；宿主runner随后另外检查Mac当前文件未被重复追加")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()
        attach(app, "P05-real-thread-restored")
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        if FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-cua-requested").path) {
            try Data().write(to: directory.appendingPathComponent("ui-cua-ready"), options: .atomic)
            let inspected = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
                FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-cua-finished").path)
            }, object: nil)
            // 360s = 6min for explicit Computer Use review; this does not extend the 180s execution limit.
            XCTAssertEqual(XCTWaiter.wait(for: [inspected], timeout: 360), .completed, "Computer Use须实际操作后明确结束，超时不算通过")
        }
    }

    @MainActor private func reveal(_ element: XCUIElement, in app: XCUIApplication) {
        // The fixture has at most two files; six half-page swipes cover the full detail page.
        for _ in 0..<6 where !element.isHittable { app.swipeUp() }
        XCTAssertTrue(element.isHittable, "目标入口必须可滚动到并操作")
    }
    @MainActor private func attach(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }
}
