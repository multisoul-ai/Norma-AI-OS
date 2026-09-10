import XCTest

final class ProjectBoardFlowTests: XCTestCase {
    /// 项目默认显示原卡看板，切换项目后卡片、权限和指挥台必须属于同一项目。
    ///
    /// 数据构造：隔离Mac有两个只读项目，第二个默认关闭；两项目复用node_id=1以暴露旧视图残留。
    /// 执行过程：顶部项目名与电脑名/无重复正文信息 → 顶部电脑切换 → 选择关闭项目 → 服务打开项目 → 指挥台 → 切回第一项目。
    /// 预期结果：项目和电脑名在搜索栏上方，电脑切换可触达；关闭态不能创建，不残留其他项目卡片或方向。
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
        let switcher = app.buttons["project-switcher"]
        XCTAssertEqual(switcher.label, "通知验收项目", "顶部必须直接显示当前项目名，不能仍显示泛化的项目标签")
        XCTAssertEqual(app.staticTexts["project-page-title"].label, "通知验收项目", "项目名称按原型在独立大标题中展示")
        XCTAssertLessThanOrEqual(switcher.frame.maxY, app.staticTexts["project-page-title"].frame.minY, "项目选择位于大标题上方")
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
        XCTAssertTrue(app.staticTexts["项目未在电脑上打开"].waitForExistence(timeout: 8), "关闭项目必须明确说明状态")
        XCTAssertEqual(switcher.label, "第二验收项目", "切换后顶部立即显示第二项目，即使项目尚未打开")
        XCTAssertEqual(app.staticTexts["project-page-title"].label, "第二验收项目", "关闭项目也应保留准确大标题")
        XCTAssertFalse(card.exists, "同node ID的旧卡片不能留在关闭项目")
        XCTAssertFalse(app.buttons["create-card-entry"].exists, "关闭项目不得创建卡片")
        try Data().write(to: opened, options: .atomic)
        XCTAssertTrue(card.waitForExistence(timeout: 8), "打开项目后订阅应自动显示卡片")
        XCTAssertTrue(card.label.contains("第二项目原卡"), "必须切换到第二项目的真实快照")
        XCTAssertFalse(app.staticTexts["通知回源验收卡片"].exists, "不能串回第一项目")
        let direction = app.buttons["project-command-center"]
        if !direction.isHittable { app.swipeUp() }
        direction.tap()
        XCTAssertTrue(app.buttons["workspace-tab-command"].isSelected, "方向入口必须跳至居中指挥台")
        XCTAssertTrue(app.buttons["command-plan-project"].waitForExistence(timeout: 8), "切换Tab后等待指挥台完成挂载")
        XCTAssertEqual(app.buttons["command-plan-project"].value as? String, "第二验收项目", "指挥台应保留所选项目")
        XCTAssertTrue(app.staticTexts["先定一个方向"].waitForExistence(timeout: 8), "第二项目没有计划，应显示真实空态")
        XCTAssertFalse(app.staticTexts["优先解决手机连接延迟"].exists, "不得错取第一项目的计划")
        app.buttons["workspace-tab-projects"].tap()
        app.buttons["project-switcher"].tap()
        app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收项目")).firstMatch.tap()
        XCTAssertTrue(card.waitForExistence(timeout: 8), "切回第一项目应恢复其卡片")
        XCTAssertTrue(card.label.contains("通知回源验收卡片"), "视图不能缓存同ID的其他项目卡")
        XCTAssertEqual(switcher.label, "通知验收项目", "切回第一项目后导航名称必须同步恢复")
        XCTAssertFalse(app.staticTexts["第二项目原卡"].exists, "第二项目内容应全部退出")
    }

    /// 手机停留指挥台时发现另一端的新方向；项目脉络也读取相同计划，切项目清除旧方向。
    /// 数据构造：只读双项目fixture，首项目已有两份计划；标记请求发布第三份，第二项目没有计划。
    /// 执行过程：指挥台显示旧方向 → 服务发布 → 原页面自动换方向/Todo → 项目脉络 → 切第二项目。
    /// 预期结果：无需刷新；旧方向/旧Todo退出、只读不能执行，空项目不串入首项目计划。
    /// 8s = 2s服务心跳 + 6s原生布局余量，不作为物理iPhone延迟指标。
    @MainActor func testLiveDirectionUpdatesAndProjectContextStayScoped() throws {
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
        XCTAssertTrue(app.staticTexts["优先解决手机连接延迟"].waitForExistence(timeout: 8), "先核验当前方向")
        try Data().write(to: directory.appendingPathComponent("ui-live-plan-request"), options: .atomic)
        XCTAssertTrue(app.staticTexts["下一步验证双端输入"].waitForExistence(timeout: 8), "另一端更新必须在原页面自动出现")
        XCTAssertFalse(app.staticTexts["优先解决手机连接延迟"].exists, "新方向不能和旧方向混用")
        let todo = app.staticTexts["验证同一会话输入"]
        if !todo.isHittable { app.swipeUp() }
        XCTAssertTrue(todo.waitForExistence(timeout: 8), "新计划待办应随方向同步更新")
        XCTAssertFalse(app.staticTexts["核对连接路径"].exists, "旧待办不能留给用户误操作")
        let detail = app.buttons["todo-open-0"]
        scrollToWorkspaceRow(detail, in: app)
        detail.tap()
        let accept = app.buttons["todo-accept-0"]
        if !accept.isHittable { app.swipeUp() }
        XCTAssertTrue(accept.exists, "新计划保留明确的执行入口")
        XCTAssertFalse(accept.isEnabled, "实时更新不得扩大只读授权")
        app.buttons["完成"].tap()
        app.buttons["workspace-tab-projects"].tap()
        let context = app.staticTexts["project-current-direction"]
        if !context.isHittable { app.swipeUp() }
        XCTAssertTrue(context.waitForExistence(timeout: 8), "项目页必须显示真实方向而非固定说明")
        XCTAssertEqual(context.label, "下一步验证双端输入", "脉络与指挥台必须读取同一最新计划")
        XCTAssertFalse(app.staticTexts["先创建卡片，想好目标后再安排任务。"].exists, "已有计划不能继续展示空态说明")
        app.buttons["project-switcher"].tap()
        app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "第二验收项目")).firstMatch.tap()
        let empty = app.staticTexts["还没有设定方向"]
        if !empty.isHittable { app.swipeUp() }
        XCTAssertTrue(empty.waitForExistence(timeout: 8), "无计划项目必须显示真实空态")
        XCTAssertFalse(context.exists, "切换项目不得残留上个项目方向")
        XCTAssertFalse(app.staticTexts["下一步验证双端输入"].exists, "其他项目不能泄露旧计划")
    }

}
