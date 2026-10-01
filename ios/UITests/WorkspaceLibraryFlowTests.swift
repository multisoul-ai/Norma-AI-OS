import XCTest
import Vision

final class WorkspaceLibraryFlowTests: XCTestCase {
    /// 最大辅助字号仍完整呈现工作空间标题、筛选与分开的三个底栏入口。
    /// 数据构造：全新空库，系统AX5字号；标题4字、筛选3字，三入口保持44pt触控区。
    /// 执行过程：冷启动→工作空间→识别真实截图文字→打开归档筛选→打开新建Sheet。
    /// 预期结果：标题与筛选各完整成行，不逐字竖排；底栏标签分离，两个操作均可达。
    /// 预算：8秒=启动/转场3秒+读取本地库5秒。
    @MainActor func testWorkspaceControlsAtMaximumTextSize() throws { try checkLayout(largeText: true) }

    /// 标准字号保留原型横排标题与筛选，真实操作与辅助字号使用同一路径。
    /// 数据构造：全新空库，标准L字号，三个底栏入口。
    /// 执行过程：复用标题/筛选截图识别和打开筛选、新建的流程。
    /// 预期结果：文字完整、入口可达，不因辅助字号修复破坏标准布局。
    @MainActor func testWorkspaceControlsAtStandardTextSize() throws { try checkLayout(largeText: false) }

