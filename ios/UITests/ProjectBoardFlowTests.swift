import XCTest

final class ProjectBoardFlowTests: XCTestCase {
    /// 卡片名称在输入上方独立显示，空卡不伪造输入；任务页也保留同一名称。
    ///
    /// 数据构造：隔离WSS提供卡1“通知回源验收卡片”及输入“本地通知定位验收”，卡2无输入。
    /// 执行过程：进入隔离电脑 → 核验名称与输入的文字、上下位置 → 核验空卡 → 切任务页。
    /// 预期结果：两个页面名称不被输入替换，空卡没有输入摘要，文字均不溢出所属卡片。
    @MainActor func testCardNamePrecedesInput() throws {
        try verifyCardNameAndInput(accessibility: false)
    }

    /// 最大辅助字号保留名称和输入的上下层级，不通过缩小字号隐藏内容。
    ///
    /// 数据构造：与正常字号相同的两张隔离卡片，系统字号为AX5。
    /// 执行过程：以最大辅助字号启动 → 复用项目/任务页名称、输入和空态的验收。
    /// 预期结果：名称和输入均存在、顺序正确，输入不超出卡片边界。
    @MainActor func testAccessibleCardNamePrecedesInput() throws {
        try verifyCardNameAndInput(accessibility: true)
    }

