import XCTest

final class PrototypeLayoutFlowTests: XCTestCase {
    /// 项目和任务保持v28结构，指挥台采用已确认的原生陪伴布局；原导航与只读保护继续生效。
    /// 数据构造：正式配对的隔离Mac，两张原卡和最新计划；普通字号，对应390pt原型布局。
    /// 执行过程：项目→指挥台→任务，检查标题/选择器/正文顺序，保存各页截图，再返回原卡对话。
    /// 预期结果：项目/任务保留norma头部，底部N入口居中；陪伴页有真实角色、绑定项目与开聊入口，不再显示旧简报。
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
        XCTAssertEqual(project.label, "通知验收项目", "项目名称直接作为标题和切换入口")
        XCTAssertLessThanOrEqual(brand.frame.maxY, project.frame.minY, "品牌在项目标题上方")
        XCTAssertFalse(app.staticTexts["project-page-title"].exists, "不再单独占一行展示大标题")
        XCTAssertFalse(app.buttons["切换项目所在电脑"].exists, "电脑切换仅保留在共享品牌栏")
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
        XCTAssertTrue(app.buttons["workspace-tab-command"].isSelected, "玻璃选中态应移到中间指挥台")
        XCTAssertFalse(app.buttons["workspace-tab-projects"].isSelected, "项目不能残留旧选中态")
        XCTAssertTrue(app.buttons["realtime-call-start"].waitForExistence(timeout: 8), "指挥台应呈现原生陪伴开聊入口")
        XCTAssertTrue(app.otherElements["realtime-avatar"].exists, "应使用真实角色，不使用示意图")
        XCTAssertEqual(app.buttons["realtime-project"].value as? String, "通知验收项目", "开聊前显示准确的项目")
        XCTAssertFalse(app.staticTexts["command-hero-title"].exists, "旧简报布局已由陪伴页面取代")
        XCTAssertFalse(app.buttons["command-mode-plan"].exists, "不保留旧版分段或开关")
        try checkChrome(app, name: "原型还原-指挥台")
        app.buttons["workspace-tab-tasks"].tap()
        XCTAssertTrue(app.buttons["workspace-tab-tasks"].isSelected, "玻璃选中态应移到右侧任务")
        XCTAssertFalse(app.buttons["workspace-tab-command"].isSelected, "指挥台不能残留旧选中态")
        XCTAssertTrue(app.buttons["task-project-filter"].waitForExistence(timeout: 8), "任务页直接显示项目搜索行")
        XCTAssertFalse(app.staticTexts["tasks-page-title"].exists, "任务标题不再独占一行")
        XCTAssertFalse(app.buttons["new-task"].exists, "移除标题旁的加号")
        XCTAssertEqual(app.buttons["task-project-filter"].frame.midY, app.textFields["task-search"].frame.midY, accuracy: 2, "项目范围应与搜索输入共用一行，2pt仅容许原生布局取整")
        XCTAssertLessThan(app.buttons["task-project-filter"].frame.maxX, app.textFields["task-search"].frame.minX, "项目菜单位于输入框左侧，不能覆盖文字输入")
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
        if name == "原型还原-指挥台" {
            XCTAssertTrue(app.buttons["realtime-settings"].isHittable, "陪伴页使用通话设置入口")
            XCTAssertTrue(app.buttons["realtime-project"].isHittable, "绑定项目菜单持续可访问")
        } else {
            XCTAssertTrue(app.buttons["workspace-machine-switch"].isHittable, "项目/任务页保持电脑切换入口")
            XCTAssertTrue(app.buttons["workspace-notifications"].isHittable, "项目/任务通知入口保持可访问")
        }
        XCTAssertLessThan(left.frame.midX, center.frame.midX, "指挥台位于项目右侧")
        XCTAssertLessThan(center.frame.midX, right.frame.midX, "指挥台位于任务左侧")
        XCTAssertGreaterThan(right.frame.maxX - left.frame.minX, app.frame.width * 0.8, "悬浮底栏使用原型的近全宽布局")
        XCTAssertEqual(app.tabBars.count, 0, "不能叠加系统胶囊TabBar")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = name; shot.lifetime = .keepAlways; add(shot)
    }
}
