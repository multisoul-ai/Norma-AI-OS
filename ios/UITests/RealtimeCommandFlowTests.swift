import XCTest

final class RealtimeCommandFlowTests: XCTestCase {
    /// 默认语音服务免Key；未配对时主动开聊只提示连接Mac。
    /// 数据构造：隔离模拟器、标准UserDefaults启动参数选择正式默认网关，无手动口令。
    /// 执行过程：真实App打开指挥台 → 设置查看自动配置/缺少Key输入 → 关闭 → 主动开聊。
    /// 预期结果：自动配置可见、URL/Key输入不存在；未配对不弹设置且没有通话结束按钮。
    @MainActor func testDefaultVoiceRequiresNoKeyConfiguration() {
        let app=XCUIApplication()
        app.launchArguments += ["-realtime-gateway", "https://norma-voice.vercel.app", "-realtime-provider", "doubao"]
        app.launch();app.buttons["workspace-tab-command"].tap();app.buttons["realtime-settings"].tap()
        // 8s = scene loading5s + native navigation3s，等待真实布局完成。
        XCTAssertTrue(app.descendants(matching:.any)["realtime-automatic-service"].firstMatch.waitForExistence(timeout:8),"默认服务应显示自动配置")
        XCTAssertFalse(app.textFields["realtime-gateway"].exists,"用户不能被要求填写服务地址")
        XCTAssertFalse(app.secureTextFields["realtime-access-token"].exists,"用户不能被要求填写Key或口令")
        app.buttons["realtime-settings-close"].tap();app.buttons["realtime-call-start"].tap()
        XCTAssertTrue(app.staticTexts["请先配对并选择 Mac 项目，语音服务会自动配置。"].waitForExistence(timeout:3),"无配对时应给出可操作提示")
        XCTAssertFalse(app.buttons["realtime-settings-close"].exists,"不能把免Key入口退回手动设置")
        XCTAssertFalse(app.buttons["realtime-call-end"].exists,"没有授权不能伪造接通")
    }

    /// 陪伴页面在最大辅助字号下仍能开聊、阅读提醒和关闭设置。
    /// 数据构造：AX5 为系统最大辅助字号；44pt 为主要操作的最小触控目标。
    ///   8s = 5s 场景加载 + 3s 原生导航；4 次滚动覆盖提醒区，不降低用户字号。
    /// 执行过程：打开指挥台 → 审计首屏和底栏 → 滚动到待决定 → 打开并关闭设置。
    /// 预期结果：开聊无遮挡、主按钮可触达；没有文字裁切，不产生通话或任务。
    @MainActor func testAccessibleCompanionControls() throws {
        let app = XCUIApplication()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        app.launch(); app.buttons["workspace-tab-command"].tap()
        let start = app.buttons["realtime-call-start"]
        XCTAssertTrue(start.waitForExistence(timeout: 8), "最大字号仍应显示主动开聊入口")
        XCTAssertGreaterThanOrEqual(start.frame.height, 44, "开聊按钮至少44pt")
        XCTAssertLessThanOrEqual(start.frame.maxY, app.buttons["workspace-tab-command"].frame.minY, "底栏不能遮挡开聊按钮")
        XCTAssertFalse(app.buttons["realtime-call-end"].exists, "查看页面不能自动开始通话")
        try app.performAccessibilityAudit(for: [.textClipped, .hitRegion, .sufficientElementDescription])
        let decisions = app.buttons["command-decision-inbox"]
        scrollToWorkspaceRow(decisions, in: app)
        XCTAssertTrue(decisions.isHittable, "大字提醒应可滚动到达")
        XCTAssertLessThanOrEqual(decisions.frame.maxY, start.frame.minY, "提醒必须完整滚动到通话控制栏上方")
        XCTAssertGreaterThanOrEqual(decisions.frame.height, 44, "决定入口至少44pt")
        decisions.tap()
        XCTAssertTrue(app.buttons["command-decisions-close"].waitForExistence(timeout: 8), "大字号必须能实际打开决定弹层")
        app.buttons["command-decisions-close"].tap()
        app.buttons["realtime-settings"].tap()
        XCTAssertTrue(app.buttons["realtime-settings-close"].waitForExistence(timeout: 8), "设置可独立关闭")
        app.buttons["realtime-settings-close"].tap()
        XCTAssertFalse(app.buttons["realtime-call-end"].exists, "查看设置不能创建媒体连接")
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "原生陪伴指挥台-AX5"; shot.lifetime = .keepAlways; add(shot)
    }

