import XCTest

final class ChatLayoutFlowTests: XCTestCase {
    /// 空白对话提示在可用对话区域居中，键盘弹出后跟随视口重新布局。
    ///
    /// 数据构造：隔离服务的空白卡2，无run/session及消息；8s = 2s心跳 + 6s导航余量。
    /// 执行过程：打开空白卡 → 检查提示中心 → 聚焦输入 → 检查缩小后的提示中心。
    /// 预期结果：提示水平/垂直居中，不贴顶、不遮挡输入，也不出现等待历史提示。
    @MainActor func testEmptyConversationIsCentered() throws {
        _ = try fixture()
        let app = XCUIApplication(); app.launch(); app.buttons["workspace-tab-projects"].tap()
        let card = app.buttons["agent-card-2"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "需要隔离服务的空白卡2")
        card.tap()
        let title = app.staticTexts["开始对话"]
        let detail = app.staticTexts["告诉 Agent 你想完成什么。"]
        let list = app.scrollViews["chat-message-list"]
        let input = app.descendants(matching: .any)["chat-message-input"].firstMatch
        XCTAssertTrue(title.waitForExistence(timeout: 8), "空白卡必须显示开始对话")
        // 原生空状态标题位于图标下方；允许80pt容纳图标与文字组合的内部布局。
        XCTAssertEqual(title.frame.midX, list.frame.midX, accuracy: 2, "提示必须水平居中")
        XCTAssertEqual(title.frame.midY, list.frame.midY, accuracy: 80, "提示应在视口中央，不能贴在顶部元信息下")
        XCTAssertLessThan(detail.frame.maxY, input.frame.minY, "提示不能遮挡输入框")
        XCTAssertFalse(app.staticTexts["等待 Agent 写入对话…"].exists, "未绑定空卡不能冒充已有会话")
        input.tap()
        // 3s覆盖系统键盘动画及SwiftUI重新布局。
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3), "必须检查键盘出现后的可用区域")
        XCTAssertEqual(title.frame.midY, list.frame.midY, accuracy: 80, "键盘弹出后仍应居中")
        XCTAssertLessThanOrEqual(detail.frame.maxY, input.frame.minY, "键盘弹出后提示不得覆盖输入")
    }

    /// 聊天键盘未收起就返回或进入后台，不能让项目重建后的底栏上移。
    /// 数据构造：只读隔离卡7，仅聚焦空输入框；按钮底距=8pt外距+4pt轨道内距=12pt，2pt仅为取整容差。
    /// 执行过程：项目打开原聊天 → 唤起真实键盘 → 点击返回/边缘返回手势/Home恢复 → 不切Tab先测量。
    /// 显式ui-keyboard-lock模式由Computer Use确认系统锁屏，再由XCTest唤醒App，覆盖输入页被系统移出前台。
    /// 预期结果：旧键盘消失，项目与选中态恢复；无额外安全区或系统TabBar，也不发送远程命令。
    @MainActor func testBottomNavigationAfterChatKeyboardExit() throws {
        let directory = try fixture()
        continueAfterFailure = false
        let app = XCUIApplication()
        // 显式标记时由Computer Use按真实锁屏键；普通回归不依赖人工操作。
        let lockReplay = FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-keyboard-lock").path)
        for exit in lockReplay ? ["lock"] : ["background", "swipe", "back"] {
            app.launch()
            let card = app.buttons["agent-card-7"]
            // 8s = 2s快照心跳 + 6s原生导航和TLS余量；3s覆盖键盘动画。
            XCTAssertTrue(card.waitForExistence(timeout: 8), "需要隔离项目的原卡")
            card.tap()
            let input = app.descendants(matching: .any)["chat-message-input"].firstMatch
            XCTAssertTrue(input.waitForExistence(timeout: 8), "必须进入真实聊天页")
            input.tap()
            XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3), "退出前必须有真实键盘")
            XCTAssertFalse(app.buttons["chat-send-message"].isEnabled, "只读验收不能执行Agent")
            XCTAssertLessThanOrEqual(input.frame.maxY, app.keyboards.firstMatch.frame.minY, "聊天输入仍须跟随键盘避让，不能被底栏修复破坏")
            if exit == "lock" {
                let ready = directory.appendingPathComponent("keyboard-lock-ready")
                let locked = directory.appendingPathComponent("keyboard-was-locked")
                try Data().write(to: ready, options: .atomic)
                defer { try? FileManager.default.removeItem(at: ready) }
                // 180s = 120s远程工具操作预算 + 60s锁屏截图核验余量，不改变生产超时。
                let lockObserved = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
                    FileManager.default.fileExists(atPath: locked.path)
                }, object: nil)
                XCTAssertEqual(XCTWaiter.wait(for: [lockObserved], timeout: 180), .completed, "必须由Computer Use看到真实锁屏后确认")
                XCTAssertTrue(app.wait(for: .runningBackground, timeout: 5), "锁屏必须让App实际进入后台")
                app.activate()
            }
            if exit == "background" {
                XCUIDevice.shared.press(.home)
                // 5s覆盖系统切后台动画；必须已在后台，不能立刻activate取消切换。
                XCTAssertTrue(app.wait(for: .runningBackground, timeout: 5), "必须实际进入后台再恢复")
                app.activate()
            }
            if input.exists {
                if exit == "swipe" {
                    // 从屏幕左缘1pt发起系统返回手势，终点90%宽度，避开正文横向选择。
                    app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: 1, dy: app.frame.height / 2))
                        .press(forDuration: 0.05, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)))
                } else { app.navigationBars.buttons.element(boundBy: 0).tap() }
            }
            XCTAssertTrue(card.waitForExistence(timeout: 8), "返回应重新读取同一项目的原卡")
            XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "旧聊天键盘必须退出")
            let tab = app.buttons["workspace-tab-projects"]
            XCTAssertTrue(tab.isSelected, "项目恢复不能改变主导航选中态")
            // 3s覆盖键盘与导航返回动画，排除动画中间帧；仍直接读取坐标，不点击Tab触发布局。
            let settled = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
                abs(app.frame.maxY - tab.frame.maxY - 12) <= 2
            }, object: nil)
            _ = XCTWaiter.wait(for: [settled], timeout: 3)
            let gap = app.frame.maxY - tab.frame.maxY
            print("CHAT_KEYBOARD_EXIT mode=\(exit) gap=\(gap)")
            let shot = XCTAttachment(screenshot: app.screenshot())
            shot.name = "chat-keyboard-exit-\(exit)"; shot.lifetime = .keepAlways; add(shot)
            XCTAssertEqual(gap, 12, accuracy: 2, "键盘退出后不能残留安全区，且不能靠点击Tab修复")
            XCTAssertEqual(app.tabBars.count, 0, "不应恢复额外系统TabBar")
        }
    }

    /// 手机终端长中文与无空格路径完整换行，旋转和辅助字号不会恢复电脑行宽。
    /// 数据构造：隔离卡7，160列的5行终端；中文重复8次、路径重复6次，加一对有标记的软折行；9个控制键。
    /// 执行过程：打开原卡终端→核对完整文本和宽度→检查所有按键→唤起/收起键盘→旋转→辅助字号。
    /// 预期结果：行尾和硬换行完整保留，软折行按手机宽度排版；所有按键在视口内，键盘不覆盖；只读禁用。
    @MainActor func testTerminalWrapsToPhoneWidth() throws {
        let dir = try fixture()
        let flags = ["ui-chat-layout", "ui-chat-live"].map { dir.appendingPathComponent($0) }
        for flag in flags { try Data().write(to: flag) }
        defer {
            for flag in flags { try? FileManager.default.removeItem(at: flag) }
            XCUIDevice.shared.orientation = .portrait
        }
        let app = XCUIApplication()
        for accessible in [false, true] {
            XCUIDevice.shared.orientation = .portrait
            app.launchArguments = accessible ? ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] : []
            app.launch(); app.buttons["workspace-tab-projects"].tap()
            let card = app.buttons["agent-card-7"]
            // 8s = 2s订阅心跳 + 6s真实TLS和原生导航调度余量。
            XCTAssertTrue(card.waitForExistence(timeout: 8), "必须打开隔离原卡")
            card.tap(); app.segmentedControls["chat-content-tabs"].buttons["终端"].tap()
            let output = app.staticTexts["terminal-output"]
            XCTAssertTrue(output.waitForExistence(timeout: 8), "必须加载真实终端响应")
            XCTAssertTrue(output.label.contains("终端行尾"), "中文行尾不能丢失")
            XCTAssertTrue(output.label.contains("terminal-end.swift"), "无空格长路径不能裁去尾部")
            XCTAssertTrue(output.label.contains("短行应原样保留"), "不能重写终端内容")
            XCTAssertTrue(output.label.contains("软折行前半 后半应连在一起"), "应消除电脑宽度产生的软折行")
            XCTAssertFalse(output.label.contains("软折行前半 \n后半"), "不能把桌面自动折行当成新段落")
            // 每侧16pt正文边距；2pt仅容纳布局取整，不能用隐藏横向滚动通过检查。
            XCTAssertGreaterThanOrEqual(output.frame.minX, app.frame.minX + 14, "正文必须留在左边界内")
            XCTAssertLessThanOrEqual(output.frame.maxX, app.frame.maxX - 14, "正文必须自动换行到手机宽度")
            XCTAssertFalse(app.buttons["terminal-send"].isEnabled, "排版不能改变只读权限")
            for index in 0..<9 {
                let key = app.buttons["terminal-key-\(index)"]
                XCTAssertTrue(key.exists, "终端按键\(index)必须存在")
                XCTAssertGreaterThanOrEqual(key.frame.minX, app.frame.minX + 14, "按键\(index)不能滑到左边界之外")
                XCTAssertLessThanOrEqual(key.frame.maxX, app.frame.maxX - 14, "按键\(index)不能隐藏在横向滚动区")
                XCTAssertGreaterThanOrEqual(key.frame.width, 44, "按键\(index)必须保留44pt触控宽度")
                XCTAssertGreaterThanOrEqual(key.frame.height, 44, "按键\(index)必须保留44pt触控高度")
            }
            let lastKey = app.buttons["terminal-key-8"]
            XCTAssertGreaterThan(lastKey.frame.minY, app.buttons["terminal-key-0"].frame.minY, "竖屏按键必须自动换行，不依赖隐藏的横滑")
            let input = app.descendants(matching: .any)["terminal-input"].firstMatch
            input.tap()
            XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3), "终端输入必须正常唤起系统键盘")
            XCTAssertLessThanOrEqual(lastKey.frame.maxY, app.keyboards.firstMatch.frame.minY, "Ctrl-C不能被竖屏键盘挡住")
            app.buttons["terminal-dismiss-keyboard"].tap()
            XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "必须能收起键盘继续阅读终端")
            if !accessible {
                let portrait = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
                portrait.name = "terminal-wrapped-portrait"; portrait.lifetime = .keepAlways; add(portrait)
                XCUIDevice.shared.orientation = .landscapeLeft
                // 3s覆盖系统旋转动画和布局稳定，截图须在横向视口完成后获取。
                let rotated = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
                    app.frame.width > app.frame.height && output.frame.minY < app.frame.height / 2
                }, object: nil)
                XCTAssertEqual(XCTWaiter.wait(for: [rotated], timeout: 3), .completed, "应完成横屏布局")
                XCTAssertLessThanOrEqual(output.frame.width, app.frame.width - 28, "旋转后正文仍应约束在视口")
            }
            let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
            shot.name = accessible ? "terminal-wrapped-AX5" : "terminal-wrapped-landscape"
            shot.lifetime = .keepAlways; add(shot)
        }
    }

    /// Chat渲染Agent内联Markdown，用户输入原文不被格式化或修改。
    /// 数据构造：一条含反引号的用户输入，一条含粗体/代码/空行的Agent回复，以及原有工具卡。
    /// 执行过程：通过正式TLS打开原卡→读取用户和Agent正文→核对工具摘要和紧凑输入区。
    /// 预期结果：用户反引号保留，Agent格式符隐藏而段落不丢失，工具卡仍可展开。
    @MainActor func testChatFormatsOnlyAgentMarkdown() throws {
        let dir = try fixture(), flag = dir.appendingPathComponent("ui-chat-layout")
        try Data().write(to: flag)
        defer { try? FileManager.default.removeItem(at: flag) }
        let app = XCUIApplication(); app.launch(); app.buttons["workspace-tab-projects"].tap()
        let card = app.buttons["agent-card-7"]
        // 8s = 2s心跳 + 6s真实连接和导航余量。
        XCTAssertTrue(card.waitForExistence(timeout: 8), "必须打开原卡")
        card.tap()
        let user = app.staticTexts["检查 `原始输入`，请保留原文。"]
        let reply = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "已检查")).firstMatch
        XCTAssertTrue(reply.waitForExistence(timeout: 8), "必须显示Agent回复")
        XCTAssertTrue(user.exists, "用户输入应保持原文")
        XCTAssertTrue(reply.label.contains("手机布局"), "粗体正文必须保留")
        XCTAssertTrue(reply.label.contains("swift test"), "代码内容必须保留")
        XCTAssertTrue(reply.label.contains("\n\n"), "段落间空行不能被吞掉")
        XCTAssertFalse(reply.label.contains("**"), "不能直接显示粗体语法")
        XCTAssertFalse(reply.label.contains("`"), "不能直接显示代码定界符")
        XCTAssertTrue(app.buttons["tool-card-tool-command-0"].exists, "工具仍显示为原有可展开卡片")
        let input = app.descendants(matching: .any)["chat-message-input"].firstMatch
        // 单行44pt触控高度 + 边框取整余量2pt，不再额外叠加上下各12pt空白。
        XCTAssertLessThanOrEqual(input.frame.height, 46, "普通输入框应保持紧凑")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "chat-refined-layout"
        shot.lifetime = .keepAlways; add(shot)
    }

    /// 点击对话外部与工具按钮可收键盘；输入、文字选择和原生下滑仍可正常工作。
    ///
    /// 数据构造：只读TLS卡7、三张工具卡和一行本地草稿；8s为2s心跳+6s导航余量，3s为系统键盘动画预算。
    /// 执行过程：写草稿 → 点标题收起 → 重新编辑 → 点工具并展开 → 长按选择输出 → 下滑收起 → 终端外部轻点。
    /// 预期结果：草稿原文保留；收键盘不吞按钮、不阻止拷贝菜单、不发送消息；终端也能外部收起。
    @MainActor func testOutsideTapAndNativeGesturesDismissKeyboard() throws {
        let dir = try fixture(), flag = dir.appendingPathComponent("ui-tool-cards")
        try Data().write(to: flag)
        defer { try? FileManager.default.removeItem(at: flag) }
        let app = XCUIApplication(); app.launch(); app.buttons["workspace-tab-command"].tap()
        app.buttons["command-mode-decisions"].tap()
        let detail = app.buttons["approval-task-details"]
        XCTAssertTrue(detail.waitForExistence(timeout: 8), "只能从隔离决策打开原卡")
        detail.tap()
        let input = app.descendants(matching: .any)["chat-message-input"].firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 8), "需要真实聊天输入")
        let text = "Keep my unsent draft"
        input.tap(); input.typeText(text)
        app.navigationBars["检查登录测试"].staticTexts["检查登录测试"].tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "轻点对话标题必须收起键盘")
        XCTAssertEqual(input.value as? String, text, "外部轻点不能清除草稿")
        input.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3), "重新编辑必须正常唤起键盘")
        let tool = app.buttons["tool-card-tool-result-0"]
        XCTAssertTrue(tool.waitForExistence(timeout: 8), "需要准确的原会话工具卡")
        tool.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "点工具也可收起键盘")
        XCTAssertEqual(tool.value as? String, "已展开", "收键盘手势不得吞掉工具按钮点击")
        let output = app.textViews.matching(NSPredicate(format: "value == %@", "原文件内容")).firstMatch
        XCTAssertTrue(output.waitForExistence(timeout: 3), "工具展开必须显示完整输出")
        if !output.isHittable { app.scrollViews["chat-message-list"].swipeUp() }
        output.press(forDuration: 1)
        let copy = app.descendants(matching: .any).matching(NSPredicate(format: "label IN %@", ["Copy", "复制", "拷贝"])).firstMatch
        XCTAssertTrue(copy.waitForExistence(timeout: 3), "长按必须保留系统文字选择菜单")
        app.navigationBars["检查登录测试"].staticTexts["检查登录测试"].tap()
        input.tap()
        let scroll = app.scrollViews["chat-message-list"]
        let start = scroll.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.2))
        let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.9))
        start.press(forDuration: 0.05, thenDragTo: end, withVelocity: .slow, thenHoldForDuration: 0.1)
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "向下拖动应跟手收起键盘")
        XCTAssertEqual(input.value as? String, text, "滑动收起必须保留原文")
        XCTAssertFalse(app.buttons["chat-send-message"].isEnabled, "阅读与收键盘不能提升只读权限")
        input.tap(); input.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: text.count))
        app.segmentedControls["chat-content-tabs"].buttons["终端"].tap()
        let terminal = app.descendants(matching: .any)["terminal-input"].firstMatch
        terminal.tap(); terminal.typeText("keep terminal text")
        app.navigationBars["检查登录测试"].staticTexts["检查登录测试"].tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "终端同样支持外部轻点收起")
        XCTAssertEqual(terminal.value as? String, "keep terminal text", "终端未发送内容必须保留")
        XCTAssertFalse(app.buttons["terminal-send"].isEnabled, "收起不能发送终端输入")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-decisions.json").path), "浏览不产生远程控制")
    }

    private func fixture() throws -> URL {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("仅使用显式隔离TLS fixture")
        }
        return URL(fileURLWithPath: path).deletingLastPathComponent()
    }
}
