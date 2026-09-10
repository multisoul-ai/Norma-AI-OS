import XCTest

final class ChatInteractionFlowTests: XCTestCase {
    /// Agent尚未将回复写入历史时，聊天内直接显示原卡输出并接收改变的行。
    /// 数据构造：24行隔离终端、1行持续变化正文，历史先空缺该回复，结束后才写入最终正文。
    /// 执行过程：打开运行卡 → 看见第一句 → 原屏幕更新第二句 → 正式历史写入并结束。
    /// 预期结果：无需切换终端或刷新；旧行不重复，完成后最终消息替代实时预览。
    @MainActor func testChatShowsTerminalDeltasBeforeHistoryIsWritten() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui",
              let path = ProcessInfo.processInfo.environment["NORMA_TEST_QR"] else { throw XCTSkip("仅使用隔离聊天fixture") }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        for name in ["ui-chat-growth", "ui-chat-more"] {
            try? FileManager.default.removeItem(at: dir.appendingPathComponent(name))
        }
        let flags = ["ui-chat-interactions", "ui-chat-live", "ui-chat-live-delta", "ui-chat-final"].map { dir.appendingPathComponent($0) }
        try Data().write(to: flags[0]); try Data().write(to: flags[1])
        defer { for file in flags { try? FileManager.default.removeItem(at: file) } }
        let app = XCUIApplication(); app.launch(); app.buttons["workspace-tab-projects"].tap()
        let card = app.buttons["agent-card-7"]
        // 8s=2s既有订阅心跳+6s原生导航及TLS调度余量，非物理网络性能断言。
        XCTAssertTrue(card.waitForExistence(timeout: 8), "必须打开原卡")
        card.tap()
        let output = app.staticTexts["chat-live-output"]
        XCTAssertTrue(output.waitForExistence(timeout: 8), "历史未落盘时仍应有实时输出")
        guard output.exists else { return }
        XCTAssertTrue(output.label.contains("正在写第一句"), "必须显示来自原屏幕的第一句")
        XCTAssertFalse(app.staticTexts["最终整理好的回复"].exists, "不能提前伪造正式消息")
        try Data().write(to: flags[2])
        let changed = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in output.label.contains("正在写第二句") }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [changed], timeout: 8), .completed, "生成中持续接收改变行")
        XCTAssertFalse(output.label.contains("正在写第一句"), "屏幕重绘不能重复追加旧行")
        try Data().write(to: flags[3])
        XCTAssertTrue(app.staticTexts["最终整理好的回复"].waitForExistence(timeout: 8), "结束后必须展示正式历史")
        XCTAssertTrue(output.waitForNonExistence(timeout: 8), "完成后不能残留生成中预览")
    }
    /// 正式聊天初次打开停在最新记录，发送成功清空输入，同一回复增长时持续跟随底部。
    /// 数据构造：隔离TLS/SQLite与现有Claude卡7；24条各约4行历史超过一屏；后续同ID回复增加32行。
    /// 执行过程：打开原卡 → 核对初始末条 → 发送一句原文 → 核对回执清空 → 服务增量增长同一回复。
    /// 预期结果：无需手动滑动就能看到最新内容；只收到一次准确消息，回复增长不重复且输入不残留。
    @MainActor func testChatFollowsLiveReplyAndClearsSuccessfulMessage() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("仅使用显式隔离聊天fixture")
        }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        let enabled = dir.appendingPathComponent("ui-chat-interactions")
        let growth = dir.appendingPathComponent("ui-chat-growth")
        let more = dir.appendingPathComponent("ui-chat-more")
        try Data().write(to: enabled)
        try Data().write(to: dir.appendingPathComponent("ui-card-management"))
        defer {
            for file in [enabled, growth, more] { try? FileManager.default.removeItem(at: file) }
        }
        let app = XCUIApplication(); app.launch(); app.buttons["workspace-tab-projects"].tap()
        // 8s = 2s订阅心跳 + 6s真实TLS、授权更新与导航余量，不作为网络性能阈值。
        let card = app.buttons["agent-card-7"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "必须进入有准确run/session的原Claude卡")
        card.tap()
        let initial = app.staticTexts["初始历史末条"]
        XCTAssertTrue(initial.waitForExistence(timeout: 8), "初次读取应直接呈现历史末条")
        XCTAssertTrue(initial.isHittable, "历史末条应自动滚到可见区域")
        let input = app.descendants(matching: .any)["chat-message-input"].firstMatch
        let message = "请继续检查连接"
        input.tap(); input.typeText(message)
        let send = app.buttons["chat-send-message"]
        XCTAssertTrue(send.isEnabled, "正式授权后原卡可以发送")
        send.tap()
        let cleared = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            input.value as? String == ""
        }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [cleared], timeout: 8), .completed, "准确成功回执应清空已发送输入")
        let first = app.staticTexts["实时回复第一段"]
        XCTAssertTrue(first.waitForExistence(timeout: 8), "生成未结束前应收到第一段")
        XCTAssertTrue(first.isHittable, "发送后首段应自动进入可见区域")
        try Data().write(to: growth)
        let latest = app.staticTexts.matching(NSPredicate(format: "label ENDSWITH %@", "实时回复最新部分")).firstMatch
        XCTAssertTrue(latest.waitForExistence(timeout: 8), "同一消息的正文增长必须持续渲染")
        let composerTop = input.frame.minY
        // 20pt = 列表16pt底部padding + 4pt布局取整余量；末段不能藏在输入区下方。
        XCTAssertLessThanOrEqual(latest.frame.maxY, composerTop + 20, "回复增长必须跟随底部，不能仍停在回复开头")
        XCTAssertGreaterThan(latest.frame.maxY, app.segmentedControls["chat-content-tabs"].frame.maxY, "末段应在当前可视区域")
        XCTAssertFalse(first.exists, "增长应替换原ID内容，不能留下重复第一段消息")
        if app.buttons["收起键盘"].exists { app.buttons["收起键盘"].tap() }
        let list = app.scrollViews["chat-message-list"]
        list.swipeDown()
        let jump = app.buttons["chat-latest-message"]
        XCTAssertTrue(jump.waitForExistence(timeout: 3), "向上阅读后应提供回到最新入口，3s覆盖惯性滚动")
        try Data().write(to: more)
        let further = app.staticTexts.matching(NSPredicate(format: "label ENDSWITH %@", "继续生成的末段")).firstMatch
        XCTAssertTrue(further.waitForExistence(timeout: 8), "暂停跟随期间仍须接收增量")
        XCTAssertTrue(jump.exists, "更新不能强制退出历史阅读")
        XCTAssertGreaterThan(further.frame.maxY, input.frame.minY, "阅读旧内容时不能被新输出拉到底部")
        jump.tap()
        XCTAssertLessThanOrEqual(further.frame.maxY, input.frame.minY + 20, "主动点击后应回到实时末尾")
        XCTAssertFalse(jump.exists, "回到底部后应恢复自动跟随")
        let records = try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("ui-chat-sent.json"))) as? [[String: String]]
        XCTAssertEqual(records?.count, 1, "一次发送只能写入一次命令")
        XCTAssertEqual(records?.first?["text"], message, "原文必须完整且未重复")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "chat-live-following"; shot.lifetime = .keepAlways; add(shot)
    }
}
