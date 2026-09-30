import XCTest

final class PairingFlowTests: XCTestCase {
    /// 输入页带着键盘关闭或切后台后，主页底栏不继承键盘的安全区残留。
    /// 数据构造：未配对模拟器，地址127.0.0.1仅本地输入；按钮底距=8pt外距+4pt轨道内距=12pt。
    /// 执行过程：打开嵌套验证码Sheet → 唤起键盘 → 直接返回或Home恢复 → 关闭Sheet → 不切Tab先测底栏。
    /// 预期结果：键盘及输入页完全退出，原项目仍被选中；底栏距屏幕12pt，无重复系统TabBar。
    @MainActor func testBottomNavigationAfterKeyboardExit() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launch()
        for background in [false, true] {
            // 5s是原生导航预算；3s容纳系统键盘动画，2pt仅容纳像素取整。
            XCTAssertTrue(app.buttons["manage-computers"].waitForExistence(timeout: 5), "需要公共电脑入口")
            app.buttons["manage-computers"].tap(); app.buttons["add-computer"].tap()
            app.buttons["enter-pairing-code"].tap()
            let address = app.textFields["pairing-address-input"]
            XCTAssertTrue(address.waitForExistence(timeout: 5), "必须进入实际输入页")
            address.tap(); address.typeText("127.0.0.1")
            XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3), "必须实际唤起键盘，不能用未编辑页面替代")
            if background { XCUIDevice.shared.press(.home); app.activate() }
            app.buttons["pairing-code-close"].tap(); app.buttons["pairing-cancel"].tap()
            app.buttons["computers-close"].tap()
            XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "退出输入流程后键盘必须消失")
            let tab = app.buttons["workspace-tab-projects"]
            XCTAssertTrue(tab.waitForExistence(timeout: 5), "返回项目必须恢复原底栏")
            XCTAssertTrue(tab.isSelected, "退出输入页不得切换原选中项目")
            let gap = app.frame.maxY - tab.frame.maxY
            print("KEYBOARD_EXIT background=\(background) gap=\(gap)")
            XCTAssertEqual(gap, 12, accuracy: 2, "带键盘退出后不能残留旧安全区，且不能靠切换Tab修复")
            XCTAssertEqual(app.tabBars.count, 0, "不能恢复出额外系统TabBar")
            XCTAssertFalse(address.exists, "已关闭的输入页不能继续占用布局")
        }
    }

    /// 底栏从后台恢复及切换Tab后，仍固定在屏幕底边8pt，而非重新叠加系统安全区。
    /// 数据构造：3个共享主Tab；玻璃轨道内边距4pt，所以按钮底边距屏幕为8+4=12pt；2pt容纳像素取整。
    /// 执行过程：逐页点击 → 记录位置 → Home进入后台/activate恢复 → 不点击先核验；显式fixture同时验证项目恢复。
    /// 预期结果：恢复前后间距均为12pt；不出现额外34pt安全区，也不因切页产生第二套底栏。
    @MainActor func testBottomNavigationSurvivesBackground() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launch()
        let paired = ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui"
        if paired {
            XCTAssertTrue(app.buttons["agent-card-1"].waitForExistence(timeout: 8), "显式fixture必须先完成配对并显示项目")
            XCTAssertEqual(app.buttons["project-switcher"].label, "通知验收项目", "只在隔离项目中执行验收")
        }
        for id in ["projects", "command", "tasks"] {
            let button = app.buttons["workspace-tab-" + id]
            // 5s覆盖原生启动/恢复及布局动画，不代表接口响应预算。
            XCTAssertTrue(button.waitForExistence(timeout: 5), "\(id)入口必须存在")
            button.tap()
            for cycle in 0..<2 {
                XCTAssertTrue(button.isSelected, "第\(cycle)轮\(id)必须保留选中态")
                let gap = app.frame.maxY - button.frame.maxY
                print("BOTTOM_NAV cycle=\(cycle) tab=\(id) gap=\(gap)")
                XCTAssertEqual(gap, 12, accuracy: 2, "第\(cycle)轮\(id)不能恢复为系统安全区之上的旧位置")
                XCTAssertEqual(app.tabBars.count, 0, "恢复后不能出现系统TabBar与自定义底栏重叠")
                if cycle == 0 {
                    XCUIDevice.shared.press(.home); app.activate()
                    XCTAssertTrue(button.waitForExistence(timeout: 5), "恢复后无需再次点击就应显示原底栏")
                    if paired && id == "projects" {
                        // 8s = 2s快照心跳 + 6s原生导航余量；必须走目录清空再恢复的真实链路。
                        XCTAssertTrue(app.buttons["agent-card-1"].waitForExistence(timeout: 8), "后台恢复后必须重新读到隔离项目卡片")
                    }
                }
            }
        }
    }

    /// 原型主导航保持项目/指挥台/任务，电脑管理从公共入口打开且关闭后返回原Tab。
    ///
    /// 数据构造：独立空模拟器；3个主Tab；5s容纳启动和系统导航，不代表网络延迟。
    /// 执行过程：核对Tab顺序 → 进入任务 → 管理电脑 → 关闭 → 检查仍在任务页。
    /// 预期结果：指挥台位于中间，任务可达；电脑不能占用主Tab，空任务不虚构Agent执行。
    @MainActor func testPrimaryNavigationMatchesPrototype() {
        let app = XCUIApplication(); app.launch()
        XCTAssertTrue(app.buttons["workspace-tab-tasks"].waitForExistence(timeout: 5), "第三个主Tab必须是任务")
        XCTAssertFalse(app.tabBars.buttons["电脑"].exists, "电脑管理不能替代任务Tab")
        let projects = app.buttons["workspace-tab-projects"]
        let command = app.buttons["workspace-tab-command"]
        let tasks = app.buttons["workspace-tab-tasks"]
        XCTAssertEqual(projects.label, "项目", "项目入口必须保留原生可访问名称")
        XCTAssertEqual(command.label, "指挥台", "指挥台入口必须保留原生可访问名称")
        XCTAssertLessThan(projects.frame.midX, command.frame.midX, "项目应位于左侧")
        XCTAssertLessThan(command.frame.midX, tasks.frame.midX, "指挥台应位于正中间")
        app.buttons["workspace-tab-tasks"].tap()
        XCTAssertTrue(app.staticTexts["连接电脑后查看任务"].waitForExistence(timeout: 5), "空态不能伪造任务")
        app.buttons["manage-computers"].tap()
        XCTAssertTrue(app.buttons["add-computer"].waitForExistence(timeout: 5), "公共入口必须保留配对能力")
        app.buttons["computers-close"].tap()
        XCTAssertTrue(app.buttons["workspace-tab-tasks"].isSelected, "管理电脑返回后必须保留任务导航")
        XCTAssertFalse(app.buttons["computers-close"].exists, "电脑管理Sheet应完全关闭")
    }

    /// 通知设置在无配对时可达，进入页面不自动申请系统授权或伪造订阅。
    ///
    /// 数据构造：独立空数据模拟器；5s=启动和原生导航的最大等待预算。
    /// 执行过程：电脑页 → 通知设置 → 读取未连接说明 → 关闭。
    /// 预期结果：无连接不能启用任务提醒，系统授权弹窗不能由浏览页面触发。
    @MainActor func testNotificationSettingsRequiresAnExplicitOptIn() {
        let app = XCUIApplication(); app.launch()
        XCTAssertTrue(app.buttons["manage-computers"].waitForExistence(timeout: 5), "需要公共电脑入口")
        app.buttons["manage-computers"].tap()
        app.buttons["computer-more-actions"].tap()
        app.buttons["notification-settings"].tap()
        XCTAssertTrue(app.staticTexts["连接电脑后开启任务提醒"].waitForExistence(timeout: 5), "应说明按电脑订阅")
        XCTAssertFalse(app.buttons["enable-notifications"].exists, "未配对不得触发系统授权")
        XCTAssertFalse(app.staticTexts["任务提醒已开启"].exists, "空状态不能伪造订阅成功")
        app.buttons["notification-settings-close"].tap()
        XCTAssertTrue(app.buttons["add-computer"].waitForExistence(timeout: 5), "关闭设置回到原电脑页")
    }

    /// 指挥台居中，跨电脑审批和本机决定记录在未连接时仍可访问。
    ///
    /// 数据构造：无配对记录的原生 App；5s 为启动与页面转换的等待上限。
    /// 执行过程：点击中间 Tab → 切换待你决策 → 显示全部电脑范围 → 查看决定记录 → 关闭。
    /// 预期结果：无连接显示明确空态；不能伪造待审批或工具执行成功，也不能依赖当前选中电脑。
    @MainActor func testCommandCenterAndDecisionRecords() {
        let app = XCUIApplication(); app.launch()
        let command = app.buttons["workspace-tab-command"]
        XCTAssertTrue(command.waitForExistence(timeout: 5), "中间 Tab 必须保留指挥台")
        command.tap()
        let decisions = app.buttons["command-decision-inbox"]
        XCTAssertTrue(decisions.waitForExistence(timeout: 5), "决策应由原生指挥台提醒进入")
        decisions.tap()
        XCTAssertTrue(app.navigationBars["待决定 · 全部电脑"].waitForExistence(timeout: 5), "指挥台范围不能由项目电脑选择器决定")
        XCTAssertTrue(app.staticTexts["先连接你的电脑"].exists, "没有配对时必须说明前置条件")
        app.buttons["approval-records"].tap()
        XCTAssertTrue(app.staticTexts["没有决定记录"].waitForExistence(timeout: 5), "离线时也能进入本机记录")
        XCTAssertFalse(app.staticTexts["工具执行成功"].exists, "没有真实执行证据不能伪造成功")
        app.buttons["approval-records-close"].tap()
        XCTAssertTrue(app.buttons["approval-records"].waitForExistence(timeout: 5), "关闭后返回指挥台")
        XCTAssertFalse(app.staticTexts["没有决定记录"].exists, "Sheet 不应残留")
    }

    /// 指挥台默认显示原生陪伴入口；未连接时不伪造项目或Todo。
    ///
    /// 数据构造：无配对的App；5s=启动和原生导航动画的等待上限。
    /// 执行过程：进入指挥台 → 检查未绑定状态 → 打开项目菜单。
    /// 预期结果：开聊入口独立于Mac；没有项目时无计划入口，也无虚构任务。
    @MainActor func testProjectPlansEntryWithoutPairedComputer() {
        let app = XCUIApplication(); app.launch()
        let command = app.buttons["workspace-tab-command"]
        XCTAssertTrue(command.waitForExistence(timeout: 5), "需要原生指挥台")
        command.tap()
        let project = app.buttons["realtime-project"]
        XCTAssertTrue(project.waitForExistence(timeout: 5), "原生指挥台需显示通话绑定状态")
        XCTAssertEqual(project.value as? String, "未绑定项目", "未连接电脑不能伪造项目")
        XCTAssertTrue(app.buttons["realtime-call-start"].isHittable, "语音入口不能依赖 Mac 在线")
        XCTAssertFalse(app.buttons["generate-project-plan"].exists, "无项目不得发起计划生成")
        XCTAssertTrue(app.buttons["command-decision-inbox"].exists, "离线仍有全部电脑决定入口")
        project.tap()
        XCTAssertFalse(app.buttons["command-project-plans"].exists, "没有项目不能打开错误项目的计划")
    }

    /// 未连接电脑时也能打开本机队列记录，关闭后回到电脑页。
    ///
    /// 数据构造：没有队列的模拟器 App；5s 只容纳启动动画和系统调度。
    /// 执行过程：电脑 Tab → 消息队列 → 空态 → 关闭。
    /// 预期结果：入口独立于授权项目；关闭后不残留 Sheet，不凭空显示已发送。
    @MainActor func testLocalQueueRecordsRemainReachable() {
        let app = XCUIApplication(); app.launch()
        let devices = app.buttons["manage-computers"]
        XCTAssertTrue(devices.waitForExistence(timeout: 5), "需要原生电脑页")
        devices.tap()
        app.buttons["computer-more-actions"].tap()
        let queue = app.buttons["local-message-queue"]
        XCTAssertTrue(queue.waitForExistence(timeout: 5), "断线时也须能找回消息原文")
        queue.tap()
        XCTAssertTrue(app.staticTexts["没有排队消息"].waitForExistence(timeout: 5), "无记录时显示真实空态")
        XCTAssertFalse(app.staticTexts["Agent 已收到"].exists, "没有消息不能伪造成功")
        app.buttons["queue-close"].tap()
        XCTAssertTrue(app.buttons["add-computer"].waitForExistence(timeout: 5), "关闭后回到电脑页")
        XCTAssertFalse(app.staticTexts["没有排队消息"].exists, "队列 Sheet 应已关闭")
    }

    /// 原生配对支持六位验证码与扫码；无效地址和无相机均不能伪造配对成功。
    ///
    /// 数据构造：全新安装的模拟器 App，无电脑记录；5s 等待仅容纳启动动画和系统调度。
    /// 执行过程：电脑tab → 添加电脑 → 输入六位码和无效地址并拒绝 → 返回 → 相机/图片入口 → 取消 → 本地草稿 → 关闭。
    /// 预期结果：扫码不可用提示可见；取消不伪造配对；本地草稿空态可访问，关闭后保留电脑列表。
    @MainActor func testNativePairingGuideAndScannerFallback() {
        let app = XCUIApplication()
        app.launch()
        let devices = app.buttons["manage-computers"]
        XCTAssertTrue(devices.waitForExistence(timeout: 5), "公共导航必须有电脑入口")
        devices.tap()
        app.buttons["add-computer"].tap()
        XCTAssertTrue(app.staticTexts["连接你的电脑"].waitForExistence(timeout: 5), "必须先说明在哪里生成配对信息")
        app.buttons["enter-pairing-code"].tap()
        let input = app.textFields["pairing-code-input"]
        let address = app.textFields["pairing-address-input"]
        XCTAssertTrue(input.waitForExistence(timeout: 5), "不使用相机也能输入六位验证码")
        XCTAssertTrue(address.exists, "首次连接必须有明确电脑地址")
        input.tap(); input.typeText("012345")
        XCTAssertFalse(app.buttons["verify-pairing-code"].isEnabled, "无地址不能猜测目标或发送验证码")
        address.tap(); address.typeText("ws://192.168.1.2:9443")
        app.buttons["verify-pairing-code"].tap()
        XCTAssertTrue(app.staticTexts["请输入电脑上显示的局域网或 Tailscale 地址。"].waitForExistence(timeout: 5), "协议降级必须在网络发送前拒绝")
        XCTAssertFalse(app.staticTexts["等待电脑确认"].exists, "无效地址不得产生配对状态")
        app.buttons["pairing-code-close"].tap()
        app.buttons["scan-qr"].tap()
        XCTAssertTrue(app.staticTexts["此设备暂不可扫码"].waitForExistence(timeout: 5), "模拟器不能伪造相机扫描结果")
        XCTAssertTrue(app.buttons["scan-qr-photo"].exists, "无相机时仍须允许选择真实二维码图片")
        app.buttons["scanner-close"].tap()
        app.buttons["pairing-cancel"].tap()
        XCTAssertTrue(app.buttons["add-computer"].waitForExistence(timeout: 5), "取消必须回到原页面")
        XCTAssertFalse(app.staticTexts["核对码"].exists, "取消后不保留未完成配对状态")
        app.buttons["computer-more-actions"].tap()
        app.buttons["local-drafts"].tap()
        XCTAssertTrue(app.staticTexts["没有未发送的草稿"].waitForExistence(timeout: 5), "尚未连接电脑也能访问本地草稿")
        app.buttons["drafts-close"].tap()
        XCTAssertTrue(app.buttons["add-computer"].waitForExistence(timeout: 5), "草稿页关闭不能丢失原导航")
        XCTAssertFalse(app.staticTexts["没有未发送的草稿"].exists, "关闭后不应残留草稿Sheet")
    }
}


