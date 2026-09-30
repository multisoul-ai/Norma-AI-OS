import XCTest

final class ChatVoiceInputFlowTests: XCTestCase {
    /// 点击语音切换会走真实系统授权回调；拒权及再次切换均不能终止 App。
    /// 数据构造：隔离卡片 7，不设置 NORMA_CHAT_VOICE_FIXTURE，不采集音频；保留发送记录原值。
    ///   8s = 订阅 2s + 导航/系统授权回调余量 6s。
    /// 执行过程：打开聊天 → 切语音 → 拒绝系统语音授权 → 切回键盘 → 再切语音。
    /// 预期结果：App 保持前台、真实拒权提示存在、原输入框可恢复，发送记录没有变化。
    @MainActor func testSystemSpeechPermissionCallbackKeepsComposerAlive() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui",
              let path = ProcessInfo.processInfo.environment["NORMA_TEST_QR"] else { throw XCTSkip("仅隔离聊天 fixture") }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        let flags = ["ui-decision-deck", "ui-chat-interactions", "ui-card-management", "ui-chat-cold-session"].map { directory.appendingPathComponent($0) }
        for flag in flags { try Data().write(to: flag) }
        defer { for flag in flags { try? FileManager.default.removeItem(at: flag) } }
        let sent = directory.appendingPathComponent("ui-chat-sent.json")
        let originalSent = try? Data(contentsOf: sent)
        let app = XCUIApplication()
        app.launch()
        app.buttons["workspace-tab-projects"].tap()
        let card = app.buttons["agent-card-7"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "必须进入隔离卡片执行真实系统授权")
        card.tap()
        let toggle = app.buttons["chat-input-mode"]
        XCTAssertTrue(toggle.waitForExistence(timeout: 8), "必须通过实际模式按钮请求授权")
        toggle.tap()
        let system = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let deny = system.alerts.buttons.matching(NSPredicate(format: "label IN %@", ["Don't Allow", "不允许"])).firstMatch
        if deny.waitForExistence(timeout: 8) { deny.tap() }
        let error = app.staticTexts["chat-voice-error"]
        XCTAssertTrue(error.waitForExistence(timeout: 8), "真实系统拒权回调必须回到界面，不能崩溃")
        XCTAssertEqual(app.state, .runningForeground, "授权回调不能因线程断言终止 App")
        XCTAssertTrue(error.label.contains("系统设置"), "拒权后必须给出恢复路径")
        toggle.tap()
        XCTAssertTrue(app.descendants(matching: .any)["chat-message-input"].firstMatch.exists, "拒权后必须能恢复文字输入")
        XCTAssertFalse(app.buttons["chat-voice-hold"].exists, "切回键盘后不能残留语音按钮")
        toggle.tap()
        XCTAssertTrue(app.buttons["chat-voice-hold"].waitForExistence(timeout: 8), "已拒权时重复切换也不能崩溃")
        XCTAssertEqual(try? Data(contentsOf: sent), originalSent, "系统授权流程不能发送任务")
    }

    /// 上滑取消和权限拒绝都不修改文字草稿，也不会发送或让录音按钮永久锁定。
    /// 数据构造：隔离卡片 7；原文 keep；成功转写替身“不要加入”、拒权替身 denied。
    ///   上滑 100pt > 取消阈值 60pt；按住 1s 覆盖准备回调；8s 为订阅 2s + UI 余量 6s。
    /// 执行过程：输入 keep → 按住并上滑 → 切回检查 → 拒权重新启动 → 按住 → 检查错误及草稿。
    /// 预期结果：被取消的语音不追加；拒权后文字仍可编辑，发送记录不存在。
    @MainActor func testCancellationAndPermissionFailurePreserveText() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui",
              let path = ProcessInfo.processInfo.environment["NORMA_TEST_QR"] else { throw XCTSkip("仅隔离聊天 fixture") }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        let flags = ["ui-decision-deck", "ui-chat-interactions", "ui-card-management", "ui-chat-cold-session"].map { directory.appendingPathComponent($0) }
        for flag in flags { try Data().write(to: flag) }
        defer { for flag in flags { try? FileManager.default.removeItem(at: flag) } }
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_CHAT_VOICE_FIXTURE"] = "不要加入"
        app.launch()
        app.buttons["workspace-tab-projects"].tap()
        let card = app.buttons["agent-card-7"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "必须进入隔离卡片")
        card.tap()
        let input = app.descendants(matching: .any)["chat-message-input"].firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 8), "文字输入应可用")
        input.tap(); input.typeText("keep")
        let toggle = app.buttons["chat-input-mode"]
        toggle.tap()
        let hold = app.buttons["chat-voice-hold"]
        XCTAssertTrue(hold.waitForExistence(timeout: 8), "应切换为按住说话")
        let start = hold.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        start.press(forDuration: 1, thenDragTo: start.withOffset(CGVector(dx: 0, dy: -100)))
        XCTAssertTrue(hold.exists, "取消后应保留语音模式供重试")
        toggle.tap()
        XCTAssertEqual(input.value as? String, "keep", "取消的转写不能加入原草稿")
        app.terminate()
        app.launchEnvironment["NORMA_CHAT_VOICE_FIXTURE"] = "denied"
        app.launch()
        app.buttons["workspace-tab-projects"].tap()
        XCTAssertTrue(card.waitForExistence(timeout: 8), "重启后原卡应可打开")
        card.tap()
        XCTAssertTrue(toggle.waitForExistence(timeout: 8), "拒权不应移除模式入口")
        toggle.tap(); hold.press(forDuration: 1)
        XCTAssertTrue(app.staticTexts["chat-voice-error"].waitForExistence(timeout: 8), "拒权应解释如何恢复")
        XCTAssertTrue(app.staticTexts["chat-voice-error"].label.contains("系统设置"), "拒权提示应有明确恢复路径")
        toggle.tap()
        XCTAssertEqual(input.value as? String, "keep", "拒权和重启都不能丢弃草稿")
        XCTAssertFalse(FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-chat-sent.json").path), "取消和拒权不得发送任务")
        input.tap(); input.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 4)) // keep = 4 个字符。
        XCTAssertEqual(input.value as? String, "", "仅清理本测试的四字母草稿，供下一条独立流程")
    }

    /// 手机聊天可在键盘和按住说话之间切换，保留原草稿并通过原发送链路提交一次。
    /// 数据构造：隔离 TLS 项目/卡片 7；草稿“draft”，语音 fixture“补充测试”。
    ///   8s = 项目订阅 2s + 原生导航/草稿写入余量 6s；按住 1s 覆盖本地准备回调。
    /// 执行过程：输入草稿 → 收起并重新打开键盘 → 切语音 → 切回核验 → 再切语音并按住 → 核验转写 → 点击发送。
    /// 预期结果：两种入口互斥，草稿完整，无自动发送；准确回执后仅发送一次合并正文。
    @MainActor func testKeyboardVoiceSwitchRetainsDraftAndSendsTranscript() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui",
              let path = ProcessInfo.processInfo.environment["NORMA_TEST_QR"] else { throw XCTSkip("仅隔离聊天 fixture") }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        let flags = ["ui-decision-deck", "ui-chat-interactions", "ui-card-management", "ui-chat-cold-session"].map { directory.appendingPathComponent($0) }
        for flag in flags { try Data().write(to: flag) }
        defer { for flag in flags { try? FileManager.default.removeItem(at: flag) } }
        let sent = directory.appendingPathComponent("ui-chat-sent.json")
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_CHAT_VOICE_FIXTURE"] = "补充测试"
        app.launch()
        app.buttons["workspace-tab-projects"].tap()
        let card = app.buttons["agent-card-7"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "必须进入隔离 Agent 卡片")
        card.tap()
        let input = app.descendants(matching: .any)["chat-message-input"].firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 8), "默认应显示文字输入框")
        input.tap(); input.typeText("draft")
        app.buttons["chat-dismiss-keyboard"].tap()
        // 3s 覆盖系统键盘退场；只改变焦点，不修改五字母草稿。
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "显式收起入口必须保留并能关闭键盘")
        XCTAssertEqual(input.value as? String, "draft", "收起键盘不能发送或清空草稿")
        input.tap()
        let toggle = app.buttons["chat-input-mode"]
        XCTAssertTrue(toggle.exists, "输入框旁必须提供可发现的语音切换按钮")
        toggle.tap()
        let hold = app.buttons["chat-voice-hold"]
        XCTAssertTrue(hold.waitForExistence(timeout: 8), "语音模式中央必须显示按住说话按钮")
        XCTAssertFalse(input.exists, "语音模式应替换文字框")
        XCTAssertEqual(toggle.label, "切换到键盘输入", "切换图标的标签应指向键盘")
        let voiceShot = XCTAttachment(screenshot: app.screenshot())
        voiceShot.name = "chat-voice-ready"; voiceShot.lifetime = .keepAlways; add(voiceShot)
        toggle.tap()
        XCTAssertEqual(input.value as? String, "draft", "往返切换不能清空已输入的草稿")
        XCTAssertFalse(hold.exists, "文字模式不应残留语音按钮")
        // 3s 覆盖系统键盘进场；下一次点击必须使用键盘避让后的按钮位置。
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3), "切回文字应恢复系统键盘")
        toggle.tap()
        XCTAssertTrue(hold.waitForExistence(timeout: 8), "再次切换应显示当前语音控件")
        hold.press(forDuration: 1)
        XCTAssertTrue(input.waitForExistence(timeout: 8), "转写完成应回到可检查的文字草稿")
        XCTAssertEqual(input.value as? String, "draft\n补充测试", "转写应准确追加到原草稿")
        let draftShot = XCTAttachment(screenshot: app.screenshot())
        draftShot.name = "chat-voice-transcript"; draftShot.lifetime = .keepAlways; add(draftShot)
        XCTAssertFalse(FileManager.default.fileExists(atPath: sent.path), "松开录音不能自动执行 Agent 任务")
        app.buttons["chat-send-message"].tap()
        let cleared = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in input.value as? String == "" }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [cleared], timeout: 8), .completed, "准确发送回执后应清空草稿")
        let records = try JSONSerialization.jsonObject(with: Data(contentsOf: sent)) as? [[String: String]]
        XCTAssertEqual(records?.count, 1, "按住和松开不应重复发送")
        XCTAssertEqual(records?.first?["text"], "draft\n补充测试", "语音任务必须走原卡发送链路")
    }
}
