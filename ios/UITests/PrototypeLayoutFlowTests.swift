import XCTest

final class PrototypeLayoutFlowTests: XCTestCase {
    /// 三个主页按v28恢复品牌头部、悬浮底栏与各自信息层级，并保留真实项目切换及只读提交保护。
    /// 数据构造：正式配对的隔离Mac，两张原卡和最新计划；普通字号，对应390pt原型布局。
    /// 执行过程：项目→指挥台→任务，检查标题/选择器/正文顺序，保存各页截图，再返回原卡对话。
    /// 预期结果：顶部均显示norma，底部N入口居中；项目无常驻搜索；计划有简报；任务有独立标题和胶囊筛选。
    @MainActor func testThreePagesMatchPrototypeHierarchy() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui" else {
            throw XCTSkip("仅使用已配对隔离服务")
        }
        let app = XCUIApplication()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryL"]
        app.launch(); app.buttons["manage-computers"].tap()
        let machine = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "通知验收 Mac")).firstMatch
        // 8s = 2s服务心跳 + 6s原生导航调度，不是性能验收指标。
        XCTAssertTrue(machine.waitForExistence(timeout: 8), "只选择隔离电脑")
        machine.tap()
        let brand = app.buttons["workspace-machine-switch"]
        XCTAssertTrue(brand.waitForExistence(timeout: 8), "恢复原型norma品牌和电脑切换头部")
        guard brand.exists else { return }
        XCTAssertTrue(brand.label.contains("norma"), "主品牌不能被项目名取代")
        let project = app.buttons["project-switcher"]
        let title = app.staticTexts["project-page-title"]
        XCTAssertTrue(title.exists, "原型项目大标题必须恢复")
        XCTAssertLessThan(brand.frame.maxY, title.frame.minY, "品牌在项目标题上方")
        XCTAssertLessThan(project.frame.minY, title.frame.minY, "项目切换放在项目名称前")
        XCTAssertFalse(app.searchFields["搜索卡片"].exists, "项目首页不应被系统常驻搜索栏挤压")
        let card = app.buttons["agent-card-1"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "原卡保持可访问")
        XCTAssertGreaterThanOrEqual(card.frame.height, 165, "卡片高度至少为原型165pt")
        XCTAssertLessThanOrEqual(card.frame.height, 200, "标准字号卡片不能异常拉长")
        let create = app.buttons["create-card-toolbar"]
        XCTAssertTrue(create.isEnabled, "只读用户也应能打开创建入口查看要求")
        create.tap()
        let confirm = app.buttons["create-empty-card"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 3), "入口应打开真实创建表单")
        XCTAssertFalse(confirm.isEnabled, "可查看入口不代表有创建权限")
        app.buttons["new-card-close"].tap()
        try checkChrome(app, name: "原型还原-项目")
        app.buttons["workspace-tab-command"].tap()
        XCTAssertTrue(app.staticTexts["command-hero-title"].waitForExistence(timeout: 8), "指挥台必须先展示工作简报大卡")
        XCTAssertTrue(app.staticTexts["优先解决手机连接延迟"].exists, "方向继续读取真实最新计划")
        XCTAssertFalse(app.staticTexts["旧工作简报"].exists, "不能使用旧计划填充视觉")
        XCTAssertLessThan(app.staticTexts["command-hero-title"].frame.minY, app.buttons["command-decision-inbox"].frame.minY, "简报在待决策/待验收数据卡上方")
        try checkChrome(app, name: "原型还原-指挥台")
        app.buttons["workspace-tab-tasks"].tap()
        let taskTitle = app.staticTexts["tasks-page-title"]
        XCTAssertTrue(taskTitle.waitForExistence(timeout: 8), "任务页有独立大标题")
        XCTAssertLessThan(taskTitle.frame.maxY, app.buttons["task-project-filter"].frame.minY, "所有项目选择位于任务标题下方")
        XCTAssertLessThan(app.buttons["task-project-filter"].frame.maxY, app.textFields["task-search"].frame.minY, "项目范围位于搜索框上方")
        XCTAssertTrue(app.buttons["task-filter-all"].isSelected, "原型白色全部胶囊默认选中")
        let task = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "本地通知定位验收")).firstMatch
        XCTAssertTrue(task.waitForExistence(timeout: 8), "必须等真实任务卡出现，不能把加载或失败页当成视觉验收")
        try checkChrome(app, name: "原型还原-任务")
        app.buttons["workspace-tab-projects"].tap(); card.tap()
        XCTAssertTrue(app.buttons["chat-send-message"].waitForExistence(timeout: 8), "卡片仍打开原会话")
        XCTAssertFalse(app.buttons["workspace-tab-command"].exists, "聊天页隐藏主页底栏，给对话和键盘完整空间")
    }

    @MainActor private func checkChrome(_ app: XCUIApplication, name: String) throws {
        let left = app.buttons["workspace-tab-projects"], center = app.buttons["workspace-tab-command"], right = app.buttons["workspace-tab-tasks"]
        XCTAssertTrue(app.buttons["workspace-machine-switch"].isHittable, "三个页面都能切换电脑")
        XCTAssertTrue(app.buttons["workspace-notifications"].isHittable, "通知入口保持可访问")
        XCTAssertLessThan(left.frame.midX, center.frame.midX, "指挥台位于项目右侧")
        XCTAssertLessThan(center.frame.midX, right.frame.midX, "指挥台位于任务左侧")
        XCTAssertGreaterThan(right.frame.maxX - left.frame.minX, app.frame.width * 0.8, "悬浮底栏使用原型的近全宽布局")
        XCTAssertEqual(app.tabBars.count, 0, "不能叠加系统胶囊TabBar")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }
}
