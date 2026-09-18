import XCTest

final class CardCreationFlowTests: XCTestCase {
    /// 卡片选项始终可打开；只读解释权限，无关未决消息不影响另一张空卡。
    /// 数据构造：原卡1为Codex完成任务且有1条未发送的持久请求，原卡2为Claude空卡；初始只读。
    /// 执行过程：查看卡1 → 查看卡2 → 升级control → 打开原生删除确认并取消 → 安排任务打开原卡聊天。
    /// 预期结果：同卡先核对，无关卡可删除；打开和取消不发送指令，安排任务仅进入原卡输入框。
    @MainActor func testCardOptionsStayAvailableAndTargetOriginalCard() throws {
        try awaitCardOptions(accessibility: false)
    }

    /// 最大辅助字号的选项可滚动且窗口变更关闭旧面板。
    /// 数据构造：同上2张卡及AX5字号；8s = 2s心跳 + 6s导航余量，最多6次滚动覆盖全高面板。
    /// 执行过程：复用权限、隔离和取消链路 → 校验按钮边界 → 替换项目窗口。
    /// 预期结果：文本不横向裁切、操作可触达；旧窗口选项关闭且没有创建或删除副作用。
    @MainActor func testAccessibleCardOptionsAndWindowChange() throws {
        try awaitCardOptions(accessibility: true)
    }

