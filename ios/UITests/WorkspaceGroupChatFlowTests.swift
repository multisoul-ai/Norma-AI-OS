import XCTest

final class WorkspaceGroupChatFlowTests: XCTestCase {
    @MainActor private func openChat(_ app: XCUIApplication) throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_FIXTURE"], path.hasPrefix("/") else {
            throw XCTSkip("需要隔离本地 Mac 与已配对模拟器；不连接用户项目")
        }
        struct Fixture: Decodable { let workspaceID: String }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        app.launch()
        // 15秒 = App首启3秒 + 群聊同步轮询5秒 + 模拟器调度余量7秒。
        XCTAssertTrue(app.buttons["workspace-tab-projects"].waitForExistence(timeout: 15), "原生底栏应可进入工作空间")
        app.buttons["workspace-tab-projects"].tap()
        let workspace = app.buttons["workspace-card-\(fixture.workspaceID)"]
        XCTAssertTrue(workspace.waitForExistence(timeout: 15), "列表必须使用正式Mac目录")
        workspace.tap()
        XCTAssertTrue(app.buttons["group-mention"].waitForExistence(timeout: 8), "进入工作空间应直接显示群聊")
        XCTAssertFalse(app.buttons["create-work-thread"].exists, "不能再出现创建工作线程的前置步骤")
    }

    /// 场景：快捷新建 Sheet 仅选类型及项目，取消时保持群聊草稿和阅读位置。
    /// 数据构造：隔离Mac的1个已有项目；一份未发送草稿。8秒=Sheet转场3秒+同步余量5秒。
    /// 执行过程：输入草稿 → 打开顶部＋ → 检查无需滚动可见的自动选中项目 → 取消 → 从设置再打开。
    /// 预期结果：没有名称和任务输入；取消不丢草稿；设置保留添加已有Agent入口。
    @MainActor func testQuickCreateSheetPreservesChatDraft() throws {
        let app = XCUIApplication()
        try openChat(app)
        let input = app.descendants(matching: .any)["group-message-input"].firstMatch
        input.tap(); input.typeText("Creation keeps this draft")
        let create = app.buttons["group-create-agent"]
        XCTAssertTrue(create.waitForExistence(timeout: 8), "群聊顶部必须有新建Agent入口")
        create.tap()
        XCTAssertTrue(app.navigationBars["新建智能体"].waitForExistence(timeout: 8), "新建应打开原生Sheet")
        let submit = app.buttons["agent-create-submit"]
        XCTAssertTrue(submit.exists, "创建Sheet应有明确提交入口")
        let project = app.descendants(matching: .any)["agent-create-project"].firstMatch
        XCTAssertTrue(project.exists, "单项目必须自动选择并显示真实项目")
        XCTAssertTrue(project.isHittable, "medium Sheet 的所属项目不能被底部操作遮住")
        XCTAssertLessThanOrEqual(project.frame.maxY, submit.frame.minY, "项目内容必须完整位于固定提交操作上方")
        XCTAssertFalse(app.textFields["名称"].exists, "名字由Mac生成，无需手机填写")
        XCTAssertFalse(app.textFields["任务"].exists, "创建不能隐式派发任务")
        app.buttons["agent-create-cancel"].tap()
        XCTAssertTrue((input.value as? String)?.contains("Creation keeps this draft") == true, "取消创建不能丢失聊天草稿")
        app.buttons["group-settings"].tap()
        let createInSettings = app.buttons["workspace-create-agent"]
        if !createInSettings.isHittable { app.scrollViews["group-message-scroll"].swipeUp() }
        XCTAssertTrue(createInSettings.waitForExistence(timeout: 8), "设置Agent区必须有新建入口")
        XCTAssertTrue(app.buttons["workspace-add-agent"].exists, "已有Agent入口仍须保留")
        createInSettings.tap()
        XCTAssertTrue(app.buttons["agent-create-submit"].waitForExistence(timeout: 8), "两个入口应共用创建Sheet")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "Agent-create-native-sheet"; shot.lifetime = .keepAlways; add(shot)
    }

    /// 场景：从 App 提交真实创建，成功刷新成员，保留草稿与正在阅读的旧消息位置。
    /// 数据构造：隔离 Mac 的长群聊、1个项目、1份未发草稿；30秒=后台持久化20秒+同步10秒。
    ///   位置容差2pt=系统布局取整1pt+滚动恢复取整1pt，不允许跳转到其它消息。
    /// 执行过程：写草稿并收键盘 → 阅读旧消息 → 新建Codex → 完成 → 比较旧消息和概览 → 选新成员。
    /// 预期结果：真实成功与新名称；概览人数变化；无隐式新聊天任务；原草稿与旧消息位置保持。
    @MainActor func testAppCreatesRealMemberAndPreservesReadingPosition() throws {
        let app = XCUIApplication()
        try openChat(app)
        let input = app.descendants(matching: .any)["group-message-input"].firstMatch
        input.tap()
        input.press(forDuration: 1)
        if app.menuItems["全选"].waitForExistence(timeout: 2) { app.menuItems["全选"].tap() }
        input.typeText(" App create draft remains")
        app.buttons["group-create-agent"].tap()
        app.buttons["agent-create-cancel"].tap()
        XCTAssertFalse(app.keyboards.firstMatch.exists, "阅读历史前需关闭键盘，不能在被输入层遮住的区域滑动")
        app.scrollViews["group-message-scroll"].swipeDown()
        let oldMessage = app.buttons["group-task-8"]
        for _ in 0..<8 where !oldMessage.isHittable { app.scrollViews["group-message-scroll"].swipeDown() }
        XCTAssertTrue(oldMessage.isHittable, "验收必须先实际阅读旧消息")
        let oldY = oldMessage.frame.minY
        let oldCount = app.buttons["group-overview"].label
        app.buttons["group-create-agent"].tap()
        app.buttons["agent-create-submit"].tap()
        let success = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "已创建 ")).firstMatch
        XCTAssertTrue(success.waitForExistence(timeout: 30), "必须等到真实创建成功，不能用已接收代替")
        let name = String(success.label.dropFirst("已创建 ".count))
        XCTAssertFalse(name.isEmpty, "Mac必须返回实际自动名称")
        app.buttons["agent-create-cancel"].tap()
        XCTAssertTrue((input.value as? String)?.contains("App create draft remains") == true, "创建后不能清除未发送草稿")
        XCTAssertNotEqual(app.buttons["group-overview"].label, oldCount, "创建成功后概览应更新人数")
        XCTAssertTrue(oldMessage.isHittable, "成功后必须保留正在阅读的旧消息")
        XCTAssertEqual(oldMessage.frame.minY, oldY, accuracy: 2, "创建不能跳转到最早或最近消息")
        let reading = XCTAttachment(screenshot: app.screenshot()); reading.name = "Agent-created-reading-preserved"; reading.lifetime = .keepAlways; add(reading)
        app.buttons["group-mention"].tap()
        let created = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@ AND label BEGINSWITH %@", "group-member-", name)).firstMatch
        for _ in 0..<4 where !created.isHittable { app.collectionViews.firstMatch.swipeUp() }
        XCTAssertTrue(created.isHittable, "新卡片应可立即通过@选择")
        XCTAssertTrue(created.isEnabled, "启动失败的卡片不能被显示为可用")
        created.tap()
        XCTAssertTrue((input.value as? String)?.contains("App create draft remains") == true, "选择新Agent也不能清除原草稿")
    }

    /// 场景：@卡片选择有明确选中状态，键盘上方可发送且关闭页面保留草稿。
    /// 数据构造：隔离Mac空间中1张开发卡片、1份未发送草稿；8秒=转场3秒+键盘/SQLite余量5秒。
    /// 执行过程：直入群聊 → 输入草稿 → 选卡片 → 重开选择器 → 关闭Sheet → 重启App。
    /// 预期结果：原目标标为选中；发送按钮不被键盘遮挡；草稿跨重启仍在；没有任务自动派发。
    @MainActor func testExactMemberSelectionAndKeyboardPreserveDraft() throws {
        let app = XCUIApplication()
        try openChat(app)
        let input = app.descendants(matching: .any)["group-message-input"].firstMatch
        input.tap(); input.typeText("UI draft remains unsent")
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 8), "输入消息应使用原生软件键盘")
        XCTAssertLessThanOrEqual(app.buttons["group-send"].frame.maxY, app.keyboards.firstMatch.frame.minY, "发送操作应完整位于键盘上方")
        app.buttons["group-mention"].tap()
        let member = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "group-member-")).firstMatch
        XCTAssertTrue(member.waitForExistence(timeout: 8), "选择器应列出真实Agent卡片")
        let identifier = member.identifier
        member.tap()
        app.buttons["group-mention"].tap()
        XCTAssertTrue(app.buttons[identifier].waitForExistence(timeout: 8), "再次进入选择器应仍显示原卡片")
        XCTAssertTrue(app.buttons[identifier].isSelected, "已选目标需要明确勾选状态")
        app.buttons["完成"].tap()
        XCTAssertFalse(app.buttons[identifier].exists, "关闭Sheet后不能继续拦截群聊")
        let keyboard = XCTAttachment(screenshot: app.screenshot()); keyboard.name = "P05-group-keyboard-selected"; keyboard.lifetime = .keepAlways; add(keyboard)
        app.terminate()
        try openChat(app)
        XCTAssertTrue((input.value as? String)?.contains("UI draft remains unsent") == true, "返回与重启都不能丢失未发送原文")
        XCTAssertFalse(app.staticTexts["待发送"].exists, "仅编辑草稿不能自动产生派工操作")
    }

    /// 场景：真实Codex成果的Markdown围栏不能作为正文显示，代码内容仍可阅读。
    /// 数据构造：正式任务已有proof.txt成果，包含1段路径和1个text代码块；最多6次滚动用于长历史。
    /// 执行过程：打开群聊 → 找到成果卡 → 查看正文 → 核对代码内容和关闭动作。
    /// 预期结果：用户看见真实文件内容；Markdown控制语法隐藏；Sheet可返回群聊。
    @MainActor func testRealResultDisplaysFormattedCode() throws {
        let app = XCUIApplication()
        try openChat(app)
        let result = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@ AND label CONTAINS %@", "group-task-", "proof.txt")).firstMatch
        for _ in 0..<6 where !result.isHittable { app.scrollViews["group-message-scroll"].swipeUp() }
        XCTAssertTrue(result.isHittable, "真实成果卡必须可打开，不能只存在于后台缓存")
        result.tap()
        let resultTab = app.buttons["结果"]
        XCTAssertTrue(resultTab.waitForExistence(timeout: 8), "Agent详情默认终端，应提供结果切换")
        resultTab.tap()
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "Norma mobile chat ok.")).firstMatch.waitForExistence(timeout: 8), "实际执行成果必须可阅读")
        XCTAssertFalse(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "```text")).firstMatch.exists, "成果不能暴露Markdown围栏语法")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "P05-real-result-formatted"; shot.lifetime = .keepAlways; add(shot)
        app.buttons["完成"].tap()
        XCTAssertTrue(app.buttons["group-send"].exists, "关闭成果应回到可输入的群聊")
    }

    /// 场景：重开长群聊默认显示最近真实成果，不能从最早失败记录开始。
    /// 数据构造：本地验收已存在早期失败任务和末尾真实任务成果；无需新增派工。
    /// 执行过程：打开共享空间 → 不执行滚动 → 检查最后一张任务成果卡。
    /// 预期结果：末尾成果在可视区域；不是只把控件创建在不可见的长列表中。
    @MainActor func testOpeningHistoryShowsLatestResultWithoutScrolling() throws {
        let app = XCUIApplication()
        try openChat(app)
        let lastResult = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "group-task-")).allElementsBoundByIndex.last
        XCTAssertNotNil(lastResult, "长历史必须保留真实成果")
        XCTAssertTrue(lastResult?.isHittable == true, "打开群聊应定位最新成果，无需用户反复滚动")
    }

    /// 场景：长群聊的工作空间概览固定在导航栏下，阅读旧消息后仍可打开。
    /// 数据构造：隔离 Mac 已有超出一屏的真实历史和末尾成果；不发送新消息。
    ///   8 秒 = 原生转场 3 秒 + 模拟器与同步余量 5 秒。
    ///   位置容差 1 pt = 允许系统布局浮点取整；不能容忍随手势滚动。
    /// 执行过程：打开群聊 → 检查概览与最新成果 → 下滑阅读历史 → 打开并关闭概览。
    /// 预期结果：概览完整位于导航栏下、消息区上；滚动不移走概览；Sheet 关闭后输入仍可用。
    @MainActor func testOverviewRemainsAtTopWhileReadingHistory() throws {
        let app = XCUIApplication()
        try openChat(app)
        let overview = app.buttons["group-overview"]
        XCTAssertTrue(overview.waitForExistence(timeout: 8), "群聊必须提供工作空间概览入口")
        XCTAssertTrue(overview.isHittable, "打开最新消息时，概览仍须在顶部可见并可点")
        let navigation = app.navigationBars.firstMatch
        XCTAssertGreaterThanOrEqual(overview.frame.minY, navigation.frame.maxY, "概览不能被系统导航栏遮挡")
        XCTAssertLessThanOrEqual(overview.frame.maxY, app.scrollViews["group-message-scroll"].frame.minY, "概览应位于消息滚动区上方")
        let lastResult = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "group-task-")).allElementsBoundByIndex.last
        XCTAssertTrue(lastResult?.isHittable == true, "顶部概览不能使聊天重新定位到最早记录")
        let originalY = overview.frame.minY
        app.scrollViews["group-message-scroll"].swipeDown()
        XCTAssertFalse(lastResult?.isHittable == true, "下滑必须实际移到较早历史，不能用未滚动的页面证明概览固定")
        XCTAssertTrue(overview.isHittable, "阅读历史时概览不能随消息滚出屏幕")
        XCTAssertEqual(overview.frame.minY, originalY, accuracy: 1, "概览位置应独立于消息滚动位置")
        overview.tap()
        XCTAssertTrue(app.navigationBars["工作空间概览"].waitForExistence(timeout: 8), "概览入口应打开原生详情 Sheet")
        app.buttons["完成"].tap()
        XCTAssertFalse(app.navigationBars["工作空间概览"].exists, "关闭概览后不能残留遮挡聊天的 Sheet")
        XCTAssertTrue(overview.isHittable, "返回聊天后概览应继续固定在顶部")
        XCTAssertTrue(app.buttons["group-send"].exists, "关闭概览后应保留消息输入与发送入口")
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "P05-fixed-workspace-overview"; shot.lifetime = .keepAlways; add(shot)
    }
}