extension PairingFlowTests {
    /// 地址与数字键盘均可点击外部收起，切换输入框不能丢失内容或中断编辑。
    ///
    /// 数据构造：独立模拟器，测试地址127.0.0.1与六位数字012345；不验证电脑、不发送网络请求。
    /// 执行过程：地址输入 → 点击页标题收起 → 切到验证码 → 重点输入框 → 点击外部 → 使用收起按钮。
    /// 预期结果：外部点击后键盘消失；点击输入框时保持编辑；两个值完整保留，验证从未触发。
    @MainActor func testPairingKeyboardDismissalPreservesBothFields() {
        let app = XCUIApplication(); app.launch()
        // 5s覆盖启动和导航，3s=键盘系统动画与查询调度预算，不是网络等待。
        XCTAssertTrue(app.buttons["manage-computers"].waitForExistence(timeout: 5), "需要电脑管理入口")
        app.buttons["manage-computers"].tap(); app.buttons["add-computer"].tap()
        app.buttons["enter-pairing-code"].tap()
        let address = app.textFields["pairing-address-input"], code = app.textFields["pairing-code-input"]
        XCTAssertTrue(address.waitForExistence(timeout: 5), "地址输入必须可编辑")
        address.tap()
        if let old = address.value as? String, old != address.placeholderValue {
            address.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: old.count))
        }
        address.typeText("127.0.0.1")
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3), "地址编辑需显示真实键盘")
        app.navigationBars["输入验证码"].staticTexts["输入验证码"].tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "点击输入框外的标题必须收起键盘")
        guard !app.keyboards.firstMatch.exists else { return }
        XCTAssertEqual(address.value as? String, "127.0.0.1", "收起不应更改地址")
        address.tap(); code.tap(); code.typeText("012345")
        code.tap()
        XCTAssertTrue(app.keyboards.firstMatch.exists, "切换或点击输入框不得把刚打开的键盘关闭")
        app.staticTexts["验证码"].tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "数字键盘也应支持外部轻点收起")
        XCTAssertEqual(code.value as? String, "012345", "收起不能清空验证码或去掉前导零")
        code.tap()
        let dismiss = app.buttons["pairing-dismiss-keyboard"]
        XCTAssertTrue(dismiss.waitForExistence(timeout: 3), "没有Return键的数字键盘必须有明确收起入口")
        dismiss.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 3), "显式收起应立即生效")
        XCTAssertEqual(address.value as? String, "127.0.0.1", "字段切换与收起必须保留地址")
        XCTAssertFalse(app.staticTexts["正在验证…"].exists, "收起键盘不得触发验证")
        app.buttons["pairing-code-close"].tap(); app.buttons["pairing-cancel"].tap()
    }
}