    @MainActor private func checkLayout(largeText: Bool) throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = UUID().uuidString
        app.launchArguments = ["-UIPreferredContentSizeCategoryName", largeText ? "UICTContentSizeCategoryAccessibilityXXXL" : "UICTContentSizeCategoryL"]
        app.launch()
        let workspaces = app.buttons["workspace-tab-projects"]
        XCTAssertTrue(workspaces.waitForExistence(timeout: 8), "三个底栏入口必须可用")
        workspaces.tap()
        let create = app.buttons["create-workspace"]
        XCTAssertTrue(create.waitForExistence(timeout: 8), "新建必须出现在工作空间首页")
        let shot = app.screenshot()
        let attachment = XCTAttachment(screenshot: shot)
        attachment.name = largeText ? "P03-controls-AX5" : "P03-controls-standard"
        attachment.lifetime = .keepAlways; add(attachment)
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.recognitionLanguages = ["zh-Hans", "en-US"]
        request.usesLanguageCorrection = false
        try VNImageRequestHandler(cgImage: XCTUnwrap(shot.image.cgImage)).perform([request])
        let rows = (request.results ?? []).compactMap { observation -> (String, CGRect)? in
            guard let text = observation.topCandidates(1).first?.string else { return nil }
            return (text.replacingOccurrences(of: " ", with: ""), observation.boundingBox)
        }
        // Vision uses normalized bottom-left coordinates; exclude bottom navigation from title recognition.
        XCTAssertTrue(rows.contains { $0.0 == "工作空间" && $0.1.midY > 0.5 }, "大标题须完整成行，不能断为工作/空间；实际文字：\(rows.map(\.0))")
        XCTAssertTrue(rows.contains { $0.0.contains("新建") }, "新建按钮须完整成行，不能断为新/建")
        XCTAssertTrue(rows.contains { $0.0.contains("活动中") }, "筛选须完整可读，不能逐字竖排")
        let normaTab = app.buttons["workspace-tab-command"]
        let inboxTab = app.buttons["workspace-tab-tasks"]
        let screenshot = try XCTUnwrap(shot.image.cgImage)
        let workspaceText = try visibleLabel("工作空间", button: workspaces, screenshot: screenshot, appFrame: app.frame)
        let normaText = try visibleLabel("Norma", button: normaTab, screenshot: screenshot, appFrame: app.frame)
        let inboxText = try visibleLabel("收件箱", button: inboxTab, screenshot: screenshot, appFrame: app.frame)
        // 8pt is one standard spacing unit between actual visible labels, including wrapped Chinese text.
        XCTAssertGreaterThanOrEqual(normaText.minX - workspaceText.maxX, 8, "工作空间与Norma文字须有至少8pt留白")
        XCTAssertGreaterThanOrEqual(inboxText.minX - normaText.maxX, 8, "Norma与收件箱文字须有至少8pt留白")
        XCTAssertGreaterThanOrEqual(min(workspaces.frame.width, workspaces.frame.height), 44, "工作空间触控区不得小于44pt")
        XCTAssertGreaterThanOrEqual(min(normaTab.frame.width, normaTab.frame.height), 44, "Norma触控区不得小于44pt")
        XCTAssertGreaterThanOrEqual(min(inboxTab.frame.width, inboxTab.frame.height), 44, "收件箱触控区不得小于44pt")
        XCTAssertFalse(workspaces.frame.intersects(normaTab.frame), "工作空间与Norma触控区不能重叠")
        XCTAssertFalse(normaTab.frame.intersects(inboxTab.frame), "Norma与收件箱触控区不能重叠")
        let filter = app.buttons["workspace-archive-filter"]
        XCTAssertGreaterThanOrEqual(filter.frame.height, 44, "筛选触控高度必须满足44pt")
        filter.tap(); app.buttons["已归档"].tap()
        XCTAssertTrue(filter.label.contains("已归档"), "选择后须反馈归档筛选")
        create.tap()
        XCTAssertTrue(app.textFields["workspace-name"].waitForExistence(timeout: 8), "新建仍可打开原生Sheet")
        XCTAssertFalse(app.buttons["workspace-create-save"].isEnabled, "空名称不能创建")
        app.buttons["取消"].tap()
        normaTab.tap()
        XCTAssertTrue(app.descendants(matching: .any)["norma-private-screen"].waitForExistence(timeout: 8), "Norma入口须进入角色单聊主屏")
        XCTAssertFalse(create.exists, "切换到Norma不得仍显示工作空间内容")
        inboxTab.tap()
        XCTAssertTrue(app.buttons["任务提醒设置"].waitForExistence(timeout: 8), "收件箱入口须进入自己的页面")
        workspaces.tap()
        XCTAssertTrue(create.waitForExistence(timeout: 8), "返回工作空间后新建入口仍可用")
    }

    @MainActor private func visibleLabel(_ label: String, button: XCUIElement, screenshot: CGImage, appFrame: CGRect) throws -> CGRect {
        let area = button.frame.intersection(appFrame)
        let scale = CGFloat(screenshot.width) / appFrame.width
        let pixels = CGRect(x: (area.minX - appFrame.minX) * scale, y: (area.minY - appFrame.minY) * scale,
                            width: area.width * scale, height: area.height * scale)
        let crop = try XCTUnwrap(screenshot.cropping(to: pixels), "只读取对应入口真实可见的文字")
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate; request.recognitionLanguages = ["zh-Hans", "en-US"]
        request.usesLanguageCorrection = false
        try VNImageRequestHandler(cgImage: crop).perform([request])
        let parts = (request.results ?? []).compactMap { observation -> (String, CGRect)? in
            guard let text = observation.topCandidates(1).first?.string.replacingOccurrences(of: " ", with: ""),
                  !text.isEmpty, label.contains(text) else { return nil }
            let box = observation.boundingBox
            return (text, CGRect(x: area.minX + box.minX * area.width, y: area.minY + (1 - box.maxY) * area.height,
                                 width: box.width * area.width, height: box.height * area.height))
        }
        XCTAssertEqual(parts.map(\.0).joined(), label, "底栏\(label)须完整可读，允许中文自然换行")
        return parts.map(\.1).reduce(CGRect.null) { $0.union($1) }
    }

    /// 从新首页创建 Workspace，重启后配置仍在，归档后可重开。
    /// 数据构造：1 个隔离 Workspace 库，初始 0 个 Workspace，创建「移动端体验」1 个；8 秒 = 启动/转场 3 秒 + 本机持久化余量 5 秒。
    /// 执行过程：想法库 → 工作空间 → 新建 → 查看设置 → 重启 → 归档 → 切换归档列表 → 重开。
    /// 预期结果：名称和独立空间保留；设置真实提供项目与 Agent 入口；归档不删除；旧连接首页不出现。
    @MainActor func testCreateArchiveAndReopenWorkspaceFromNewHome() {
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_WORKSPACE_TEST_NAMESPACE"] = UUID().uuidString
        app.launch()
        XCTAssertTrue(app.buttons["record-idea"].waitForExistence(timeout: 8), "首屏应该是想法库")
        app.buttons["workspace-tab-projects"].tap()
        app.buttons["create-workspace"].tap()
        app.textFields["workspace-name"].tap()
        app.textFields["workspace-name"].typeText("移动端体验")
        app.buttons["workspace-create-save"].tap()
        let card = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-card-")).firstMatch
        let settings = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-configure-")).firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 8), "本地创建后应该出现真实 Workspace 卡片")
        settings.tap()
        XCTAssertTrue(app.buttons["workspace-add-project"].waitForExistence(timeout: 8), "设置中应提供项目配置入口")
        XCTAssertTrue(app.buttons["workspace-add-agent"].exists, "设置中应提供 Agent 配置入口")
        XCTAssertFalse(app.staticTexts["iOS App"].exists, "不能把原型项目写成用户的真实关联")
        let settingsShot = XCTAttachment(screenshot: app.screenshot())
        settingsShot.name = "P04-workspace-settings-empty"
        settingsShot.lifetime = .keepAlways
        add(settingsShot)
        app.terminate(); app.launch()
        app.buttons["workspace-tab-projects"].tap()
        XCTAssertTrue(card.waitForExistence(timeout: 8), "重启后 Workspace 仍应存在")
        XCTAssertTrue(app.staticTexts["移动端体验"].exists, "重启后名称必须保留")
        settings.tap()
        app.buttons["workspace-toggle-archive"].tap()
        XCTAssertEqual(app.buttons["workspace-toggle-archive"].label, "重开工作空间", "归档成功后应立即提供重开")
        app.buttons["workspace-settings-back"].tap()
        XCTAssertFalse(card.exists, "活动列表不应继续显示已归档 Workspace")
        app.buttons["workspace-archive-filter"].tap()
        app.buttons["已归档"].tap()
        XCTAssertTrue(card.waitForExistence(timeout: 8), "已归档列表必须保留同一 Workspace")
        settings.tap()
        app.buttons["workspace-toggle-archive"].tap()
        XCTAssertEqual(app.buttons["workspace-toggle-archive"].label, "归档工作空间", "重开后应恢复为活动状态")
    }
}