    @MainActor private func verifyCardNameAndInput(accessibility: Bool) throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui" else {
            throw XCTSkip("仅使用隔离notification-ui服务")
        }
        let app = XCUIApplication()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", accessibility ? "UICTContentSizeCategoryAccessibilityXXXL" : "UICTContentSizeCategoryL"]
        app.launch(); app.buttons["manage-computers"].tap()
        let machine = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        // 8s = 2s服务心跳 + 6s原生导航和布局余量；不作为网络性能阈值。
        XCTAssertTrue(machine.waitForExistence(timeout: 8), "只能进入隔离验收电脑")
        machine.tap()
        let card = app.buttons["agent-card-1"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "必须读取原卡片")
        let name = app.staticTexts["agent-card-name-1"], input = app.staticTexts["agent-card-input-1"]
        XCTAssertTrue(name.exists, "有输入时卡片名称仍须独立可见，不能只存在于按钮的辅助标签")
        XCTAssertEqual(name.label, "通知回源验收卡片", "名称必须来自卡片title，不能被task_title替换")
        XCTAssertTrue(input.exists, "有输入的卡片必须显示对应摘要")
        XCTAssertEqual(input.label, "本地通知定位验收", "输入不能重复显示卡片名称")
        XCTAssertLessThanOrEqual(name.frame.maxY, input.frame.minY, "名称必须位于输入上方，不能重叠")
        XCTAssertTrue(card.frame.contains(input.frame), "输入必须完整留在原卡片内")
        XCTAssertEqual(app.staticTexts["agent-card-name-2"].label, "空白验收卡片", "无输入卡片仍须保留名称")
        XCTAssertFalse(app.staticTexts["agent-card-input-2"].exists, "无输入不能用名称伪造第二行")
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = accessibility ? "card-name-input-AX5" : "card-name-input"
        shot.lifetime = .keepAlways; add(shot)
        app.buttons["workspace-tab-tasks"].tap()
        let taskName = app.staticTexts["task-card-name-1"], taskInput = app.staticTexts["task-card-input-1"]
        XCTAssertTrue(taskName.waitForExistence(timeout: 8), "任务页也必须保留来源卡片名称")
        XCTAssertEqual(taskName.label, "通知回源验收卡片", "任务页与项目页必须指向同一名称")
        XCTAssertEqual(taskInput.label, "本地通知定位验收", "任务页须显示同一输入")
        XCTAssertLessThanOrEqual(taskName.frame.maxY, taskInput.frame.minY, "任务页也必须先名称、后输入")
        XCTAssertLessThanOrEqual(taskInput.frame.maxX, app.frame.maxX, "辅助字号输入不能溢出屏幕")
        XCTAssertFalse(app.staticTexts["task-card-name-2"].exists, "无任务的空卡不能混入任务列表")
    }

    /// 项目默认显示原卡看板，切换项目后卡片、权限和指挥台必须属于同一项目。
    ///
    /// 数据构造：隔离Mac有两个只读项目，第二个默认关闭；两项目复用node_id=1以暴露旧视图残留。
    /// 执行过程：单行项目名/创建按钮 → 顶部电脑切换 → 选择关闭项目 → 服务打开项目 → 指挥台 → 切回第一项目。
    /// 预期结果：项目标题即切换入口，无重复大标题或电脑行；关闭态不能创建，不残留其他项目卡片或方向。
    /// 8s = 2s快照心跳 + 6s原生导航余量，仅为验收等待预算。
    @MainActor func testDefaultBoardSwitchAndCommandContext() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("仅使用隔离双项目fixture")
        }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-project-navigation").path) else {
            throw XCTSkip("需要显式开启双项目验收")
        }
        let opened = directory.appendingPathComponent("ui-secondary-open")
        defer { try? FileManager.default.removeItem(at: opened) }
        let app = XCUIApplication(); app.launch()
        app.buttons["manage-computers"].tap()
        let machine = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        XCTAssertTrue(machine.waitForExistence(timeout: 8), "只选择隔离电脑")
        machine.tap()
        let card = app.buttons["agent-card-1"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "项目Tab必须直接显示原Agent卡片")
        XCTAssertTrue(card.label.contains("通知回源验收卡片"), "默认卡片应属于第一项目")
        XCTAssertFalse(app.staticTexts["项目脉络"].exists, "项目页不再显示脉络卡片")
        XCTAssertFalse(app.buttons["记录与成果"].exists, "脉络内的记录折叠区应一起移除")
        let switcher = app.buttons["project-switcher"]
        XCTAssertEqual(switcher.label, "通知验收项目", "顶部必须直接显示当前项目名，不能仍显示泛化的项目标签")
        XCTAssertFalse(app.staticTexts["project-page-title"].exists, "项目名已经是切换入口，不再另占一行大标题")
        XCTAssertFalse(app.buttons["切换项目所在电脑"].exists, "项目标题下不能重复品牌栏的电脑切换")
        let create = app.buttons["create-card-toolbar"]
        // 同行控件中心允许1pt像素取整差；44pt原生触控高度 + 12pt标题下留白 = 56pt。
        XCTAssertEqual(switcher.frame.midY, create.frame.midY, accuracy: 1, "项目名与创建按钮必须同排居中")
        XCTAssertLessThanOrEqual(app.buttons["card-filters"].frame.minY - switcher.frame.minY, 56, "筛选紧接项目行，不能保留旧大标题和电脑行的空位")
        let computer = app.buttons["manage-computers"]
        XCTAssertTrue(computer.exists, "电脑切换也必须移入顶部导航，不能仍留在卡片正文")
        XCTAssertEqual(switcher.value as? String, "通知验收 Mac", "顶部应保留当前电脑完整身份供VoiceOver读取")
        XCTAssertFalse(app.scrollViews.buttons["manage-computers"].exists, "卡片正文不应重复电脑切换入口")
        XCTAssertTrue(app.buttons["workspace-machine-switch"].exists, "品牌头部始终保留电脑切换")
        let header = XCTAttachment(screenshot: app.screenshot()); header.name = "项目和电脑置顶"; header.lifetime = .keepAlways; add(header)
        if computer.exists {
            XCTAssertTrue(computer.isHittable, "顶部电脑入口应可直接点击")
            computer.tap()
            XCTAssertTrue(machine.waitForExistence(timeout: 8), "顶部入口须打开现有电脑列表")
            machine.tap()
            XCTAssertTrue(card.waitForExistence(timeout: 8), "返回同一电脑仍显示其原卡")
        }
        XCTAssertFalse(app.searchFields["搜索项目"].exists, "目录搜索应移入切换面板")
        app.buttons["create-card-toolbar"].tap()
        XCTAssertTrue(app.buttons["create-empty-card"].waitForExistence(timeout: 3), "只读也能打开创建说明")
        XCTAssertFalse(app.buttons["create-empty-card"].isEnabled, "打开说明不能扩大只读授权")
        app.buttons["new-card-close"].tap()
        app.buttons["project-switcher"].tap()
        let second = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "第二验收项目")).firstMatch
        XCTAssertTrue(second.waitForExistence(timeout: 8), "切换面板应展示当前电脑的其他授权项目")
        second.tap()
        XCTAssertTrue(app.staticTexts["暂时无法加载项目"].waitForExistence(timeout: 8), "关闭项目必须明确说明状态")
        XCTAssertEqual(switcher.label, "第二验收项目", "切换后顶部立即显示第二项目，即使项目尚未打开")
        XCTAssertFalse(app.staticTexts["project-page-title"].exists, "关闭项目也不应恢复重复标题")
        XCTAssertFalse(card.exists, "同node ID的旧卡片不能留在关闭项目")
        XCTAssertFalse(app.buttons["create-card-entry"].exists, "关闭项目不得创建卡片")
        try Data().write(to: opened, options: .atomic)
        XCTAssertTrue(card.waitForExistence(timeout: 8), "打开项目后订阅应自动显示卡片")
        XCTAssertTrue(card.label.contains("第二项目原卡"), "必须切换到第二项目的真实快照")
        XCTAssertFalse(app.staticTexts["通知回源验收卡片"].exists, "不能串回第一项目")
        app.buttons["workspace-tab-command"].tap()
        XCTAssertTrue(app.buttons["workspace-tab-command"].isSelected, "底部仍可直接进入指挥台")
        XCTAssertTrue(app.buttons["realtime-project"].waitForExistence(timeout: 8), "切换Tab后等待指挥台完成挂载")
        XCTAssertEqual(app.buttons["realtime-project"].value as? String, "第二验收项目", "指挥台应保留所选项目")
        XCTAssertFalse(app.staticTexts["工作简报"].exists, "新指挥台不保留旧dashboard")
        XCTAssertFalse(app.staticTexts["优先解决手机连接延迟"].exists, "不得错取第一项目的计划")
        app.buttons["workspace-tab-projects"].tap()
        app.buttons["project-switcher"].tap()
        app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收项目")).firstMatch.tap()
        XCTAssertTrue(card.waitForExistence(timeout: 8), "切回第一项目应恢复其卡片")
        XCTAssertTrue(card.label.contains("通知回源验收卡片"), "视图不能缓存同ID的其他项目卡")
        XCTAssertEqual(switcher.label, "通知验收项目", "切回第一项目后导航名称必须同步恢复")
        XCTAssertFalse(app.staticTexts["第二项目原卡"].exists, "第二项目内容应全部退出")
    }

    /// 手机停留指挥台时发现另一端的新方向，项目页不再重复展示方向。
    /// 数据构造：只读双项目fixture，首项目已有两份计划；标记请求发布第三份，第二项目没有计划。
    /// 执行过程：从指挥台打开方向编辑 → 服务发布 → 提示核对并保留草稿 → 查看新详情 → 切第二项目。
    /// 预期结果：自动发现远端更新且不覆盖草稿；新详情不混旧Todo，只读不能执行，空项目不串数据。
    /// 8s = 2s服务心跳 + 6s原生布局余量，不作为物理iPhone延迟指标。
    @MainActor func testLiveDirectionUpdatesStayInCommandCenter() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("仅使用隔离只读计划fixture")
        }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-project-navigation").path) else {
            throw XCTSkip("需要双项目fixture")
        }
        let app = XCUIApplication(); app.launch()
        app.buttons["manage-computers"].tap()
        let machine = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        XCTAssertTrue(machine.waitForExistence(timeout: 8), "只选择隔离电脑")
        machine.tap(); app.buttons["workspace-tab-command"].tap()
        openCommandPlans(in: app)
        let field = app.descendants(matching: .any)["plan-direction"].firstMatch
        let loaded = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "优先解决手机连接延迟"), object: field)
        XCTAssertEqual(XCTWaiter.wait(for: [loaded], timeout: 8), .completed, "先核验当前方向")
        try Data().write(to: directory.appendingPathComponent("ui-live-plan-request"), options: .atomic)
        let reload = app.buttons["direction-reload-priority"]
        XCTAssertTrue(reload.waitForExistence(timeout: 8), "另一端更新应在编辑页提示核对")
        XCTAssertEqual(field.value as? String, "优先解决手机连接延迟", "远端更新不能覆盖正在编辑的草稿")
        openCommandPlanRecord("下一步验证双端输入", in: app)
        let todo = app.staticTexts["验证同一会话输入"]
        scrollToWorkspaceRow(todo, in: app)
        XCTAssertTrue(todo.exists, "共享详情应显示新计划的待办")
        XCTAssertFalse(app.staticTexts["核对连接路径"].exists, "新详情不得混入旧待办")
        let accept = app.buttons["todo-accept-0"]
        scrollToWorkspaceRow(accept, in: app)
        XCTAssertTrue(accept.isHittable, "新计划保留明确的执行入口")
        XCTAssertFalse(accept.isEnabled, "实时更新不得扩大只读授权")
        closeCommandPlanRecord(in: app)
        app.buttons["workspace-tab-projects"].tap()
        XCTAssertTrue(app.buttons["agent-card-1"].exists, "项目页继续展示Agent卡片")
        XCTAssertFalse(app.staticTexts["project-current-direction"].exists, "方向不再在项目页重复展示")
        XCTAssertFalse(app.buttons["project-command-center"].exists, "被移除的卡片内不再保留跳转按钮")
        app.buttons["project-switcher"].tap()
        app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "第二验收项目")).firstMatch.tap()
        app.buttons["workspace-tab-command"].tap()
        openCommandPlans(in: app)
        let history = app.buttons["direction-history"]
        scrollToWorkspaceRow(history, in: app); history.tap()
        XCTAssertTrue(app.staticTexts["还没有计划建议"].waitForExistence(timeout: 8), "第二项目计划记录应为空")
        XCTAssertFalse(app.staticTexts["下一步验证双端输入"].exists, "其他项目不能泄露旧计划")
    }

}
