import XCTest

final class TasksFlowTests: XCTestCase {
    /// Claude 历史尚不可读时不能永久加载，恢复后用户可在原卡重新读取。
    ///
    /// 数据构造：原卡 7 / session-7，真实 WSS 返回 history_unavailable；
    ///   12s = 1+2+4s 有限重连退避 + 5s 原生导航和调度余量。
    /// 执行过程：打开 Claude → 等待读取失败 → 恢复历史源 → 点击重新读取。
    /// 预期结果：失败时 spinner 消失、刷新可用；恢复后显示原会话且不发送任何命令。
    @MainActor func testClaudeUnavailableHistoryCanRecoverWithoutEndlessLoading() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("只使用明确的隔离聊天服务")
        }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        let marker = dir.appendingPathComponent("ui-chat-history-unavailable")
        try Data().write(to: marker)
        defer { try? FileManager.default.removeItem(at: marker) }
        let app = XCUIApplication(); app.launch()
        app.buttons["manage-computers"].tap()
        let machine = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        XCTAssertTrue(machine.waitForExistence(timeout: 5), "只能操作隔离电脑")
        machine.tap()
        let card = app.buttons["agent-card-7"]
        XCTAssertTrue(card.waitForExistence(timeout: 5), "应打开具有确切会话的 Claude 原卡")
        card.tap()
        let failure = app.staticTexts["对话暂时无法读取，请刷新后重试。"]
        XCTAssertTrue(failure.waitForExistence(timeout: 12), "历史缺失必须结束加载并显示真实错误")
        XCTAssertFalse(app.descendants(matching: .any)["workspace-loading"].firstMatch.exists, "失败后不应继续转圈")
        let refresh = app.buttons["重新读取对话"]
        XCTAssertTrue(refresh.isEnabled, "失败后应允许在当前卡重新读取")
        try FileManager.default.removeItem(at: marker)
        refresh.tap()
        XCTAssertTrue(app.staticTexts["来自原会话 session-7 的完整回复"].waitForExistence(timeout: 8), "恢复后应显示同一会话的正文")
        XCTAssertFalse(failure.exists, "恢复后必须清除旧错误")
        XCTAssertFalse(app.descendants(matching: .any)["workspace-loading"].firstMatch.exists, "收到正文后不能残留加载")
        XCTAssertFalse(app.buttons["chat-send-message"].isEnabled, "只读权限不能因重试被提升")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-chat-sent.json").path), "读取和恢复都不能自动发消息")
    }

    /// 最大字号下任务表单可操作，同名node在不同项目及新旧窗口不能共享草稿。
    /// 数据构造：两个只读项目都使用Codex node1，但workspace/route/run/session均不同；AX5字体；
    ///   每段最多8次滚动覆盖两屏高表单，8s = 2s心跳 + 6s布局与网络调度余量。
    /// 执行过程：首项目写目标 → 切第二项目写另一目标 → 返回首项目 → 检查纵排按钮 → 替换窗口再还原。
    /// 预期结果：目标分别保存且不串项目/窗口；只读不能发送；AX5按钮不重叠、文字不溢出屏幕。
    @MainActor func testAccessibleNewTaskProjectAndWindowIsolation() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else { throw XCTSkip("仅隔离双项目fixture") }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-project-navigation").path) else { throw XCTSkip("必须先授权隔离第二项目") }
        let second = dir.appendingPathComponent("ui-secondary-open")
        let replaced = dir.appendingPathComponent("ui-stale")
        try Data().write(to: second)
        defer { try? FileManager.default.removeItem(at: second); try? FileManager.default.removeItem(at: replaced) }
        let app = XCUIApplication()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        app.launch(); app.buttons["workspace-tab-tasks"].tap()
        app.buttons["new-task"].tap()
        let input = app.descendants(matching: .any)["new-task-input"].firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 8), "AX5下仍应直接进入目标表单")
        for _ in 0..<8 where !input.isHittable { app.scrollViews.firstMatch.swipeUp() }
        input.tap(); input.typeText("首项目的目标")
        app.buttons["chat-dismiss-keyboard"].tap()
        let picker = app.buttons["new-task-project"]
        for _ in 0..<8 where !picker.isHittable { app.scrollViews.firstMatch.swipeDown() }
        picker.tap()
        app.buttons["第二验收项目"].tap()
        XCTAssertTrue(input.waitForExistence(timeout: 8), "第二项目已打开且有原Codex卡")
        XCTAssertFalse((input.value as? String ?? "").contains("首项目的目标"), "同node_id不能串项目草稿")
        for _ in 0..<8 where !input.isHittable { app.scrollViews.firstMatch.swipeUp() }
        input.tap(); input.typeText("第二项目的目标")
        app.buttons["chat-dismiss-keyboard"].tap()
        for _ in 0..<8 where !picker.isHittable { app.scrollViews.firstMatch.swipeDown() }
        picker.tap(); app.buttons["通知验收项目"].tap()
        XCTAssertEqual(input.value as? String, "首项目的目标", "返回首项目应恢复其完整原文")
        XCTAssertFalse((input.value as? String ?? "").contains("第二项目的目标"), "不得搬运另一个项目的目标")
        let save = app.buttons["new-task-save"], send = app.buttons["new-task-send"]
        for _ in 0..<8 where !send.isHittable { app.scrollViews.firstMatch.swipeUp() }
        XCTAssertTrue(save.isHittable, "辅助字号的保存按钮应可点击")
        XCTAssertTrue(send.isHittable, "辅助字号的发送按钮应可见，即使只读禁用")
        XCTAssertFalse(send.isEnabled, "切换项目不得提升权限")
        XCTAssertGreaterThanOrEqual(send.frame.minY, save.frame.maxY, "辅助字号两按钮必须纵排且不重叠")
        XCTAssertLessThanOrEqual(save.frame.maxX, app.frame.maxX, "保存按钮不能溢出屏幕")
        XCTAssertLessThanOrEqual(send.frame.maxX, app.frame.maxX, "发送按钮不能溢出屏幕")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "new-task-accessibility"; shot.lifetime = .keepAlways; add(shot)
        try Data().write(to: replaced)
        let cleared = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in !(input.value as? String ?? "").contains("首项目的目标") }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [cleared], timeout: 8), .completed, "替换窗口不得自动搬运旧目标")
        XCTAssertFalse(send.isEnabled, "新窗口没有目标和控制权限不能发送")
        try FileManager.default.removeItem(at: replaced)
        let restored = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in input.value as? String == "首项目的目标" }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [restored], timeout: 8), .completed, "旧窗口草稿不能因切换被丢弃")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-chat-sent.json").path), "全流程未发送任何任务")
    }

    /// 原型新建任务在同一弹层选择原卡、输入、存草稿和发送，不先进入聊天。
    /// 数据构造：隔离TLS项目先只读；Claude原卡7/run-7/session-7，目标原文为一条中文句子；
    ///   8s = 2s订阅心跳 + 6s原生导航和调度余量，不作为网络性能指标。
    /// 执行过程：打开任务弹层 → 选择Claude → 写目标/收起键盘 → 只读存草稿 → 重开恢复 → 升级隔离授权 → 发送。
    /// 预期结果：未授权不发送；草稿保留且不串Codex；准确原卡收到原文一次；成功打开同一对话并清空输入。
    @MainActor func testNewTaskDraftAndOriginalCardSend() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("仅用于隔离聊天服务")
        }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        let flag = dir.appendingPathComponent("ui-chat-interactions")
        try Data().write(to: flag)
        defer { try? FileManager.default.removeItem(at: flag) }
        let app = XCUIApplication(); app.launch(); app.buttons["workspace-tab-tasks"].tap()
        let entry = app.buttons["new-task"]
        XCTAssertTrue(entry.waitForExistence(timeout: 8), "任务页应保留加号入口")
        entry.tap()
        let input = app.descendants(matching: .any)["new-task-input"].firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 8), "点击加号应直接看到目标输入，而非卡片目录")
        guard input.exists else { return }
        XCTAssertFalse(app.segmentedControls["chat-content-tabs"].exists, "输入目标前不应先打开对话")
        app.buttons["new-task-agent"].tap(); app.buttons["new-task-agent-7"].tap()
        let message = "请继续检查连接"
        input.tap(); input.typeText(message)
        let hide = app.buttons["chat-dismiss-keyboard"]
        XCTAssertTrue(hide.waitForExistence(timeout: 3), "表单也必须能收起键盘")
        hide.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "收起按钮应真正关闭键盘")
        let send = app.buttons["new-task-send"]
        XCTAssertFalse(send.isEnabled, "只读权限不能从新弹层发送")
        app.buttons["new-task-agent"].tap(); app.buttons["new-task-agent-1"].tap()
        XCTAssertFalse((input.value as? String ?? "").contains(message), "Claude草稿不能串到Codex")
        app.buttons["new-task-agent"].tap(); app.buttons["new-task-agent-7"].tap()
        XCTAssertEqual(input.value as? String, message, "切回原Claude卡恢复目标原文")
        app.buttons["new-task-save"].tap()
        XCTAssertTrue(entry.waitForExistence(timeout: 8), "保存后回到任务页")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-chat-sent.json").path), "保存草稿不得发送任务")
        entry.tap()
        XCTAssertTrue(input.waitForExistence(timeout: 8), "重开仍是目标表单")
        XCTAssertEqual(input.value as? String, message, "重开应记住原Agent和已保存目标")
        try Data().write(to: dir.appendingPathComponent("ui-card-management"))
        let enabled = XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"), object: send)
        XCTAssertEqual(XCTWaiter.wait(for: [enabled], timeout: 8), .completed, "正式权限更新后才允许发送")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "new-task-form"; shot.lifetime = .keepAlways; add(shot)
        send.tap()
        XCTAssertTrue(app.segmentedControls["chat-content-tabs"].waitForExistence(timeout: 8), "准确成功回执后打开原对话")
        XCTAssertTrue(app.staticTexts["实时回复第一段"].waitForExistence(timeout: 8), "原会话结果应继续实时显示")
        let chatInput = app.descendants(matching: .any)["chat-message-input"].firstMatch
        XCTAssertFalse((chatInput.value as? String ?? "").contains(message), "成功发送后不应残留目标原文")
        let sent = try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("ui-chat-sent.json"))) as? [[String: String]]
        XCTAssertEqual(sent?.count, 1, "新表单只能提交一次")
        XCTAssertEqual(sent?.first?["text"], message, "保留目标原文；fixture另核对原node/run/session")
    }

    /// 指挥台首层直接显示最新方向、工作简报和Agent待办，不能展示旧计划或越过只读权限。
    ///
    /// 数据构造：notification-ui持久化旧/新两份计划，最新计划含一项Codex建议；8s为TLS读取与原生导航余量。
    /// 执行过程：选择隔离电脑 → 指挥台默认行动计划 → 读取最新简报和待办 → 调整方向 → 返回 → 切换待你决策再回来。
    /// 预期结果：无需进入项目目录/计划列表；旧计划不显示，只读不能安排或生成；两分段各自保持正确内容。
    @MainActor func testCommandCenterShowsLatestPlanAndTodoDirectly() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui" else {
            throw XCTSkip("仅使用隔离只读计划fixture")
        }
        let app = XCUIApplication(); app.launch()
        app.buttons["manage-computers"].tap()
        let machine = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        XCTAssertTrue(machine.waitForExistence(timeout: 8), "只操作隔离电脑")
        machine.tap(); app.buttons["workspace-tab-command"].tap()
        let planMode = app.buttons["command-mode-plan"]
        XCTAssertTrue(planMode.isSelected, "默认直接显示行动计划")
        XCTAssertTrue(app.staticTexts["优先解决手机连接延迟"].waitForExistence(timeout: 8), "当前方向须从实际数据库读取")
        XCTAssertTrue(app.staticTexts["先核对连接路径，再整理实际测量结果。"].exists, "工作简报须来自最新计划")
        XCTAssertFalse(app.staticTexts["旧工作简报"].exists, "不能误把最早计划当作当前方向")
        let todo = app.staticTexts["核对连接路径"]
        if !todo.isHittable { app.swipeUp() }
        XCTAssertTrue(todo.waitForExistence(timeout: 8), "待办应直接显示，不应再点击一层计划列表")
        let detail = app.buttons["todo-open-0"]
        scrollToWorkspaceRow(detail, in: app)
        detail.tap()
        let accept = app.buttons["todo-accept-0"]
        if !accept.isHittable { app.swipeUp() }
        XCTAssertTrue(accept.exists, "保留真实Todo操作及禁用状态")
        XCTAssertFalse(accept.isEnabled, "只读手机不能通过新指挥台安排任务")
        app.buttons["完成"].tap()
        app.buttons["adjust-project-direction"].tap()
        XCTAssertTrue(app.buttons["generate-project-plan"].waitForExistence(timeout: 8), "调整方向复用既有真实生成页面")
        XCTAssertFalse(app.buttons["generate-project-plan"].isEnabled, "只读不能生成新计划")
        app.buttons["完成"].tap()
        XCTAssertTrue(app.buttons["command-plan-project"].waitForExistence(timeout: 8), "关闭方向页返回原指挥台")
        app.buttons["command-mode-decisions"].tap()
        XCTAssertTrue(app.staticTexts["command-decision-scope"].waitForExistence(timeout: 8), "决策继续汇总全部获授权电脑")
        XCTAssertEqual(app.staticTexts["command-decision-scope"].label, "全部电脑，待你决策", "范围仍明确覆盖全部电脑")
        XCTAssertFalse(app.buttons["todo-accept-0"].exists, "决策页不残留计划操作")
        planMode.tap()
        XCTAssertTrue(app.staticTexts["优先解决手机连接延迟"].waitForExistence(timeout: 8), "返回行动计划重新读取准确方向")
    }

    /// 对话读取最终失败后退出重连提示，避免用户误以为请求仍在等待。
    ///
    /// 数据构造：notification-ui只提供真实项目WSS、不提供Provider历史；12s = 1+2+4s重试退避 + 5s调度余量。
    /// 执行过程：选择隔离电脑 → 打开合成卡 → 正式历史接口失败并完成有限重试。
    /// 预期结果：显示读取错误，旧“正在重新读取”提示和加载指示均消失；只读发送仍禁用。
    @MainActor func testUnavailableConversationStopsLoadingNotice() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui" else {
            throw XCTSkip("仅用于不提供Provider历史的隔离UI服务")
        }
        let app = XCUIApplication(); app.launch()
        app.buttons["manage-computers"].tap()
        let machine = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        XCTAssertTrue(machine.waitForExistence(timeout: 5), "仅操作明确的隔离电脑")
        machine.tap()
        let card = app.buttons["agent-card-1"]
        XCTAssertTrue(card.waitForExistence(timeout: 5), "须进入准确原卡")
        card.tap()
        let failure = app.staticTexts["电脑暂时无法处理请求。已发送的操作请查询结果。"]
        XCTAssertTrue(failure.waitForExistence(timeout: 12), "有限重试结束必须呈现真实错误")
        XCTAssertFalse(app.staticTexts["正在重新读取完整对话…"].exists, "失败后不能残留进行中的重连提示")
        XCTAssertFalse(app.progressIndicators["正在读取对话…"].exists, "失败后不能继续显示加载进度")
        XCTAssertFalse(app.buttons["chat-send-message"].isEnabled, "读取失败不能扩大只读权限")
    }

    /// 任务页使用真实WSS状态，空白卡不冒充任务，未知项目也不能被显示成零任务。
    ///
    /// 数据构造：notification-ui隔离只读项目，Codex有一项已完成任务，Claude为空白卡；
    ///   8s = 2s服务心跳 + 6s原生导航/调度余量，不是网络性能指标。
    /// 执行过程：选择验收电脑及首项目 → 任务筛选 → 服务端改运行状态 → 搜索与清除 → 打开原对话并返回 → 服务端不可读再恢复。
    /// 预期结果：状态变化自动更新，不显示空白卡；只读不得发送；不可读不显示冗余提示，也不能冒充零任务。
    @MainActor func testLiveTaskFiltersAndConversationReturn() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"], path.hasPrefix("/") else {
            throw XCTSkip("需要隔离notification-ui服务及显式验收模拟器")
        }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        let running = directory.appendingPathComponent("ui-task-running")
        let unavailable = directory.appendingPathComponent("ui-project-unavailable")
        defer { try? FileManager.default.removeItem(at: running); try? FileManager.default.removeItem(at: unavailable) }
        let app = XCUIApplication(); app.launch()
        app.buttons["manage-computers"].tap()
        let machine = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        XCTAssertTrue(machine.waitForExistence(timeout: 8), "只操作隔离电脑")
        machine.tap(); app.buttons["workspace-tab-tasks"].tap()
        let task = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "本地通知定位验收")).firstMatch
        XCTAssertTrue(task.waitForExistence(timeout: 8), "任务应来自已配对服务的真实WSS快照")
        XCTAssertFalse(app.staticTexts["空白验收卡片"].exists, "空白Agent不应成为任务")
        XCTAssertFalse(app.staticTexts["第二验收项目：项目尚未在电脑上打开"].exists, "未打开项目不再逐条显示橙色提示")
        app.buttons["task-project-filter"].tap()
        let scope = app.buttons["通知验收项目"]
        XCTAssertTrue(scope.waitForExistence(timeout: 3), "只筛选首项目的任务，其他未打开项目不能混入无匹配状态")
        scope.tap()
        app.buttons["task-filter-已完成"].tap()
        XCTAssertTrue(app.buttons["task-filter-已完成"].isSelected, "玻璃选中项必须对应实际完成筛选")
        XCTAssertFalse(app.buttons["task-filter-all"].isSelected, "选中完成后全部不能仍为选中态")
        XCTAssertTrue(task.exists, "完成筛选应保留已完成任务")
        app.buttons["task-filter-进行中"].tap()
        XCTAssertTrue(app.buttons["task-filter-进行中"].isSelected, "选中高亮必须随筛选移动")
        XCTAssertFalse(app.buttons["task-filter-已完成"].isSelected, "旧选中项必须退出")
        XCTAssertTrue(app.staticTexts["没有匹配的任务"].waitForExistence(timeout: 8), "完成任务不得混入进行中")
        XCTAssertFalse(task.exists, "完成任务应从进行中结果中消失")
        try Data().write(to: running, options: .atomic)
        XCTAssertTrue(task.waitForExistence(timeout: 8), "状态变化无需手动刷新或重开Tab")
        let search = app.textFields["task-search"]
        search.tap(); search.typeText("NoMatchForNorma")
        XCTAssertTrue(app.staticTexts["没有匹配的任务"].waitForExistence(timeout: 8), "查询应立即筛除无关任务")
        XCTAssertFalse(task.exists, "无关任务不得残留")
        app.buttons["clear-task-filters"].tap()
        XCTAssertTrue(task.waitForExistence(timeout: 8), "清除筛选应恢复原任务")
        // 恢复完成态再进入会话，不让合成运行状态影响只读输入校验。
        try FileManager.default.removeItem(at: running)
        app.buttons["task-filter-已完成"].tap()
        XCTAssertTrue(task.waitForExistence(timeout: 8), "同一订阅也应接收完成更新")
        app.buttons["workspace-tab-projects"].tap()
        app.buttons["workspace-tab-tasks"].tap()
        XCTAssertTrue(app.buttons["task-filter-已完成"].isSelected, "切换底栏后保留实际任务筛选")
        XCTAssertTrue(task.waitForExistence(timeout: 8), "玻璃底栏切换后仍订阅原任务")
        task.tap()
        let send = app.buttons["chat-send-message"]
        XCTAssertTrue(send.waitForExistence(timeout: 8), "任务应直接打开原Agent对话")
        XCTAssertFalse(send.isEnabled, "任务入口不能提升只读权限")
        XCTAssertFalse(app.buttons["workspace-tab-tasks"].exists, "共享底栏不能盖住聊天页和键盘")
        let back = app.navigationBars.buttons.matching(NSPredicate(format: "label IN %@", ["任务", "Back", "返回"])).firstMatch
        XCTAssertTrue(back.waitForExistence(timeout: 8), "聊天导航不能因任务源onDisappear清空而丢失")
        back.tap()
        XCTAssertTrue(app.buttons["workspace-tab-tasks"].isSelected, "返回后玻璃底栏仍选中任务")
        XCTAssertTrue(task.waitForExistence(timeout: 8), "返回应恢复同一筛选结果")
        try Data().write(to: unavailable, options: .atomic)
        // 等待首项目的WSS失效更新，避免在仍有旧任务时提前验证不可读状态。
        XCTAssertTrue(task.waitForNonExistence(timeout: 8), "收到不可读更新后，旧任务必须在订阅等待预算内退出")
        XCTAssertFalse(app.staticTexts["部分任务暂时无法读取"].exists, "任务列表不再显示冗余读取提示")
        XCTAssertFalse(app.staticTexts["已连接项目的任务仍会更新，其他项目恢复连接后自动显示。"].exists, "提示说明也应一并移除")
        XCTAssertEqual(app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "通知验收项目：")).count, 0, "已打开项目读取失败也不应重复显示逐项目错误")
        XCTAssertFalse(app.staticTexts["当前没有任务"].exists, "未知任务数不能显示为零")
        XCTAssertFalse(app.staticTexts["没有匹配的任务"].exists, "不可读不是筛选无结果")
        XCTAssertFalse(task.exists, "不可读时必须清除过期任务")
        try FileManager.default.removeItem(at: unavailable)
        XCTAssertTrue(task.waitForExistence(timeout: 8), "服务恢复后原任务自动回来")
    }
    /// 任务卡显示WSS携带的摘要、耗时和步骤，字段移除后不残留旧值。
    ///
    /// 数据构造：隔离fixture的卡1显示60000ms=1分钟；8s覆盖2s心跳和6s导航余量。
    /// 执行过程：启用摘要/计时/2步fixture → 更新摘要与2/2 → 移除可选字段。
    /// 预期结果：计时显示在原任务卡内；字段缺失后没有“耗时0分钟”，原任务仍可打开。
    @MainActor func testTaskTimingUpdatesAndOldMacFallback() throws {
        try verifyTaskTiming(accessibility: false)
    }

    /// 最大辅助字号下耗时和任务入口都可读，移除可选字段后不残留伪值。
    /// 数据构造：同一1分钟fixture，AX5字体；8s为TLS与原生导航余量。
    /// 执行过程：以AX5启动并复用正常字号的完整WSS增加/移除流程。
    /// 预期结果：文字包含在原任务卡内、不溢出屏幕；旧Mac不显示零耗时。
    @MainActor func testAccessibleTaskTimingAndOldMacFallback() throws {
        try verifyTaskTiming(accessibility: true)
    }

    @MainActor private func verifyTaskTiming(accessibility: Bool) throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"], path.hasPrefix("/") else {
            throw XCTSkip("仅使用隔离验收服务")
        }
        let marker = URL(fileURLWithPath: path).deletingLastPathComponent().appendingPathComponent("ui-task-timing")
        let progressMarker = marker.deletingLastPathComponent().appendingPathComponent("ui-task-progress")
        let summaryMarker = marker.deletingLastPathComponent().appendingPathComponent("ui-task-summary")
        let firstSummary = "已定位重复转写的触发条件，正在检查手机和电脑之间的连接恢复。"
        let finalSummary = "连接恢复验证已完成，原卡片中的会话内容保持一致。"
        try Data().write(to: marker, options: .atomic)
        try Data().write(to: progressMarker, options: .atomic)
        try Data(firstSummary.utf8).write(to: summaryMarker, options: .atomic)
        defer {
            try? FileManager.default.removeItem(at: marker)
            try? FileManager.default.removeItem(at: progressMarker)
            try? FileManager.default.removeItem(at: summaryMarker)
        }
        let app = XCUIApplication()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", accessibility ? "UICTContentSizeCategoryAccessibilityXXXL" : "UICTContentSizeCategoryL"]
        app.launch()
        app.buttons["manage-computers"].tap()
        let machine = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        XCTAssertTrue(machine.waitForExistence(timeout: 8), "只能进入隔离电脑")
        machine.tap(); app.buttons["workspace-tab-tasks"].tap()
        let summary = app.staticTexts["task-summary-1"]
        XCTAssertTrue(summary.waitForExistence(timeout: 8), "摘要须出现在原任务卡内")
        XCTAssertEqual(summary.label, firstSummary, "显示真实摘录，不能继续使用状态模板")
        XCTAssertLessThanOrEqual(summary.frame.maxX, app.frame.maxX, "摘要不能超出屏幕")
        try Data(finalSummary.utf8).write(to: summaryMarker, options: .atomic)
        expectation(for: NSPredicate(format: "label == %@", finalSummary), evaluatedWith: summary)
        waitForExpectations(timeout: 8)
        XCTAssertFalse(app.staticTexts[firstSummary].exists, "替换后不能残留旧操作说明")
        let summaryAttachment = XCTAttachment(screenshot: app.screenshot())
        summaryAttachment.name = "task-summary"; summaryAttachment.lifetime = .keepAlways; add(summaryAttachment)
        try FileManager.default.removeItem(at: summaryMarker)
        expectation(for: NSPredicate(format: "label == %@", "本轮工作已完成，打开对话查看完整回复与执行结果。"), evaluatedWith: summary)
        waitForExpectations(timeout: 8)
        XCTAssertFalse(app.staticTexts[finalSummary].exists, "字段撤销或旧Mac不能保留之前的摘要")
        let elapsed = app.staticTexts["task-elapsed-1"]
        XCTAssertTrue(elapsed.waitForExistence(timeout: 8), "正式耗时必须可见")
        XCTAssertEqual(elapsed.label, "耗时 1 分钟", "完成耗时不能按手机当前时间继续增长")
        let task = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "本地通知定位验收")).firstMatch
        XCTAssertTrue(task.exists, "耗时不能替代原任务入口")
        // Up to six short list scrolls cover an AX5 card without using the command-center-only helper.
        for _ in 0..<6 {
            if elapsed.isHittable && elapsed.frame.maxY < app.buttons["workspace-tab-tasks"].frame.minY { break }
            app.scrollViews.firstMatch.swipeUp()
        }
        XCTAssertTrue(elapsed.isHittable, "最大字号下也应能滚动到耗时")
        XCTAssertTrue(task.frame.contains(elapsed.frame), "最大字号的耗时也必须留在任务卡内")
        XCTAssertLessThanOrEqual(elapsed.frame.maxX, app.frame.maxX, "耗时不能溢出屏幕右边")
        let count = app.staticTexts["task-progress-count-1"]
        let stage = app.staticTexts["task-stage-1"]
        XCTAssertTrue(count.waitForExistence(timeout: 8), "原型步骤计数须随WSS显示")
        XCTAssertEqual(count.label, "1 / 2 步", "完成数只能来自服务")
        XCTAssertTrue(stage.label.contains("Checking beta"), "真实阶段必须保留")
        for _ in 0..<6 {
            if count.isHittable && count.frame.maxY < app.buttons["workspace-tab-tasks"].frame.minY { break }
            app.scrollViews.firstMatch.swipeUp()
        }
        XCTAssertTrue(count.isHittable, "AX5也须能滚到步骤")
        XCTAssertTrue(task.frame.contains(count.frame), "步骤计数必须留在任务卡内")
        XCTAssertLessThanOrEqual(stage.frame.maxX, app.frame.maxX, "阶段不可溢出屏幕")
        let attachment = XCTAttachment(screenshot: app.screenshot()); attachment.name = "task-timing-and-progress"; attachment.lifetime = .keepAlways; add(attachment)
        try Data("done".utf8).write(to: progressMarker, options: .atomic)
        expectation(for: NSPredicate(format: "label == %@", "2 / 2 步"), evaluatedWith: count)
        waitForExpectations(timeout: 8)
        XCTAssertTrue(stage.label.contains("步骤已完成"), "WSS推进后应显示完成阶段")
        try FileManager.default.removeItem(at: progressMarker)
        expectation(for: NSPredicate(format: "exists == false"), evaluatedWith: count)
        waitForExpectations(timeout: 8)
        XCTAssertFalse(stage.exists, "计划缺失不能残留旧阶段")
        try FileManager.default.removeItem(at: marker)
        let disappeared = NSPredicate(format: "exists == false")
        expectation(for: disappeared, evaluatedWith: elapsed)
        waitForExpectations(timeout: 8)
        XCTAssertFalse(app.staticTexts["耗时 0 分钟"].exists, "旧Mac缺字段不能伪造零耗时")
        XCTAssertTrue(task.exists, "仅移除显示数据不应丢失原任务")
    }

}
