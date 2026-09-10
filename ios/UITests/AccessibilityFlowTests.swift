import XCTest

final class AccessibilityFlowTests: XCTestCase {
    /// 最大辅助字号下，指挥台计划与项目选择入口保持可读，不裁切或压缩触控区域。
    ///
    /// 数据构造：notification-ui同一只读项目及最新计划，AX5系统最大辅助字号；8s为导航/真实TLS读取余量。
    /// 执行过程：选验收电脑 → 指挥台行动计划 → 审计首屏 → 滚动到待办 → 审计操作区域。
    /// 预期结果：当前方向完整显示，待办可触达；只读安排保持禁用，无需缩小用户字体。
    @MainActor func testAccessibleCommandPlan() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui" else {
            throw XCTSkip("仅使用隔离只读计划fixture")
        }
        let app = XCUIApplication()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        app.launch(); app.buttons["manage-computers"].tap()
        let machine = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        XCTAssertTrue(machine.waitForExistence(timeout: 8), "仅操作隔离电脑")
        machine.tap(); app.buttons["workspace-tab-command"].tap()
        XCTAssertTrue(app.staticTexts["优先解决手机连接延迟"].waitForExistence(timeout: 8), "大字号须保留完整方向")
        try app.performAccessibilityAudit(for: [.textClipped, .hitRegion, .sufficientElementDescription]) { issue in
            print("COMMAND_PLAN_AX_ISSUE: \(issue.element?.debugDescription ?? "unknown element")")
            print(app.debugDescription)
            return false // Record diagnostics; never suppress an audit finding.
        }
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "command-plan-AX5"; shot.lifetime = .keepAlways; add(shot)
        let detail = app.buttons["todo-open-0"]
        scrollToWorkspaceRow(detail, in: app)
        detail.tap()
        let accept = app.buttons["todo-accept-0"]
        // 有限3次滚动覆盖方向、简报、筛选三个上方区段，避免无限滚动掩盖入口丢失。
        for _ in 0..<3 where !accept.isHittable { app.swipeUp() }
        XCTAssertTrue(accept.isHittable, "待办操作区应能滚动到达")
        XCTAssertFalse(accept.isEnabled, "字体适配不能提升只读权限")
        try app.performAccessibilityAudit(for: [.textClipped, .hitRegion, .sufficientElementDescription]) { issue in
            print("COMMAND_PLAN_TODO_AX_ISSUE: \(issue.element?.debugDescription ?? "unknown element")")
            print(app.debugDescription)
            return false
        }
    }

    /// 已授权目录和实时卡片支持搜索及清除，无结果不能冒充项目或卡片被删除。
    ///
    /// 数据构造：沿用notification-ui独立只读Mac/项目/卡；“NoMatchForNorma”不在fixture标题中。
    /// 执行过程：选择电脑 → 搜索项目无结果并清除 → 进入项目 → 搜索卡片无结果并清除 → 打开原卡。
    /// 预期结果：过滤时旧项不显示；清除后仍能到达同一真实会话，不因查找开放只读发送。
    @MainActor func testProjectAndCardSearchCanBeCleared() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui" else {
            throw XCTSkip("只在独立notification-ui fixture中运行")
        }
        let app = XCUIApplication(); app.launch()
        // 5s为现有原生导航与WSS回源预算，不是网络性能目标。
        app.buttons["manage-computers"].tap()
        let machine = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        XCTAssertTrue(machine.waitForExistence(timeout: 5), "必须限定隔离验收电脑")
        machine.tap(); app.buttons["workspace-tab-projects"].tap()
        XCTAssertTrue(app.buttons["agent-card-1"].waitForExistence(timeout: 5), "默认直接显示卡片看板")
        app.buttons["project-switcher"].tap()
        let project = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收项目")).firstMatch
        XCTAssertTrue(project.waitForExistence(timeout: 5), "切换面板来自真实授权目录")
        let projectSearch = app.searchFields["搜索项目"]
        XCTAssertTrue(projectSearch.waitForExistence(timeout: 5), "项目应提供原生搜索框")
        projectSearch.tap(); projectSearch.typeText("NoMatchForNorma")
        XCTAssertTrue(app.staticTexts["没有匹配的项目"].waitForExistence(timeout: 5), "查询无结果应与没有授权区分")
        XCTAssertFalse(project.exists, "无关项目不应残留在搜索结果中")
        app.buttons["clear-project-filters"].tap()
        XCTAssertTrue(project.waitForExistence(timeout: 5), "清除应恢复当前电脑的项目")
        project.tap()
        let card = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知回源验收卡片")).firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 5), "进入同一项目后应读取真实卡片")
        app.buttons["card-filters"].tap(); app.buttons["搜索卡片"].tap()
        let cardSearch = app.textFields["搜索卡片"]
        XCTAssertTrue(cardSearch.waitForExistence(timeout: 5), "卡片应提供原生搜索框")
        cardSearch.tap(); cardSearch.typeText("NoMatchForNorma")
        app.buttons["完成"].tap()
        XCTAssertTrue(app.staticTexts["没有匹配的卡片"].waitForExistence(timeout: 5), "卡片查询应立即显示无结果")
        XCTAssertFalse(card.exists, "无关卡片不应显示")
        app.buttons["clear-card-filters"].tap()
        XCTAssertTrue(card.waitForExistence(timeout: 5), "清除不能丢失原始卡片")
        card.tap()
        XCTAssertTrue(app.buttons["chat-send-message"].waitForExistence(timeout: 5), "应仍可进入原会话")
        XCTAssertFalse(app.buttons["chat-send-message"].isEnabled, "搜索不能扩大只读权限")
    }

    /// 最大辅助字号下，项目卡片可读、可点击，聊天输入保留可用空间且不能越过只读授权发送。
    ///
    /// 数据构造：复用notification-ui的唯一只读Mac/项目及Codex/Claude两张卡；AX5为系统最大辅助字号；
    ///   五行草稿覆盖输入框1…5行上限，5s为原生导航与WSS回源预算。
    /// 执行过程：选择隔离电脑 → 项目与脉络 → 系统无障碍审计 → 卡片 → 输入长草稿 → 清空草稿。
    /// 预期结果：卡片没有被裁切且控件可触达；输入内容完整，发送保持禁用，顶部对话保留空间。
    @MainActor func testAccessibleProjectAndChat() throws {
        try checkProjectAndChat(category: "UICTContentSizeCategoryAccessibilityXXXL", name: "AX5")
    }

    /// 普通字号仍保留紧凑卡片与五行输入，不因辅助字号修复而截断草稿或开放只读发送。
    ///
    /// 数据构造：同一个隔离只读项目，系统默认Large字号，五行原文与辅助字号测试相同。
    /// 执行过程：正式项目导航 → 卡片与脉络无障碍审计 → 原生键盘输入 → 检查正文空间和权限 → 清空。
    /// 预期结果：默认字号和AX5均能完成相同操作；不丢字、不发送、不遗留测试草稿。
    @MainActor func testStandardProjectAndChat() throws {
        try checkProjectAndChat(category: "UICTContentSizeCategoryL", name: "standard")
    }

    /// 聊天输入后可明确收起键盘，再次编辑保留原文，不能把收起键盘当作发送。
    /// 数据构造：已有只读隔离项目、同一Codex卡；短草稿只有一行，不依赖长列表滚动。
    /// 执行过程：进入原卡聊天 → 输入草稿 → 收起键盘 → 核对原文 → 再次编辑并清除测试草稿。
    /// 预期结果：键盘先存在后消失，收起入口可触达；草稿不丢失，发送权限仍为只读。
    @MainActor func testChatKeyboardCanDismissWithoutLosingDraft() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui" else {
            throw XCTSkip("仅使用隔离聊天fixture")
        }
        let app = XCUIApplication(); app.launch()
        app.buttons["workspace-tab-projects"].tap()
        // 5s = 已有真实TLS读取与原生导航预算；3s = 最多2s键盘动画/调度加1s查询余量。
        let card = app.buttons["agent-card-1"]
        XCTAssertTrue(card.waitForExistence(timeout: 5), "仅进入隔离项目的原卡")
        card.tap()
        let input = app.descendants(matching: .any)["chat-message-input"].firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 5), "聊天应有可编辑输入框")
        let text = "Keyboard dismissal keeps this draft"
        input.tap(); input.typeText(text)
        defer {
            input.tap()
            input.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: text.count))
        }
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3), "输入时必须显示真实软件键盘")
        let dismiss = app.buttons["收起键盘"]
        XCTAssertTrue(dismiss.waitForExistence(timeout: 3), "短对话也需要明确的收起键盘入口")
        guard dismiss.exists else { return }
        XCTAssertTrue(dismiss.isHittable, "收起按钮不能被键盘覆盖")
        dismiss.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "点击收起后键盘必须消失")
        XCTAssertEqual(input.value as? String, text, "收起键盘不能丢失未发送草稿")
        XCTAssertFalse(app.buttons["chat-send-message"].isEnabled, "收起操作不能扩大只读权限")
    }

    /// 终端输入沿用聊天的原生收键盘入口，离线时也能关闭并保留未发送文字。
    /// 数据构造：同一只读隔离卡；终端未接入，输入一行，3s为键盘动画和查询余量。
    /// 执行过程：打开原卡终端 → 输入 → 收起键盘 → 核对正文和禁用状态。
    /// 预期结果：键盘消失、文字保留；断开的终端不能发送。
    @MainActor func testTerminalKeyboardCanDismissWithoutSending() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui" else {
            throw XCTSkip("仅使用隔离聊天fixture")
        }
        let app = XCUIApplication(); app.launch(); app.buttons["workspace-tab-projects"].tap()
        let card = app.buttons["agent-card-1"]
        XCTAssertTrue(card.waitForExistence(timeout: 5), "5s覆盖正式读取和导航")
        card.tap(); app.segmentedControls["chat-content-tabs"].buttons["终端"].tap()
        let input = app.descendants(matching: .any)["terminal-input"].firstMatch
        input.tap(); input.typeText("keep this input")
        let dismiss = app.buttons["收起键盘"]
        XCTAssertTrue(dismiss.waitForExistence(timeout: 3), "终端也必须提供收键盘入口")
        guard dismiss.exists else { return }
        dismiss.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "收起后不应残留键盘")
        XCTAssertEqual(input.value as? String, "keep this input", "收键盘不能清空未发送内容")
        XCTAssertFalse(app.buttons["terminal-send"].isEnabled, "离线终端不能发送")
    }

    @MainActor private func checkProjectAndChat(category: String, name: String) throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui" else {
            throw XCTSkip("仅在独立notification-ui fixture和显式模拟器中运行")
        }
        let app = XCUIApplication()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", category]
        app.launch()
        app.buttons["manage-computers"].tap()
        let machine = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        XCTAssertTrue(machine.waitForExistence(timeout: 5), "不能操作用户其他已配对电脑")
        machine.tap()
        app.buttons["workspace-tab-projects"].tap()
        let card = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知回源验收卡片")).firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 5), "大字号仍需显示完整卡片身份")
        let otherCard = app.buttons["agent-card-2"]
        XCTAssertTrue(otherCard.exists, "项目应同时显示未派发任务的Claude卡片")
        if name == "standard" {
            // 两列同一行的上沿差不超过1pt布局取整；44pt是iOS最低触控区域。
            XCTAssertEqual(card.frame.minY, otherCard.frame.minY, accuracy: 1, "默认字号两张卡应在同一行")
            XCTAssertLessThan(card.frame.maxX, otherCard.frame.minX, "两列不能覆盖或挤压彼此")
            XCTAssertTrue(otherCard.isHittable, "第二张卡不能被项目脉络挤到首屏之外")
            XCTAssertGreaterThanOrEqual(card.frame.width, 44, "卡片必须保持可触控尺寸")
            XCTAssertFalse(app.buttons["project-session-archive"].isHittable, "记录入口默认折叠，卡片优先显示")
        } else {
            XCTAssertLessThanOrEqual(card.frame.maxY, otherCard.frame.minY, "辅助字号必须改成单列，不能挤压为两列")
            XCTAssertEqual(card.frame.minX, otherCard.frame.minX, accuracy: 1, "单列卡片应保持相同左边距")
        }
        try app.performAccessibilityAudit(for: [.textClipped, .hitRegion, .sufficientElementDescription])
        let cards = XCTAttachment(screenshot: app.screenshot()); cards.name = "accessibility-project-\(name)"; cards.lifetime = .keepAlways; add(cards)
        let context = app.staticTexts["project-current-direction"]
        // 两张卡各占一段，最多3次滑动到达真实项目脉络；保留完整方向与44pt入口。
        for _ in 0..<3 where !context.isHittable { app.swipeUp() }
        XCTAssertTrue(context.isHittable, "项目脉络须能在默认和最大辅助字号下触达")
        XCTAssertEqual(context.label, "优先解决手机连接延迟", "大字号不能截断真实方向")
        XCTAssertFalse(app.staticTexts["还没有设定方向"].exists, "已读计划不能显示空态")
        try app.performAccessibilityAudit(for: [.textClipped, .hitRegion, .sufficientElementDescription])
        let overview = XCTAttachment(screenshot: app.screenshot()); overview.name = "project-context-\(name)"; overview.lifetime = .keepAlways; add(overview)
        for _ in 0..<3 where !card.isHittable { app.swipeDown() }
        card.tap()
        let input = app.descendants(matching: .any)["chat-message-input"].firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 5), "大字号不能隐藏聊天输入框")
        try app.performAccessibilityAudit(for: [.textClipped, .hitRegion, .sufficientElementDescription])
        let message = "First line\nSecond line\nThird line\nFourth line\nFifth line"
        input.tap(); input.typeText(message)
        XCTAssertEqual(input.value as? String, message, "输入框必须保留全部五行原文")
        XCTAssertFalse(app.buttons["chat-send-message"].isEnabled, "只读项目即使有草稿也不能发送")
        XCTAssertTrue(app.staticTexts["无法发送，请检查连接与权限。"].exists, "必须说明禁用原因，不能为缩小布局隐藏权限提示")
        // 80pt = 44pt最小触控区域 + 36pt手势余量；正文仍能发起滚动，不能被输入框和键盘完全挤掉。
        XCTAssertGreaterThan(input.frame.minY - app.navigationBars.firstMatch.frame.maxY, 80, "大字号输入时对话不能被挤到消失")
        print("ACCESSIBILITY_CHAT_\(name)_SPACE=\(input.frame.minY - app.navigationBars.firstMatch.frame.maxY)")
        let chat = XCTAttachment(screenshot: app.screenshot()); chat.name = "accessibility-chat-\(name)-keyboard"; chat.lifetime = .keepAlways; add(chat)
        input.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: message.count))
        XCTAssertEqual(input.value as? String, "", "结束时应清空自己的验收草稿")
    }
}
