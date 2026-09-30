import XCTest

final class WorkThreadFlowTests: XCTestCase {
    /// 未知 @成员的发送错误必须持续可见，后台投递刷新不能清掉仍未修正的输入错误。
    /// 数据构造：隔离 Workspace/线程，1 条待发消息使前台每 5 秒刷新；另有 1 条无效提及草稿。
    ///   8 秒 = 页面与键盘约 3 秒 + 存储余量 5 秒；观察 6 秒 = 投递刷新周期 5 秒 + 1 秒余量。
    /// 执行过程：发普通留言 → 发送未知成员 → 等待后台刷新 → 修改草稿。
    /// 预期结果：错误在刷新期间不消失，草稿和唯一消息保留；修改草稿后清除原发送错误，不产生任务。
    @MainActor func testSendErrorSurvivesDeliveryRefreshUntilDraftChanges() async {
        await runSendErrorFlow(largeText: false)
    }

    /// 最大辅助字号下发送错误仍可读，输入和发送按钮保持可操作。
    /// 数据构造：独立空间、1 条待发消息和 1 条无效提及；系统字号 Accessibility XXXL。
    /// 执行过程：复用错误保留流程，跨过 5 秒刷新，再修改草稿并截图。
    /// 预期结果：错误完整可读且不会被刷新清空；输入可继续编辑，未发送内容不生成任务。
    @MainActor func testSendErrorWithAccessibilityText() async {
        await runSendErrorFlow(largeText: true)
    }