    @MainActor private func awaitCardOptions(accessibility: Bool) throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else { throw XCTSkip("仅隔离卡片fixture") }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-card-creation").path) else { throw XCTSkip("必须启用隔离卡片验收") }
        let app = XCUIApplication()
        if accessibility { app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
        app.launch(); app.buttons["workspace-tab-projects"].tap()
        let first = app.buttons["card-options-1"], second = app.buttons["card-options-2"]
        XCTAssertTrue(first.waitForExistence(timeout: 8), "只读或同卡未决都不能隐藏选项入口")
        guard first.exists else { return }
        first.tap()
        XCTAssertTrue(app.buttons["card-open-task"].waitForExistence(timeout: 3), "完成卡应显示查看任务")
        XCTAssertTrue(app.buttons["card-check-result"].exists, "同卡未决必须有查询入口")
        XCTAssertFalse(app.buttons["card-delete"].isEnabled, "同卡未决或只读不能删除")
        app.buttons["card-options-close"].tap()
        for _ in 0..<6 where !second.isHittable { app.swipeUp() }
        second.tap()
        let open = app.buttons["card-open-task"], delete = app.buttons["card-delete"]
        XCTAssertTrue(open.waitForExistence(timeout: 3), "空卡必须提供安排任务入口")
        XCTAssertTrue(open.label.contains("安排任务"), "空卡不伪造已有任务")
        XCTAssertFalse(app.buttons["card-check-result"].exists, "卡1的未决消息不能冒充卡2操作")
        XCTAssertFalse(delete.isEnabled, "只读不能删除卡2")
        try Data().write(to: dir.appendingPathComponent("ui-card-management"))
        XCTAssertTrue(open.waitForNonExistence(timeout: 8), "授权变化须关闭旧授权面板")
        second.tap()
        XCTAssertTrue(delete.waitForExistence(timeout: 8), "新授权下能重开选项")
        XCTAssertTrue(delete.isEnabled, "卡1的未决消息不能锁住卡2")
        for _ in 0..<6 where !delete.isHittable { app.scrollViews.firstMatch.swipeUp() }
        XCTAssertTrue(delete.isHittable, "删除入口必须可触达")
        XCTAssertLessThanOrEqual(delete.frame.maxX, app.frame.maxX, "按钮不得横向裁切")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = accessibility ? "card-options-AX5" : "card-options"; shot.lifetime = .keepAlways; add(shot)
        delete.tap()
        XCTAssertTrue(app.buttons["confirm-delete-card"].waitForExistence(timeout: 3), "删除应直接显示原生确认弹窗")
        XCTAssertFalse(app.navigationBars["删除卡片"].exists, "不能再进入整页删除表单")
        let confirm = app.buttons["confirm-delete-card"]
        XCTAssertTrue(confirm.isHittable, "确认操作应直接可触达，无需滚动电脑和项目信息")
        XCTAssertTrue(confirm.exists, "必须进入卡2删除确认，不能误进卡1消息结果")
        XCTAssertFalse(app.buttons["查询结果"].exists, "删除确认不得混入其他卡请求")
        app.buttons["取消"].tap()
        XCTAssertTrue(open.waitForExistence(timeout: 3), "取消应返回卡片选项")
        if accessibility {
            let stale = dir.appendingPathComponent("ui-stale")
            defer { try? FileManager.default.removeItem(at: stale) }
            try Data().write(to: stale)
            XCTAssertTrue(open.waitForNonExistence(timeout: 8), "项目窗口替换后不能继续操作旧选项")
        } else {
            open.tap()
            XCTAssertTrue(app.descendants(matching: .any)["chat-message-input"].firstMatch.waitForExistence(timeout: 8), "安排任务必须进入已有聊天输入框")
            XCTAssertTrue(app.navigationBars["空白验收卡片"].exists, "必须打开卡2，不能跳到另一个Agent")
            XCTAssertFalse(app.buttons["card-options-close"].exists, "聊天应进入完整页面，不堆叠在选项面板中")
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-created-cards.json").path), "查看、取消和进入聊天都不能创建卡片或派任务")
    }

    /// 创建弹层可随机选择项目 Agent，也保留手动类型和名称；无关未决消息不锁住创建。
    /// 数据构造：隔离只读项目、1个未打开的Codex「界面助手」及1条仅保存在手机的未决消息；空卡投影101/102；第3次服务拒绝。
    ///   8s = 2s订阅心跳 + 6s原生导航余量；最多4次滚动覆盖新增卡片与创建入口。
    /// 执行过程：只读打开弹层 → 升级 → 随机选择项目Agent创建 → 底部手动Cursor创建 → 投影为运行中并打开 → 拒绝 → 查询清理 → 重开 → 确认删除101并自动回列表。
    /// 预期结果：未授权不提交，随机选择不提前创建；首卡绑定definition_id=7，手动卡不绑定；运行中Cursor隐藏停止动作；未决创建不能重发；删除101自动返回，102保留。
    @MainActor func testCreateCardFormAndUnrelatedPendingMessage() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else { throw XCTSkip("仅隔离卡片fixture") }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-card-creation").path) else { throw XCTSkip("必须显式启用创建验收") }
        let app = XCUIApplication(); app.launch(); app.buttons["workspace-tab-projects"].tap()
        let entry = app.buttons["create-card-toolbar"]
        XCTAssertTrue(entry.waitForExistence(timeout: 8), "项目顶部应有创建入口")
        XCTAssertTrue(entry.isEnabled, "只读时也应能打开弹层查看原因，不能留下不可点的入口")
        guard entry.isEnabled else { return }
        entry.tap()
        let title = app.textFields["new-card-title"], submit = app.buttons["create-empty-card"]
        XCTAssertTrue(title.waitForExistence(timeout: 3), "加号应呈现原型的可选名称表单")
        XCTAssertFalse(submit.isEnabled, "只读不能创建")
        XCTAssertTrue(app.staticTexts["当前项目为只读，请在电脑上允许控制后创建。"].exists, "禁用必须说明原因")
        app.buttons["new-card-close"].tap()
        try Data().write(to: dir.appendingPathComponent("ui-card-management"))
        entry.tap()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"), object: submit)], timeout: 8), .completed, "正式授权更新后，无关待确认消息不得阻止创建")
        XCTAssertTrue(app.buttons["new-card-project-agent"].exists, "项目定义必须出现在原生选择菜单")
        let random = app.buttons["new-card-random-agent"]
        XCTAssertTrue(random.exists, "有可用项目定义时必须提供随机选取")
        random.tap()
        XCTAssertEqual(title.value as? String, "界面助手", "单一候选的随机结果必须准确回填名称")
        XCTAssertTrue(app.staticTexts["new-card-selected-definition"].exists, "选中后必须清楚说明将绑定项目 Agent")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "create-card-form"; shot.lifetime = .keepAlways; add(shot)
        XCTAssertFalse(app.buttons["agent-card-101"].exists, "随机选择和回填名称都不能提前创建")
        submit.tap()
        let first = app.buttons["agent-card-101"]
        XCTAssertTrue(first.waitForExistence(timeout: 8), "明确创建后应回到看板")
        XCTAssertTrue(first.label.contains("界面助手"), "自定义名称必须显示在原卡")
        XCTAssertTrue(first.label.contains("Codex"), "默认类型为Codex")
        XCTAssertTrue(first.label.contains("未安排任务"), "创建不能自动安排任务")
        let bottom = app.buttons["create-card-entry"]
        for _ in 0..<4 where !bottom.isHittable { app.swipeUp() }
        bottom.tap(); app.buttons["new-card-agent"].tap()
        let cursor = app.buttons["create-agent-Cursor"]
        XCTAssertTrue(cursor.waitForExistence(timeout: 3), "手动类型菜单必须提供Cursor")
        cursor.tap()
        XCTAssertTrue(submit.exists, "选类型后仍应等待明确创建")
        XCTAssertNotEqual(title.value as? String, "界面助手", "新卡不能沿用上次名称；空字段的占位示例不算已填值")
        submit.tap()
        let second = app.buttons["agent-card-102"]
        XCTAssertTrue(second.waitForExistence(timeout: 8), "底部入口也应创建成功")
        XCTAssertTrue(second.label.contains("Cursor"), "第二张必须保留Cursor类型")
        XCTAssertTrue(second.label.contains("未安排任务"), "Cursor也是空卡")
        let requests = try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("ui-created-cards.json"))) as? [[String: Any]]
        XCTAssertEqual(requests?.count, 2, "每次确认只能创建一张")
        XCTAssertEqual(requests?.first?["title"] as? String, "界面助手", "不能丢失可选名称")
        XCTAssertEqual(requests?.first?["definition_id"] as? Int, 7, "随机选择必须发送稳定项目定义，不得只发送同名标题")
        XCTAssertTrue(requests?.last?["title"] is NSNull, "留空让电脑分配默认名称")
        XCTAssertTrue(requests?.last?["definition_id"] is NSNull, "手动新建必须保留原有的无定义语义")
        XCTAssertEqual(requests?.last?["profile"] as? String, "Cursor", "创建请求不能把Cursor降级为Codex或Claude")
        let cursorBusy = dir.appendingPathComponent("ui-cursor-busy")
        defer { try? FileManager.default.removeItem(at: cursorBusy) }
        try Data().write(to: cursorBusy)
        let busy = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label CONTAINS %@", "思考中"), object: second
        )
        XCTAssertEqual(XCTWaiter.wait(for: [busy], timeout: 8), .completed, "fixture必须把Cursor投影为运行中，确保验证的是能力隐藏而非空闲隐藏")
        second.tap()
        XCTAssertTrue(app.descendants(matching: .any)["chat-message-input"].firstMatch.waitForExistence(timeout: 8), "运行中Cursor必须能打开对话")
        XCTAssertFalse(app.buttons["chat-stop-task"].exists, "Cursor没有可靠停止协议时必须隐藏停止动作，不能只禁用")
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(entry.waitForExistence(timeout: 8), "关闭Cursor对话后必须回到原项目")
        try Data().write(to: dir.appendingPathComponent("ui-create-reject"))
        for _ in 0..<4 where !entry.isHittable { app.swipeDown() }
        entry.tap(); submit.tap()
        XCTAssertTrue(app.staticTexts["电脑未创建卡片"].waitForExistence(timeout: 8), "失败需要准确反馈")
        XCTAssertFalse(submit.isEnabled, "未核对创建结果不能重发")
        XCTAssertFalse(app.buttons["agent-card-103"].exists, "拒绝不能产生第三张卡")
        app.buttons["new-card-check-result"].tap()
        XCTAssertTrue(app.staticTexts["项目窗口已变化，未执行操作"].waitForExistence(timeout: 8), "必须查询创建本身，不能查询那条无关消息")
        app.buttons["完成"].tap()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"), object: submit)], timeout: 8), .completed, "清理已知创建结果后允许重新填写")
        app.buttons["new-card-close"].tap()
        let finalRequests = try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("ui-created-cards.json"))) as? [[String: Any]]
        XCTAssertEqual(finalRequests?.count, 2, "查询和关闭不能自动重试")
        let original = app.buttons["card-options-1"]
        for _ in 0..<4 where !original.isHittable { app.swipeDown() }
        original.tap()
        XCTAssertTrue(app.buttons["查询操作结果"].waitForExistence(timeout: 8), "移除脉络区不能丢失原卡片中的未确认消息")
        app.buttons["card-options-close"].tap()
        let firstOptions = app.buttons["card-options-101"]
        for _ in 0..<4 where !firstOptions.isHittable { app.swipeUp() }
        firstOptions.tap(); app.buttons["card-delete"].tap()
        let delete = app.buttons["confirm-delete-card"]
        XCTAssertTrue(delete.waitForExistence(timeout: 3), "已创建空卡应直接显示删除确认")
        delete.tap()
        XCTAssertTrue(first.waitForNonExistence(timeout: 8), "删除成功应自动回到列表并移除目标卡")
        XCTAssertFalse(app.buttons["card-options-close"].exists, "删除成功不能留在选项或结果页等待完成")
        XCTAssertTrue(second.exists, "删除第一张卡不能影响第二张卡")
        XCTAssertEqual(try String(contentsOf: dir.appendingPathComponent("ui-deleted-card"), encoding: .utf8), "101", "只能提交目标卡删除")
    }
    /// 辅助字号下创建表单可滚动、字段与按钮不溢出；名称校验不能被控制授权绕过。
    /// 数据构造：AX5字体、41个ASCII字符 = 原型40字符上限 + 1；只读升control；旧窗口替换一次。
    /// 执行过程：打开表单 → 填超长名称 → 收键盘 → 升级 → 滚动核对校验 → 切换窗口。
    /// 预期结果：禁用创建且原因可读；按钮没有横向裁切；旧窗口表单关闭，不向新窗口提交旧名称。
    @MainActor func testAccessibleCreationValidationAndWindowChange() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else { throw XCTSkip("仅隔离卡片fixture") }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-card-creation").path) else { throw XCTSkip("必须显式启用创建验收") }
        let app = XCUIApplication()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"]
        app.launch(); app.buttons["workspace-tab-projects"].tap()
        app.buttons["create-card-toolbar"].tap()
        let title = app.textFields["new-card-title"], submit = app.buttons["create-empty-card"]
        XCTAssertTrue(title.waitForExistence(timeout: 8), "大字号也须呈现名称输入")
        for _ in 0..<6 where !title.isHittable { app.scrollViews.firstMatch.swipeUp() }
        title.tap(); title.typeText(String(repeating: "A", count: 41))
        app.buttons["new-card-dismiss-keyboard"].tap()
        try Data().write(to: dir.appendingPathComponent("ui-card-management"))
        let invalid = app.staticTexts["名称最多 40 个字符，不能包含换行或控制字符。"]
        XCTAssertTrue(invalid.waitForExistence(timeout: 8), "不静默截断名称，明确显示超限原因")
        for _ in 0..<6 where !submit.isHittable { app.scrollViews.firstMatch.swipeUp() }
        XCTAssertTrue(submit.isHittable, "最大字号也能滚动到主按钮")
        XCTAssertFalse(submit.isEnabled, "控制授权不能绕过名称上限")
        XCTAssertLessThanOrEqual(submit.frame.maxX, app.frame.maxX, "主按钮不得横向溢出")
        XCTAssertTrue(invalid.isHittable, "校验说明与按钮应一起可读")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "create-card-accessibility"; shot.lifetime = .keepAlways; add(shot)
        let stale = dir.appendingPathComponent("ui-stale")
        defer { try? FileManager.default.removeItem(at: stale) }
        try Data().write(to: stale)
        XCTAssertTrue(title.waitForNonExistence(timeout: 8), "原窗口变化时必须关闭旧创建表单")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-created-cards.json").path), "校验或窗口切换都不能创建卡片")
    }

}