    /// 陪伴指挥台替换旧布局，并保留审批记录、项目与任务导航。
    /// 数据构造：正式 App、标准字号，无需在线 Mac；仅验证本地入口，不构造通话或执行成功。
    ///   人物区域至少占屏宽95%（两侧总留白≤5%）、屏高44%（874pt屏约385pt），避免退回340pt小框。
    /// 执行过程：打开指挥台 → 查看角色和通话设置 → 查看全部电脑审批/记录 → 切换任务与项目。
    /// 预期结果：真实角色与新控件可访问，旧分段和简报卡不存在，未开聊不显示通话中。
    @MainActor func testCompanionEntryAndSharedDestinations() throws {
        let app = XCUIApplication()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryL"]
        app.launch()
        app.buttons["workspace-tab-command"].tap()
        // 8s = 场景资源加载余量 5s + 原生导航调度 3s；不作为性能指标。
        XCTAssertTrue(app.buttons["realtime-call-start"].waitForExistence(timeout: 8), "指挥台应提供主动开聊入口")
        XCTAssertTrue(app.otherElements["realtime-avatar"].exists, "必须呈现迁入的真实角色")
        let avatar = app.otherElements["realtime-avatar"]
        XCTAssertGreaterThanOrEqual(avatar.frame.width, app.frame.width * 0.95, "人物应接近全宽，不能被文字栏的左右边距压缩")
        XCTAssertGreaterThanOrEqual(avatar.frame.height, app.frame.height * 0.44, "人物应占据首屏主要区域，不能退回340pt小框")
        XCTAssertLessThanOrEqual(app.buttons["realtime-project"].frame.maxY, avatar.frame.minY, "项目栏不能覆盖人物")
        XCTAssertLessThanOrEqual(avatar.frame.maxY, app.staticTexts["realtime-subtitle"].frame.minY, "字幕必须在人物下方，不能覆盖脸部或手势")
        XCTAssertTrue(app.buttons["command-decision-inbox"].isHittable, "标准字号首屏应能打开待决定，无需先滚动")
        XCTAssertTrue(app.buttons["realtime-project"].exists, "派工绑定项目必须持续可见")
        XCTAssertFalse(app.buttons["command-mode-plan"].exists, "旧行动计划分段必须删除")
        XCTAssertFalse(app.buttons["command-mode-decisions"].exists, "旧决策分段必须删除")
        XCTAssertFalse(app.staticTexts["command-hero-title"].exists, "旧简报大卡必须删除")
        XCTAssertFalse(app.buttons["realtime-call-end"].exists, "未开聊不能伪造通话状态")
        app.buttons["realtime-project"].tap()
        app.buttons["realtime-discussions"].tap()
        XCTAssertTrue(app.buttons["realtime-discussions-close"].waitForExistence(timeout: 3), "草稿弹层应有原生退出入口")
        XCTAssertTrue(app.staticTexts["讨论仅作记录，不会自动派工。"].exists, "草稿恢复与派工必须明确分开")
        app.buttons["realtime-discussions-close"].tap()
        XCTAssertFalse(app.buttons["realtime-call-end"].exists, "查看草稿不能自动开聊")
        app.buttons["realtime-project"].tap()
        app.buttons["realtime-operations"].tap()
        XCTAssertTrue(app.buttons["realtime-operations-close"].waitForExistence(timeout: 3), "操作记录可独立关闭")
        XCTAssertTrue(app.staticTexts["任务已接收后仍需执行；成果是否通过验收，以成果记录为准。"].exists,
                      "不能把接收、完成和用户验收合并")
        app.buttons["realtime-operations-close"].tap()
        XCTAssertLessThanOrEqual(app.buttons["realtime-call-start"].frame.maxY,
                                 app.buttons["workspace-tab-command"].frame.minY,
                                 "开聊按钮必须完整位于底栏上方，不能被悬浮导航遮挡")
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "原生陪伴指挥台"; screenshot.lifetime = .keepAlways; add(screenshot)
        app.buttons["realtime-settings"].tap()
        XCTAssertTrue(app.buttons["realtime-settings-close"].waitForExistence(timeout: 3), "原生设置弹层应可关闭")
        let provider = app.buttons["realtime-provider"]
        XCTAssertTrue(provider.exists, "设置必须提供原生对话服务选择")
        provider.tap(); app.buttons["GPT"].tap()
        XCTAssertTrue(provider.waitForExistence(timeout: 3), "选择GPT后应返回设置")
        XCTAssertTrue(provider.label.contains("GPT") || (provider.value as? String)?.contains("GPT") == true,
                      "服务选择必须真实更新，不能只关闭菜单")
        provider.tap(); app.buttons["豆包"].tap()
        XCTAssertTrue(provider.waitForExistence(timeout: 3), "选择豆包后应返回设置")
        XCTAssertTrue(provider.label.contains("豆包") || (provider.value as? String)?.contains("豆包") == true,
                      "豆包必须成为当前服务")
        XCTAssertFalse(app.buttons["realtime-call-end"].exists, "更改服务不能自动开通话")
        app.buttons["realtime-settings-close"].tap()
        app.buttons["command-decision-inbox"].tap()
        XCTAssertTrue(app.buttons["approval-records"].waitForExistence(timeout: 3), "全部电脑审批保留记录入口")
        app.buttons["approval-records"].tap()
        XCTAssertTrue(app.buttons["approval-records-close"].waitForExistence(timeout: 3), "决定记录可独立关闭")
        app.buttons["approval-records-close"].tap()
        app.buttons["command-decisions-close"].tap()
        app.buttons["workspace-tab-tasks"].tap()
        XCTAssertTrue(app.buttons["workspace-tab-tasks"].isSelected, "任务标签和原导航必须保留")
        app.buttons["workspace-tab-projects"].tap()
        XCTAssertTrue(app.buttons["workspace-tab-projects"].isSelected, "项目导航必须继续可用")
    }
}
