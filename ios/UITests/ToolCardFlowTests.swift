import XCTest

final class ToolCardFlowTests: XCTestCase {
    /// 最大辅助字号下工具名称、状态和摘要可阅读，展开入口保持可触达。
    /// 数据构造：复用同一只读工具fixture；AX5为系统最大辅助字号，8s=2s心跳+6s导航余量。
    /// 执行过程：大字号启动正式App→原聊天首屏审计→精确滚动到工具→核对完整可见边界及可访问名称。
    /// 预期结果：工具内容不裁切、目标至少44pt；不扩大控制权限或生成命令。
    @MainActor func testAccessibleToolCards() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else { throw XCTSkip("仅隔离工具fixture") }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-decision-deck").path) else { throw XCTSkip("未启用原会话fixture") }
        let enabled = dir.appendingPathComponent("ui-tool-cards")
        try Data().write(to: enabled)
        defer { try? FileManager.default.removeItem(at: enabled) }
        let app = XCUIApplication()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        app.launch(); app.buttons["workspace-tab-command"].tap()
        app.buttons["command-mode-decisions"].tap()
        let detail = app.buttons["approval-task-details"]
        XCTAssertTrue(detail.waitForExistence(timeout: 8), "详情入口必须可达")
        for _ in 0..<3 where !detail.isHittable { app.swipeUp() }
        detail.tap()
        XCTAssertTrue(app.staticTexts["chat-full-title"].waitForExistence(timeout: 8), "原任务名必须保留可访问标签")
        XCTAssertEqual(app.staticTexts["chat-full-title"].label, "检查登录测试", "紧凑导航不能丢失任务身份")
        // 在首屏做全局审计；滚动后的边界会有正常的部分可见文本，工具本身另用下面的完整边界断言核验。
        try app.performAccessibilityAudit(for: [.textClipped, .hitRegion, .sufficientElementDescription])
        let command = app.buttons["tool-card-tool-command-0"]
        let scroll = app.scrollViews["chat-message-list"]
        // 初次聊天停在最新消息；AX5时较早的工具可能已被LazyVStack卸载，先向上回看再定位。
        // 最多4次半屏回看覆盖本fixture的两张后续工具卡，不改变完整边界验收标准。
        for _ in 0..<4 where !command.exists { scroll.swipeDown(velocity: .slow) }
        XCTAssertTrue(command.exists, "回看历史必须能找到同一工具卡片")
        guard command.exists else { return }
        let top = app.segmentedControls["chat-content-tabs"].frame.maxY
        let bottom = app.descendants(matching: .any)["chat-message-input"].firstMatch.frame.minY
        // 最多12次、每次120pt：覆盖前置长回复与初次实时状态引起的布局变化。
        for _ in 0..<12 {
            if !command.exists { scroll.swipeDown(velocity: .slow); continue }
            let frame = command.frame
            if frame.minY >= top && frame.maxY <= bottom { break }
            // 留36pt目标余量，让临界位置的位移也超过原生拖动识别阈值，避免轻点误展开卡片。
            let movement = max(-120, min(120, top + 36 - frame.minY))
            let start = scroll.coordinate(withNormalizedOffset: CGVector(dx: 0.96, dy: 0.5))
            start.press(forDuration: 0.05, thenDragTo: start.withOffset(CGVector(dx: 0, dy: movement)), withVelocity: .slow, thenHoldForDuration: 0.2)
        }
        XCTAssertTrue(command.isHittable, "展开目标应可触达")
        XCTAssertLessThanOrEqual(command.frame.maxY, bottom, "完整工具卡片不能被固定输入区遮住")
        XCTAssertGreaterThanOrEqual(command.frame.minY, app.segmentedControls["chat-content-tabs"].frame.maxY, "卡片顶部不能藏在内容导航下面")
        XCTAssertGreaterThanOrEqual(command.frame.height, 44, "大字号仍保留触控下限")
        XCTAssertTrue(command.label.contains("swift test"), "摘要仍有可访问名称")
        let visible = XCTAttachment(screenshot: app.screenshot()); visible.name = "visible-tool-card-AX5"; visible.lifetime = .keepAlways; add(visible)
        XCTAssertFalse(app.buttons["chat-send-message"].isEnabled, "辅助字号不改变权限")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-decisions.json").path), "阅读不产生控制操作")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "chat-tool-cards-AX5"; shot.lifetime = .keepAlways; add(shot)
    }

    /// 正式聊天收到Codex和Claude工具块后显示紧凑卡片，展开不发送命令，实时状态不会重置展开。
    /// 数据构造：现有只读TLS审批fixture追加命令和Read调用/结果；700行输出只在展开后挂载。
    /// 执行过程：打开原聊天→展开命令→核对全文→服务器改失败→收起→查看Read和结果。
    /// 预期结果：工具名/摘要/状态可见，默认不铺满JSON；全文可选择只读；失败不是成功且不产生写命令。
    @MainActor func testToolCardsExpandAndReceiveActualStatus() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("仅用于隔离工具渲染fixture")
        }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-decision-deck").path) else {
            throw XCTSkip("未启用原会话fixture")
        }
        let enabled = dir.appendingPathComponent("ui-tool-cards"), failed = dir.appendingPathComponent("ui-tool-failed")
        try Data().write(to: enabled)
        defer { try? FileManager.default.removeItem(at: enabled); try? FileManager.default.removeItem(at: failed) }
        let app = XCUIApplication(); app.launch()
        app.buttons["workspace-tab-command"].tap()
        app.buttons["command-mode-decisions"].tap()
        // 8s=2s服务心跳+6s真实WSS和原生导航余量，不作为网络延迟指标。
        let detail = app.buttons["approval-task-details"]
        XCTAssertTrue(detail.waitForExistence(timeout: 8), "从现有决策打开原聊天")
        detail.tap()
        let command = app.buttons["tool-card-tool-command-0"]
        XCTAssertTrue(command.waitForExistence(timeout: 8), "原始commandExecution必须渲染为命令卡片")
        guard command.exists else { return }
        XCTAssertTrue(command.label.contains("swift test"), "卡片直接显示命令摘要")
        XCTAssertTrue(command.label.contains("执行中"), "初始状态来自工具记录")
        XCTAssertFalse(app.textViews["tool-section-tool-command-0-2"].exists, "700行输出默认不能占满对话")
        XCTAssertGreaterThanOrEqual(command.frame.height, 44, "展开入口至少44pt")
        command.tap()
        let output = app.textViews["tool-section-tool-command-0-2"]
        if !output.isHittable { app.swipeUp() }
        XCTAssertTrue(output.waitForExistence(timeout: 3), "展开后挂载原生正文")
        let expected = "工具输出开始\n" + String(repeating: "完整输出\n", count: 700) + "工具输出结束"
        XCTAssertEqual(output.value as? String, expected, "完整输出不能被首屏摘要裁掉")
        try Data().write(to: failed)
        let failure = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label CONTAINS %@", "失败"), object: command)
        XCTAssertEqual(XCTWaiter.wait(for: [failure], timeout: 8), .completed, "同一消息增量应更新卡片状态")
        XCTAssertTrue(output.exists, "状态更新不能让已展开内容折叠")
        for _ in 0..<3 where !command.isHittable { app.swipeDown() }
        command.tap()
        XCTAssertFalse(output.exists, "收起后应释放长正文渲染")
        let read = app.buttons["tool-card-tool-read-0"]
        if !read.isHittable { app.swipeUp() }
        XCTAssertTrue(read.label.contains("Read"), "Claude工具应显示原名称")
        XCTAssertTrue(read.label.contains("README.md"), "文件工具应显示路径摘要")
        XCTAssertTrue(read.label.contains("已请求"), "没有工具终态的请求不能冒充完成")
        let result = app.buttons["tool-card-tool-result-0"]
        if !result.isHittable { app.swipeUp() }
        XCTAssertTrue(result.label.contains("已返回"), "工具结果独立显示真实收到状态")
        XCTAssertFalse(app.buttons["chat-send-message"].isEnabled, "工具查看不能提升只读权限")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-decisions.json").path), "展开与收起不发送审批或命令")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "chat-tool-cards"; shot.lifetime = .keepAlways; add(shot)
    }
}
