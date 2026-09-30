import XCTest

final class ThreadExecutionFlowTests: XCTestCase {
    /// 任务详情显示收到过的真实状态序列，不用最终回复代替进展。
    /// 数据构造：显式隔离库中accepted、submitted、done三种回执，1条结果与1份成果。
    /// 执行过程：开原线程任务→查三条同步进展→返回→重开。
    /// 预期结果：顺序正确、没有伪造排队；任务和原线程始终各一份。
    @MainActor func testTaskShowsConfirmedProgress() throws {
        try progressFlow(largeText: false)
    }

    /// 最大辅助字号下仍可阅读进展、同步时间并返回原线程。
    /// 数据构造：同一隔离库的3条真实回执，系统Accessibility XXXL。
    /// 执行过程：打开任务→滚动阅读首末进展及时间说明→返回→重启再打开。
    /// 预期结果：进展不被安全区遮住，重启不增加任务、不补造排队状态。
    @MainActor func testTaskProgressWithAccessibilityText() throws {
        try progressFlow(largeText: true)
    }

    /// W06追问回原线程；对象和正文重启恢复，取消保留正文，明确发送不重复派工。
    /// 数据构造：隔离库中1个已完成任务；后端未授权，追问只能显示待发送。
    /// 执行过程：详情→回线程追问→输入含提及的正文→重启→取消→再次选择→发送。
    /// 预期结果：键盘与对象可达，草稿完整；只有原任务，追问状态独立，不能虚报Agent已收到。
    @MainActor func testFollowupComposerReturnsToOriginalThread() throws { try followupFlow(largeText: false) }

    /// 最大辅助字号下任务追问、对象取消和键盘发送仍可操作。
    /// 数据构造：同一隔离fixture，系统Accessibility XXXL；本次正文附UUID以免命中此前消息。
    /// 执行过程：按标准流程选择、重启、取消并发送。
    /// 预期结果：按钮不少于44pt，当前回复对象可见；不串线程、不复制任务。
    @MainActor func testFollowupComposerWithAccessibilityText() throws { try followupFlow(largeText: true) }