    @MainActor private func runSendErrorFlow(largeText: Bool) async {
        let app = XCUIApplication()
        if largeText { app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = UUID().uuidString
        app.launch()
        app.buttons["workspace-tab-projects"].tap()
        app.buttons["create-workspace"].tap()
        app.textFields["workspace-name"].tap(); app.textFields["workspace-name"].typeText("错误反馈验收")
        app.buttons["workspace-create-save"].tap()
        let workspace = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-card-")).firstMatch
        XCTAssertTrue(workspace.waitForExistence(timeout: 8), "隔离空间应已保存")
        workspace.tap(); app.buttons["create-work-thread"].tap()
        app.textFields["thread-title"].tap(); app.textFields["thread-title"].typeText("检查发送反馈")
        app.buttons["thread-create-save"].tap()
        let input = app.textFields["thread-message-input"]
        XCTAssertTrue(input.waitForExistence(timeout: 8), "线程输入须可用")
        input.tap(); input.typeText("先讨论导航")
        app.buttons["thread-send"].tap()
        let messages = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH %@ AND identifier != %@", "thread-message-", "thread-message-input"))
        XCTAssertTrue(messages.firstMatch.waitForExistence(timeout: 8), "真实待发消息应触发后台投递刷新")
        input.tap(); input.typeText("@不存在 请检查")
        app.buttons["thread-send"].tap()
        let failure = app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "工作空间中没有 不存在")).firstMatch
        XCTAssertTrue(failure.waitForExistence(timeout: 8), "无效提及须明确说明原因")
        guard failure.exists else { return }
        let vanished = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: failure)
        vanished.isInverted = true
        await fulfillment(of: [vanished], timeout: 6)
        XCTAssertEqual(input.value as? String, "@不存在 请检查", "后台刷新不得清空未发送正文")
        XCTAssertTrue(failure.isHittable, "错误应显示在当前可读的输入区域")
        attach(app, "P05-send-error-after-refresh")
        input.tap(); input.typeText("导航")
        XCTAssertTrue(failure.waitForNonExistence(timeout: 8), "修改草稿后应清除过期发送错误")
        attach(app, "P05-composer-after-error-edit")
        XCTAssertEqual(messages.count, 1, "无效指令不能成为已发送消息")
        XCTAssertFalse(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-task-")).firstMatch.exists, "未知成员不能生成任务")
    }

    /// 想法引用进入 Workspace 线程，留言与目标长期保留，原想法仍可独立编辑。
    /// 数据构造：隔离想法库和 Workspace 库各 1 个；1 条想法、1 个 Workspace、2 个线程。
    ///   8 秒 = 页面转场/键盘约 3 秒 + 本机存储调度余量 5 秒；无网络回执，不构造 Agent 回复。
    /// 执行过程：记录想法 → 创建 Workspace → 从想法建线程 → 留言/设置目标 → 返回核对原件 → 重启查看。
    /// 预期结果：来源与消息保留，消息标待发送；缺项目须准确提示，不能声称仅恢复网络即可投递；
    ///   原想法不被移动；另一线程没有原消息和目标。
    @MainActor func testIdeaBecomesReferencedThreadWithoutMovingOriginal() {
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_IDEA_TEST_NAMESPACE"] = UUID().uuidString
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = UUID().uuidString
        app.launch()
        XCTAssertTrue(app.buttons["record-idea"].waitForExistence(timeout: 8), "冷启动应能直接记录想法")
        app.buttons["record-idea"].tap()
        app.buttons["record-text-idea"].firstMatch.tap()
        let ideaEditor = app.textViews["idea-body-editor"]
        XCTAssertTrue(ideaEditor.waitForExistence(timeout: 8), "想法编辑器必须可输入")
        ideaEditor.tap(); ideaEditor.typeText("首屏信息太多")
        app.buttons["save-idea"].tap()
        app.buttons["workspace-tab-projects"].tap()
        app.buttons["create-workspace"].tap()
        app.textFields["workspace-name"].tap(); app.textFields["workspace-name"].typeText("移动端体验")
        app.buttons["workspace-create-save"].tap()
        app.buttons["workspace-more-modules"].tap(); app.buttons["想法库"].tap()
        app.buttons["idea-card-menu"].tap(); app.buttons["在工作空间推进"].tap()
        let choice = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "thread-workspace-option-")).firstMatch
        XCTAssertTrue(choice.waitForExistence(timeout: 8), "必须由用户明确选择已有 Workspace")
        choice.tap()
        app.buttons["thread-create-save"].tap()
        XCTAssertTrue(app.buttons["thread-source-idea"].waitForExistence(timeout: 8), "建线程后必须保留来源入口")
        let mention = app.buttons["thread-mention"]
        XCTAssertTrue(mention.waitForExistence(timeout: 8), "输入区必须提供明确的 @Agent 选择入口")
        guard mention.exists else { return }
        mention.tap()
        XCTAssertTrue(app.staticTexts["还没有参与的 Agent"].waitForExistence(timeout: 8), "空工作空间不能显示虚构成员")
        XCTAssertTrue(app.buttons["thread-mention-settings"].exists, "没有成员时应提供工作空间设置入口")
        app.buttons["thread-mention-cancel"].tap()
        let input = app.textFields["thread-message-input"]
        input.tap(); input.typeText("先梳理信息层级")
        app.buttons["thread-send"].tap()
        let message = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH %@ AND label CONTAINS %@", "thread-message-", "先梳理信息层级")).firstMatch
        XCTAssertTrue(message.waitForExistence(timeout: 8), "落盘后才显示留言")
        XCTAssertTrue(message.label.contains("待发送"), "尚无 Mac 回执必须显示待发送")
        XCTAssertFalse(message.label.contains("已送达 Mac"), "本机落盘不得提前展示 Mac 持久接收")
        XCTAssertFalse(app.staticTexts["已接收"].exists, "本地写入不能冒充 Agent 收到任务")
        XCTAssertTrue(app.staticTexts["消息已保存在这部 iPhone，满足发送条件后会继续发送。"].exists,
                      "待发说明须同时适用于未配置项目、未授权和离线状态")
        XCTAssertFalse(app.staticTexts["待发送的消息已保存在这部 iPhone，连接恢复后会继续发送。"].exists,
                       "没有关联项目时，恢复网络不能解决投递条件，不能误导用户等待")
        app.buttons["thread-work-brief"].tap()
        app.buttons["brief-edit-goal"].tap()
        let factEditor = app.textViews["brief-fact-editor"]
        XCTAssertTrue(factEditor.waitForExistence(timeout: 8), "目标编辑器必须可输入")
        factEditor.tap(); factEditor.typeText("让首屏只突出下一步")
        app.buttons["brief-fact-save"].tap()
        XCTAssertTrue(app.staticTexts["让首屏只突出下一步"].waitForExistence(timeout: 8), "目标须落盘后展示")
        attach(app, "W05-persisted-thread-goal")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        attach(app, "P05-local-thread-pending")
        app.buttons["thread-source-idea"].tap()
        XCTAssertTrue(app.staticTexts["首屏信息太多"].waitForExistence(timeout: 8), "来源卡必须能读取引用快照")
        attach(app, "P05-source-reading-surface")
        app.buttons["thread-open-original-idea"].tap()
        XCTAssertEqual(ideaEditor.value as? String, "首屏信息太多", "原想法须独立存在，且仍可编辑")
        app.buttons["取消"].tap()
        app.buttons["完成"].tap()
        input.tap(); input.typeText("接下来调整导航")
        XCTAssertTrue(app.staticTexts["草稿已保存"].waitForExistence(timeout: 8), "退出前应有实际落盘反馈")
        app.terminate(); app.launch()
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@ AND label CONTAINS %@", "idea-card-", "首屏信息太多")).firstMatch.waitForExistence(timeout: 8), "重启后原想法仍在库内")
        app.buttons["workspace-tab-projects"].tap()
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-card-")).firstMatch.tap()
        let thread = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "work-thread-row-")).firstMatch
        XCTAssertTrue(thread.waitForExistence(timeout: 8), "Workspace 须提供真实线程列表")
        thread.tap()
        XCTAssertTrue(message.waitForExistence(timeout: 8), "重启后留言不能丢失")
        XCTAssertEqual(input.value as? String, "接下来调整导航", "未发送草稿重启后须原样回到输入框")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.buttons["create-work-thread"].tap()
        app.textFields["thread-title"].tap(); app.textFields["thread-title"].typeText("检查导航")
        app.buttons["thread-create-save"].tap()
        XCTAssertTrue(app.buttons["thread-work-brief"].waitForExistence(timeout: 8), "新线程应直接打开")
        XCTAssertFalse(message.exists, "第二线程不能继承第一线程消息")
        XCTAssertNotEqual(input.value as? String, "接下来调整导航", "第二线程不能继承第一线程草稿")
        app.buttons["thread-work-brief"].tap()
        XCTAssertFalse(app.staticTexts["让首屏只突出下一步"].exists, "线程目标不能跨线程继承")
    }

    @MainActor private func attach(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }
}
