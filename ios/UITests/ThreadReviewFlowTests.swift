import XCTest
import Vision

final class ThreadReviewFlowTests: XCTestCase {
    /// 完成成果在原线程有独立待验收入口，打开即看到预览，完整指令按需展开。
    /// 数据构造：正式本机网络fixture的1个完成任务/1份冻结成果，仍未验收。
    /// 执行过程：线程→独立成果卡→预览→展开本次任务→返回；不先经过任务详情。
    /// 预期结果：只有1张原成果卡、原指令全文可读；没有自动验收或重复任务。
    @MainActor func testResultCardOpensPreviewDirectly() throws { try runResultEntry(largeText: false) }

    /// 最大辅助字号下同一成果入口、完整指令和原生返回仍可操作。
    /// 数据构造：同一只读fixture，Accessibility XXXL；10秒=网络5秒+调度5秒。
    /// 执行过程：重复直接成果导航，必要时滚动到预览与任务说明。
    /// 预期结果：入口/状态不截断，指令未丢失，返回原线程且不生成任务。
    @MainActor func testResultCardWithAccessibilityText() throws { try runResultEntry(largeText: true) }

    @MainActor private func runResultEntry(largeText: Bool) throws {
        continueAfterFailure = false
        let fixture = try fixtureApp(largeText: largeText)
        let app = fixture.app
        app.launch(); openThread(fixture)
        let entry = app.buttons["thread-delivery-\(fixture.operation)"]
        XCTAssertTrue(entry.waitForExistence(timeout: 10), "完成成果必须在原线程有独立入口，不强迫用户先进入任务详情")
        reveal(entry, in: app, towardHistory: true)
        XCTAssertTrue(entry.label.contains("待验收"), "执行完成后仍需显示明确的待验收状态")
        XCTAssertFalse(entry.label.contains("已验收"), "未收到Mac验收回执不能显示已验收")
        attach(app, largeText ? "P05-result-entry-AX5" : "P05-result-entry-standard")
        entry.tap()
        let preview = app.buttons["thread-review-preview"]
        XCTAssertTrue(preview.waitForExistence(timeout: 10), "入口直接进入原成果并读取真实冻结文件")
        if !largeText {
            // 标准首屏前40%留给导航和简短归属，成果预览必须在其后立即可见。
            XCTAssertLessThan(preview.frame.minY, app.frame.minY + app.frame.height * 0.4,
                              "完整任务指令不能挤占首屏，把预览推到页面下方")
        }
        XCTAssertFalse(app.staticTexts["thread-review-instruction"].exists, "原始指令默认折叠，让成果优先显示")
        XCTAssertEqual(app.staticTexts["thread-review-status"].label, "待验收", "查看成果不会自动验收")
        attach(app, largeText ? "W09-preview-first-AX5" : "W09-preview-first-standard")
        let details = app.buttons["thread-review-task-disclosure"]
        reveal(details, in: app); details.tap()
        let instruction = app.staticTexts["thread-review-instruction"]
        XCTAssertTrue(instruction.waitForExistence(timeout: 5), "任务原文可按需展开，不能为了缩短页面而删除")
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["NORMA_THREAD_REVIEW_FIXTURE"])
        let data = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path))) as? [String: Any])
        let task = try XCTUnwrap(data["task"] as? [String: Any])
        XCTAssertEqual(instruction.label, task["task"] as? String, "必须逐字保留原始指令")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.textFields["thread-message-input"].waitForExistence(timeout: 5), "一次返回必须回到原线程")
        XCTAssertEqual(app.buttons.matching(identifier: entry.identifier).count, 1, "阅读不会复制成果或再次派工")
        XCTAssertEqual(app.buttons.matching(identifier: "thread-task-\(fixture.operation)").count, 1, "原任务仍只有一份")
    }

    /// 正式本地中继把原任务冻结文件送进原生页面，查看版本/后台恢复/返回不产生新任务。
    /// 数据构造：1个完成任务、result.txt原文7字节，交付“结果\n”7字节×20000=140000字节。
    /// 执行过程：Workspace→线程→任务→成果→文件→原文/交付→后台恢复→逐层返回→重开成果。
    /// 预期结果：完整正文与原文准确、无暂存区选项、没有自动验收，返回仍为同一任务。
    @MainActor func testFrozenFilesThroughNativeNavigation() throws { try runFlow(largeText: false) }

    /// 最大辅助字号下同一成果流程仍可读可操作，菜单与文件正文保留独立滚动区。
    /// 数据构造：同一正式网络fixture，系统Accessibility XXXL；10秒=网络5秒+转场/调度5秒。
    /// 执行过程：重复成果、版本、后台和返回流程并留截图。
    /// 预期结果：关键入口可达、全文不裁剪成缩写、不额外派工；不声称全App大字号通过。
    @MainActor func testFrozenFilesWithAccessibilityText() throws { try runFlow(largeText: true) }

    /// 原生退回意见跨Sheet/重启保留，修改任务和验收回执经本机Mac回到原线程。
    /// 数据构造：1个未验收原成果，1条意见；原任务+修改任务=2张卡，只有新成果可通过。
    /// 执行过程：取消/重启恢复意见→发送→打开修改任务成果→通过→重启核对原线程。
    /// 预期结果：意见不丢、原成果已退回、新成果Mac已验收；没有重复任务或自动验收旧成果。
    @MainActor func testReturnRevisionThenAcceptAndRestore() throws { try runDecisionFlow(largeText: false) }

    /// 最大辅助字号仍能编辑意见、查看后续任务和确认新成果；使用另一份全新网络fixture。
    /// 数据构造：同上，系统Accessibility XXXL；30秒=最多4个5秒投递轮次+10秒调度余量。
    /// 执行过程：复用完整退回/验收/重启链路，保存大字号键盘与状态截图。
    /// 预期结果：截图中键盘上方能识别末词spacing.，正文/按钮可达；2个任务状态分别保留。
    @MainActor func testRevisionAndAcceptanceWithAccessibilityText() throws { try runDecisionFlow(largeText: true) }

    @MainActor private func runDecisionFlow(largeText: Bool) throws {
        continueAfterFailure = false
        let fixture = try fixtureApp(largeText: largeText)
        let app = fixture.app
        openReview(fixture)
        let returning = app.buttons["thread-review-return"]
        XCTAssertTrue(returning.waitForExistence(timeout: 10), "须先成功读取原成果才能决定")
        attach(app, largeText ? "W09-decisions-AX5" : "W09-decisions-standard")
        returning.tap()
        let input = app.descendants(matching: .any).matching(identifier: "thread-review-feedback-input").firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 5), "退回应打开原生意见Sheet")
        let feedback = "Please improve the first screen spacing."
        input.tap(); input.typeText(feedback)
        let keyboard = app.keyboards.firstMatch
        XCTAssertTrue(keyboard.waitForExistence(timeout: 5), "必须实际显示软件键盘来验证避让")
        XCTAssertEqual(input.value as? String, feedback, "输入全文必须保留，不能裁掉末句来消除遮挡")
        XCTAssertLessThanOrEqual(input.frame.maxY, keyboard.frame.minY, "编辑区底部必须在键盘上方；光标可见性另由截图复核")
        XCTAssertTrue(app.buttons["thread-review-feedback-send"].isHittable, "输入时发送仍可达")
        attach(app, largeText ? "W09-feedback-keyboard-AX5" : "W09-feedback-keyboard-standard")
        try assertFeedbackEndVisible(app, input: input, keyboard: keyboard)
        app.buttons["取消"].tap()
        app.terminate(); openReview(fixture)
        XCTAssertTrue(returning.waitForExistence(timeout: 10), "取消不能自动退回或验收")
        returning.tap()
        XCTAssertTrue(input.waitForExistence(timeout: 5), "重启后仍可打开意见")
        XCTAssertEqual(input.value as? String, feedback, "取消并重启必须保留准确意见")
        app.buttons["thread-review-feedback-send"].tap()
        let revision = app.buttons["thread-review-revision-task"]
        XCTAssertTrue(revision.waitForExistence(timeout: 10), "发送后原成果必须关联修改任务")
        XCTAssertFalse(app.buttons["thread-review-accept"].exists, "已退回原成果不能再次直接验收")
        revision.tap()
        let nextReview = app.buttons["thread-task-open-review"]
        XCTAssertTrue(nextReview.waitForExistence(timeout: 30), "Mac执行回执和修改成果须回到关联任务")
        reveal(nextReview, in: app); nextReview.tap()
        let accept = app.buttons["thread-review-accept"]
        XCTAssertTrue(accept.waitForExistence(timeout: 10), "新成果提供独立验收入口")
        XCTAssertTrue(accept.isEnabled, "完成且无冲突的新成果可以验收")
        accept.tap()
        let accepted = app.staticTexts["thread-review-accepted"]
        XCTAssertTrue(accepted.waitForExistence(timeout: 30), "只有Mac持久保存验收才能显示已验收")
        XCTAssertFalse(app.buttons["thread-review-return"].exists, "已验收成果不可再生成修改任务")
        attach(app, largeText ? "W09-accepted-AX5" : "W09-accepted-standard")
        app.terminate(); openReview(fixture)
        XCTAssertTrue(revision.waitForExistence(timeout: 10), "重启必须保留原成果后续任务")
        XCTAssertFalse(app.staticTexts["thread-review-accepted"].exists, "新成果验收不能串到原成果")
        revision.tap()
        XCTAssertTrue(app.staticTexts["thread-task-status"].label.contains("已验收"), "重启后任务读取同一Mac验收记录")
        reveal(nextReview, in: app); nextReview.tap()
        XCTAssertTrue(accepted.waitForExistence(timeout: 10), "重开新成果仍为已验收")
        // Revision review→revision task→original review→original task→original thread: 4 native backs.
        for _ in 0..<4 { app.navigationBars.buttons.element(boundBy: 0).tap() }
        let tasks = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-"))
        XCTAssertEqual(tasks.count, 2, "退回/重启/验收只能保留原任务和一个修改任务")
        XCTAssertEqual(app.buttons.matching(identifier: "thread-task-\(fixture.operation)").count, 1, "原任务唯一且仍在原线程")
        attach(app, largeText ? "W09-return-thread-AX5" : "W09-return-thread-standard")
    }

    @MainActor private func fixtureApp(largeText: Bool) throws -> (app: XCUIApplication, workspace: String, thread: String, operation: String) {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "thread-review-ui",
              let path = ProcessInfo.processInfo.environment["NORMA_THREAD_REVIEW_FIXTURE"],
              FileManager.default.fileExists(atPath: path) else { throw XCTSkip("需要显式准备的本机中继UI fixture") }
        let fixture = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path))) as? [String: Any])
        let namespace = try XCTUnwrap(fixture["namespace"] as? String)
        let port = try XCTUnwrap(fixture["port"] as? Int)
        let task = try XCTUnwrap(fixture["task"] as? [String: Any])
        let operation = try XCTUnwrap(task["operation_id"] as? String)
        let thread = try XCTUnwrap(task["thread_id"] as? String)
        let workspace = try XCTUnwrap(task["collaboration_workspace_id"] as? String)
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = namespace
        app.launchEnvironment["NORMA_CLOUD_LOCAL_PORT"] = String(port)
        app.launchEnvironment["NORMA_CLOUD_TEST_NAMESPACE"] = namespace
        app.launchArguments = ["-UIPreferredContentSizeCategoryName", largeText ? "UICTContentSizeCategoryAccessibilityXXXL" : "UICTContentSizeCategoryL"]
        return (app, workspace, thread, operation)
    }

    @MainActor private func openReview(_ fixture: (app: XCUIApplication, workspace: String, thread: String, operation: String)) {
        let app = fixture.app
        app.launch()
        openThread(fixture)
        let taskCard = app.buttons["thread-task-\(fixture.operation)"]
        reveal(taskCard, in: app, towardHistory: true); taskCard.tap()
        let openReview = app.buttons["thread-task-open-review"]
        reveal(openReview, in: app); openReview.tap()
    }

    @MainActor private func openThread(_ fixture: (app: XCUIApplication, workspace: String, thread: String, operation: String)) {
        let app = fixture.app
        app.buttons["workspace-tab-projects"].tap()
        let workspaceCard = app.buttons["workspace-card-\(fixture.workspace)"]
        reveal(workspaceCard, in: app); workspaceCard.tap()
        let threadRow = app.buttons["work-thread-row-\(fixture.thread)"]
        reveal(threadRow, in: app); threadRow.tap()
    }

    @MainActor private func runFlow(largeText: Bool) throws {
        let fixture = try fixtureApp(largeText: largeText)
        let app = fixture.app, operation = fixture.operation
        openReview(fixture)
        let openReview = app.buttons["thread-task-open-review"]
        let file = app.buttons["review-file-result.txt"]
        XCTAssertTrue(file.waitForExistence(timeout: 10), "必须经真实中继读到原冻结文件")
        reveal(file, in: app)
        XCTAssertTrue(app.staticTexts["thread-review-file-count"].label.contains("1 个文件"), "文件数来自完整成果")
        XCTAssertFalse(app.staticTexts["thread-review-error"].exists, "成功读取不应保留错误")
        XCTAssertTrue(app.buttons["thread-review-accept"].exists, "W09必须提供验收通过入口")
        XCTAssertTrue(app.buttons["thread-review-return"].exists, "W09必须提供退回修改入口")
        attach(app, largeText ? "W09-files-AX5" : "W09-files-standard")
        file.tap()
        let text = app.textViews["review-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 10), "文件正文必须使用可选择的原生文字视图")
        XCTAssertEqual(text.value as? String, "-before\n+结果\n", "默认对比必须属于原任务")
        attach(app, largeText ? "W09-diff-AX5" : "W09-diff-standard")
        app.buttons["review-version"].tap()
        XCTAssertFalse(app.buttons["暂存区"].exists, "冻结交付不能混入当前暂存区")
        app.buttons["原文 · 任务起点"].tap()
        XCTAssertEqual(text.value as? String, "before\n", "原文必须是冻结起点")
        app.buttons["review-version"].tap(); app.buttons["本次交付"].tap()
        XCTAssertEqual(text.value as? String, String(repeating: "结果\n", count: 20_000), "长正文必须完整，不只显示首片")
        XCUIDevice.shared.press(.home); app.activate()
        XCTAssertTrue(text.waitForExistence(timeout: 10), "后台恢复应保留已完整读取的文件")
        XCTAssertEqual(text.value as? String, String(repeating: "结果\n", count: 20_000), "恢复不能改为当前工作目录文件")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(file.waitForExistence(timeout: 10), "文件返回应保留成果列表")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.staticTexts["thread-task-status"].exists, "成果返回应回到原任务")
        reveal(openReview, in: app); openReview.tap()
        XCTAssertTrue(file.waitForExistence(timeout: 10), "重新打开成果应重新认证且读取相同文件")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertEqual(app.buttons.matching(NSPredicate(format: "identifier == %@", "thread-task-\(operation)")).count, 1, "查看成果不能重复派工")
    }

    @MainActor private func reveal(_ element: XCUIElement, in app: XCUIApplication, towardHistory: Bool = false) {
        // At AX5 a card can be 1 viewport tall; at most 6 swipes covers the bounded fixture page.
        // Threads start at the newest message; earlier task cards require the opposite gesture.
        for _ in 0..<6 where !element.isHittable {
            if towardHistory { app.swipeDown() } else { app.swipeUp() }
        }
        XCTAssertTrue(element.isHittable, "页面应能滚动到目标入口")
    }
    @MainActor private func attach(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }

    @MainActor private func assertFeedbackEndVisible(_ app: XCUIApplication, input: XCUIElement, keyboard: XCUIElement) throws {
        let screenshot = try XCTUnwrap(app.screenshot().image.cgImage, "必须检查真实绘制的文字，而非控件value")
        // Pixels = points × display scale. Only inspect the editor, so predictions cannot satisfy the assertion.
        let scale = CGFloat(screenshot.width) / app.frame.width
        let area = input.frame.intersection(CGRect(x: app.frame.minX, y: app.frame.minY,
            width: app.frame.width, height: keyboard.frame.minY - app.frame.minY))
        let pixels = CGRect(x: (area.minX - app.frame.minX) * scale, y: (area.minY - app.frame.minY) * scale,
                            width: area.width * scale, height: area.height * scale)
        let visible = try XCTUnwrap(screenshot.cropping(to: pixels), "需要截取键盘上方真实输入区，排除预测栏")
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.recognitionLanguages = ["en-US"]
        request.usesLanguageCorrection = false
        try VNImageRequestHandler(cgImage: visible).perform([request])
        let lines = (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }
        XCTAssertTrue(lines.contains { $0.lowercased().contains("spacing.") },
                      "意见末词spacing.必须完整绘制在键盘上方；控件保存了全文但看不见末行仍属失败。可见文字：\(lines)")
    }
}