    @MainActor private func followupFlow(largeText: Bool) throws {
        continueAfterFailure = false
        guard let namespace = ProcessInfo.processInfo.environment["NORMA_EXECUTION_UI_NAMESPACE"],
              UUID(uuidString: namespace) != nil else { throw XCTSkip("需要显式隔离的任务fixture") }
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = namespace
        app.launchEnvironment["NORMA_CLOUD_TEST_NAMESPACE"] = UUID().uuidString
        if largeText { app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
        app.launch(); openThread(app)
        let card = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-")).firstMatch
        revealTask(card, in: app)
        card.tap()
        let followup = app.buttons["task-return-followup"]
        // 8秒=原生转场3秒+本地SQLite读取及测试调度5秒。
        XCTAssertTrue(followup.waitForExistence(timeout: 8), "已派工任务必须可回原线程追问")
        XCTAssertGreaterThanOrEqual(followup.frame.height, 44, "追问入口至少44pt")
        followup.tap()
        let cancel = app.buttons["thread-followup-cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 8), "回原线程应明确显示回复对象和取消入口")
        let input = app.textFields["thread-message-input"]
        XCTAssertTrue(input.isHittable, "原线程输入应避让键盘并可操作")
        // AX5：重复5次×8字=40字，再加提及/标记，超过输入框原先的5行，验证真实键盘空间。
        let text = "@Norma " + (largeText ? String(repeating: "请解释这次修改。", count: 5) : "解释这次修改 ") + String(UUID().uuidString.prefix(4))
        input.tap(); input.typeText(text)
        let target = app.descendants(matching: .any).matching(identifier: "thread-followup-target").firstMatch
        XCTAssertGreaterThanOrEqual(target.frame.minY, app.navigationBars.firstMatch.frame.maxY, "长正文不能把回复对象顶到导航后面")
        XCTAssertTrue(cancel.isHittable, "长正文时仍须能取消回复对象")
        XCTAssertLessThanOrEqual(app.buttons["thread-send"].frame.maxY, app.keyboards.firstMatch.frame.minY, "发送按钮不能被系统键盘遮住")
        attach(app, "P05-followup-keyboard-\(largeText ? "AX5" : "standard")")
        app.terminate(); app.launch(); openThread(app)
        XCTAssertTrue(cancel.waitForExistence(timeout: 8), "重启仍显示原任务对象")
        XCTAssertEqual(input.value as? String, text, "重启恢复追问原文")
        cancel.tap()
        XCTAssertFalse(cancel.exists, "取消对象后不保留隐形追问")
        XCTAssertEqual(input.value as? String, text, "取消对象不得清掉正文")
        revealTask(card, in: app)
        card.tap(); followup.tap()
        XCTAssertEqual(input.value as? String, text, "再次选择保留已有正文")
        app.buttons["thread-send"].tap()
        let status = app.staticTexts.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-followup-status-")).allElementsBoundByIndex
        XCTAssertFalse(status.isEmpty, "来源消息之外必须有独立追问状态")
        XCTAssertTrue(status.allSatisfy { $0.label.contains("待发送") }, "离线fixture不能虚报Agent已收到")
        XCTAssertFalse(cancel.exists, "成功本地保存后清除发送时的对象")
        attach(app, "P05-followup-pending-\(largeText ? "AX5" : "standard")")
        revealTask(card, in: app)
        XCTAssertEqual(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-")).count, 1, "追问不能创建新任务卡")
    }

    @MainActor private func progressFlow(largeText: Bool) throws {
        continueAfterFailure = false
        guard let namespace = ProcessInfo.processInfo.environment["NORMA_EXECUTION_UI_NAMESPACE"],
              UUID(uuidString: namespace) != nil else { throw XCTSkip("需要显式隔离的任务进展fixture") }
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = namespace
        app.launchEnvironment["NORMA_CLOUD_TEST_NAMESPACE"] = UUID().uuidString
        if largeText { app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
        app.launch(); openThread(app)
        let card = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-")).firstMatch
        revealTask(card, in: app)
        card.tap()
        // 8秒 = 原生转场3秒 + 本机存储与测试调度余量5秒。
        XCTAssertTrue(app.staticTexts["thread-task-status"].waitForExistence(timeout: 8), "任务详情应可打开")
        XCTAssertEqual(app.staticTexts["task-detail-heading"].label, "修复首屏交接", "任务标题应作为首要内容，不能以Agent名称替代")
        XCTAssertEqual(app.staticTexts["task-detail-machine"].label, "Mac", "应显示任务所属电脑，不借用当前选择的其他电脑")
        XCTAssertTrue(app.staticTexts["task-latest-progress"].exists, "真实结果应进入最新进展分区")
        attach(app, "W06-task-header-\(largeText ? "AX5" : "standard")")
        let progress = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH %@", "task-progress-event-"))
        XCTAssertEqual(progress.count, 3, "每次实际状态变化应在任务详情保留一条进展")
        XCTAssertTrue(progress.element(boundBy: 0).label.contains("Mac 已接收"), "第一条来自派工接收回执")
        XCTAssertTrue(progress.element(boundBy: 1).label.contains("执行中"), "第二条来自运行回执")
        XCTAssertTrue(progress.element(boundBy: 2).label.contains("执行完成"), "第三条来自最终执行状态")
        XCTAssertFalse(app.staticTexts["排队中"].exists, "未观察到排队不能补造进展")
        XCTAssertTrue(app.staticTexts["task-progress-time-note"].exists, "必须说明时间为同步时间")
        reveal(progress.element(boundBy: 0), in: app)
        attach(app, "W06-confirmed-progress-\(largeText ? "AX5" : "standard")-start")
        reveal(app.staticTexts["task-progress-time-note"], in: app)
        attach(app, "W06-confirmed-progress-\(largeText ? "AX5" : "standard")-end")
        reveal(app.staticTexts["thread-task-result-text"], in: app)
        attach(app, "W06-latest-progress-\(largeText ? "AX5" : "standard")")
        if largeText {
            reveal(app.staticTexts["thread-task-result-text"], in: app, bottomEdge: true)
            attach(app, "W06-latest-progress-AX5-end")
        }
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.textFields["thread-message-input"].waitForExistence(timeout: 8), "返回仍是原线程输入")
        XCTAssertEqual(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-")).count, 1, "查看进展不能创建另一项任务")
        app.terminate(); app.launch(); openThread(app)
        revealTask(card, in: app)
        card.tap()
        XCTAssertTrue(app.staticTexts["thread-task-status"].waitForExistence(timeout: 8), "重启后原任务仍可打开")
        XCTAssertEqual(progress.count, 3, "重启不会重复生成进展")
    }

    @MainActor private func revealTask(_ card: XCUIElement, in app: XCUIApplication) {
        // LazyVStack会卸载屏外内容。16次×约半屏=8屏，覆盖AX5下原回复及本组测试最多6条追问。
        for _ in 0..<16 {
            if card.exists && card.isHittable { return }
            app.swipeDown()
        }
        XCTFail("滚回历史后原任务仍不可达")
    }

    @MainActor private func reveal(_ item: XCUIElement, in app: XCUIApplication, bottomEdge: Bool = false) {
        // 最多12次×半个可读区=6个可读屏；拖动在导航/操作栏之间，末尾保持0.15秒消除惯性越过目标。
        for _ in 0..<12 {
            let top = app.navigationBars.firstMatch.frame.maxY + 8
            let actions = app.buttons["task-return-thread"]
            let bottom = actions.exists ? actions.frame.minY - 12 : app.frame.maxY - 60
            let height = bottom - top
            guard height >= 44 else { XCTFail("操作栏与导航之间必须保留可阅读内容的区域"); return }
            let frame = item.frame
            let long = frame.height > height
            if long ? (bottomEdge ? frame.maxY <= bottom && frame.maxY > top : frame.minY >= top && frame.minY < bottom)
                : frame.minY >= top && frame.maxY <= bottom { return }
            let offset = long ? (bottomEdge ? frame.maxY - bottom : frame.minY - top)
                : frame.midY - (top + bottom) / 2
            let step = max(-height / 2, min(height / 2, offset))
            let y = step > 0 ? bottom - 8 : top + 8
            let origin = app.coordinate(withNormalizedOffset: .zero)
            let x = app.frame.width * 0.85
            origin.withOffset(CGVector(dx: x, dy: y)).press(forDuration: 0.05,
                thenDragTo: origin.withOffset(CGVector(dx: x, dy: y - step)), withVelocity: .slow, thenHoldForDuration: 0.15)
        }
        XCTFail("内容滚动后仍未进入可读区：\(item.frame)")
    }

    /// AX5下长任务标题截断时，完整指令可展开、滚到末尾，重开仍保留原文。
    /// 数据构造：隔离库中已有1位开发Agent；重复8次短句使指令超过三行标题，追加唯一末尾标记。
    /// 执行过程：UI新建线程→直接@开发发送长指令→任务详情→展开并滚到末尾→返回重开。
    /// 预期结果：完整正文逐字相同，末尾可滚到；没有新副本或虚构Mac接收。
    @MainActor func testLongInstructionCanBeExpandedWithAccessibilityText() throws {
        continueAfterFailure = false
        guard let namespace = ProcessInfo.processInfo.environment["NORMA_EXECUTION_UI_NAMESPACE"],
              UUID(uuidString: namespace) != nil else { throw XCTSkip("需要显式隔离库") }
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = namespace
        app.launchEnvironment["NORMA_CLOUD_TEST_NAMESPACE"] = UUID().uuidString
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        app.launch(); app.buttons["workspace-tab-projects"].tap()
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-card-")).firstMatch.tap()
        app.buttons["create-work-thread"].tap()
        app.textFields["thread-title"].tap(); app.textFields["thread-title"].typeText("长指令·" + UUID().uuidString.prefix(4))
        app.buttons["thread-create-save"].tap(); app.buttons["thread-mention"].tap()
        let agent = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-mention-agent-")).firstMatch
        if !agent.isHittable { app.swipeUp() }
        agent.tap()
        let instruction = String(repeating: "检查导航与返回路径。", count: 8) + "末尾验收标记。"
        app.textFields["thread-message-input"].typeText(instruction)
        app.buttons["thread-send"].tap()
        let cards = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-"))
        // 8秒 = 原生转场3秒 + 本机保存/测试调度5秒；不等待外部模型。
        XCTAssertTrue(cards.firstMatch.waitForExistence(timeout: 8), "长指令必须真实保存为待发任务")
        cards.firstMatch.tap()
        XCTAssertEqual(app.staticTexts["thread-task-status"].label, "待发送", "未连接Mac不能显示已接收")
        XCTAssertFalse(app.buttons["task-return-followup"].isEnabled, "尚未派工的任务不能追问一个不存在的会话")
        XCTAssertTrue(app.buttons["task-return-thread"].isEnabled, "追问不可用时仍须允许返回原线程")
        attach(app, "W06-followup-disabled-AX5")
        // SwiftUI DisclosureGroup gives its identifier to expanded child text as well as its button.
        let full = app.staticTexts["task-full-instruction"]
        XCTAssertFalse(full.exists, "全文默认折叠，标题截断不能冒充已展开")
        let disclosure = app.buttons["task-full-instruction"]
        reveal(disclosure, in: app); disclosure.tap()
        XCTAssertEqual(full.label, "@开发 " + instruction, "展开必须逐字展示原指令，不能用标题截断内容替代")
        attach(app, "W06-long-instruction-AX5-open")
        // 一次滚动约半屏；最多12次覆盖AX5正文91字（10×8+末尾7+提及4），不能无界轮询。
        for _ in 0..<12 {
            if full.frame.maxY <= app.frame.maxY - 60 { break }
            app.swipeUp()
        }
        XCTAssertLessThanOrEqual(full.frame.maxY, app.frame.maxY - 60, "完整指令的末尾必须能滚到Home安全区上方")
        attach(app, "W06-long-instruction-AX5-end")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertEqual(cards.count, 1, "阅读全文不能追加另一项任务")
        cards.firstMatch.tap(); reveal(disclosure, in: app); disclosure.tap()
        XCTAssertEqual(full.label, "@开发 " + instruction, "重开后仍能读取完整原文")
    }

    /// 已保存的 Agent 回复应具名显示，完成任务可回看成果记录，重启不会重复或回到待发。
    /// 数据构造：公开 Store 生成的隔离 fixture；1 条用户消息、1 条 Agent 回复、1 个完成任务。
    ///   最多 8 秒 = 原生启动/转场 3 秒 + 本机存储与测试调度余量 5 秒。
    /// 执行过程：进入空间/线程 → 读具名回复 → 看任务详情/成果 → 返回输入 → 重启回原线程。
    /// 预期结果：作者和完成态准确，文件数量来自持久记录；没有额外回复、待发任务或假验收。
    @MainActor func testCompletedReplyAndResultRecordSurviveNavigationAndRestart() throws {
        try runFlow(largeText: false)
    }

    /// 最大辅助字号下执行结果、作者、成果说明与返回仍可达。
    /// 数据构造：同一只读 UI fixture，系统 Accessibility XXXL，结果正文允许自然换行。
    /// 执行过程：完整重复标准字号流程，并滚动到成果说明。
    /// 预期结果：保留作者和任务身份，不因字号变大隐藏结果或返回入口。
    @MainActor func testCompletedReplyWithAccessibilityText() throws { try runFlow(largeText: true) }

    /// 本地云配置加载失败不能冒充“尚未授权”，必须保留可诊断的初始化错误。
    /// 数据构造：原成果fixture；开发端口0不合法，UUID命名空间合法，不访问网络。
    /// 执行过程：启动App→原任务→成果→观察初始化错误。
    /// 预期结果：显示配置无效，不显示缺少云授权，也不展示部分文件或发起新任务。
    @MainActor func testReviewPreservesCloudInitializationFailure() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "thread-execution-ui" else { throw XCTSkip("需要显式本机fixture") }
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = "ECBB821F-7AE7-4D44-ACB5-B92E839BE954"
        app.launchEnvironment["NORMA_CLOUD_LOCAL_PORT"] = "0"
        app.launchEnvironment["NORMA_CLOUD_TEST_NAMESPACE"] = UUID().uuidString
        app.launch()
        openThread(app)
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-")).firstMatch.tap()
        let openReview = app.buttons["thread-task-open-review"]
        if !openReview.isHittable { app.swipeUp() }
        openReview.tap()
        let error = app.staticTexts["thread-review-error"]
        XCTAssertTrue(error.waitForExistence(timeout: 8), "初始化失败必须在成果页可见")
        XCTAssertTrue(error.label.contains("本地联调配置无效"), "必须保留初始化错误，不误导用户重新授权")
        XCTAssertFalse(error.label.contains("尚未获得"), "配置失败不是权限被撤销")
        XCTAssertFalse(app.staticTexts["thread-review-file-count"].exists, "初始化失败不能展示文件")
    }

    @MainActor private func runFlow(largeText: Bool) throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "thread-execution-ui" else { throw XCTSkip("需要显式准备的本机结果 UI fixture") }
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = "ECBB821F-7AE7-4D44-ACB5-B92E839BE954"
        if largeText { app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
        app.launch()
        openThread(app)
        let reply = app.otherElements.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-agent-message-")).firstMatch
        XCTAssertTrue(reply.waitForExistence(timeout: 8), "Agent 回复须有独立作者语义，不能标为你发送的消息")
        XCTAssertTrue(reply.label.contains("开发"), "必须显示派工时的 Agent 名称")
        XCTAssertFalse(reply.label.contains("待发送"), "已收到的回复不能显示待发送")
        XCTAssertEqual(reply.value as? String, "部分回复，完整内容保留在 Mac", "VoiceOver 必须知道当前只读到了部分回复")
        attach(app, "P05-execution-reply")
        let card = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-")).firstMatch
        if !card.isHittable { app.swipeDown() }
        XCTAssertTrue(card.label.contains("执行完成"), "状态来自 execution，不能停在 Agent 已接收")
        let originalCardY = card.frame.minY
        card.tap()
        XCTAssertTrue(app.staticTexts["thread-task-status"].waitForExistence(timeout: 8), "任务详情应可打开")
        XCTAssertEqual(app.staticTexts["thread-task-status"].label, "执行完成", "完成不等于用户验收")
        let result = app.staticTexts["thread-task-result-text"]
        if !result.isHittable { app.swipeUp() }
        XCTAssertTrue(result.exists, "详情须显示保存的完整预览")
        let delivery = app.staticTexts["thread-task-delivery-summary"]
        if !delivery.isHittable { app.swipeUp() }
        XCTAssertTrue(delivery.label.contains("1 个文件"), "成果数量必须读取原回合记录")
        let openReview = app.buttons["thread-task-open-review"]
        if !openReview.isHittable { app.swipeUp() }
        XCTAssertTrue(openReview.exists, "已冻结成果必须能进入文件查看，不能只提示去 Mac")
        attach(app, "P05-execution-detail")
        openReview.tap()
        XCTAssertTrue(app.staticTexts["thread-review-error"].waitForExistence(timeout: 8), "没有配对凭据的隔离任务必须说明错误，不能伪造文件")
        XCTAssertFalse(app.staticTexts["thread-review-file-count"].exists, "读取失败不能展示半份成果")
        app.buttons["thread-review-retry"].tap()
        XCTAssertTrue(app.staticTexts["thread-review-error"].waitForExistence(timeout: 8), "重试失败仍需可见且不重新派工")
        attach(app, "W09-review-unavailable")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()
        let input = app.textFields["thread-message-input"]
        XCTAssertTrue(input.waitForExistence(timeout: 8), "详情返回必须回到原线程")
        // Each snapshot may round by 2pt; 2 + 2 = 4pt allows rounding but rejects a real scroll jump.
        XCTAssertEqual(card.frame.minY, originalCardY, accuracy: 4, "返回后应保留线程阅读位置")
        app.terminate(); app.launch(); openThread(app)
        XCTAssertEqual(app.otherElements.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-agent-message-")).count, 1, "重启不得复制回复")
    }

    @MainActor private func openThread(_ app: XCUIApplication) {
        app.buttons["workspace-tab-projects"].tap()
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-card-")).firstMatch.tap()
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "work-thread-row-")).matching(NSPredicate(format: "label CONTAINS %@", "交接")).firstMatch.tap()
    }

    @MainActor private func attach(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }
}
