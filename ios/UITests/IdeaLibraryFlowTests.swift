import XCTest

final class IdeaLibraryFlowTests: XCTestCase {
    /// 首次进入 App 直接展示浅色想法库，不需要先连接电脑。
    ///
    /// 数据构造：1 个独立想法库命名空间，初始想法 0 条；8 秒 = 首启 3 秒 + 本地读取与系统调度余量 5 秒。
    /// 执行过程：冷启动 → 查看想法库和三项导航 → 打开更多模块 → 关闭抽屉。
    /// 预期结果：记录入口可用；旧项目/指挥台/任务底栏与强制连接首页不存在；抽屉可返回。
    @MainActor func testAppLaunchesIntoIdeaLibrary() {
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_IDEA_TEST_NAMESPACE"] = UUID().uuidString
        app.launch()
        XCTAssertTrue(app.buttons["record-idea"].waitForExistence(timeout: 8), "启动首页应直接进入可记录的想法库")
        XCTAssertTrue(app.staticTexts["先记录一个想法"].exists, "空库应说明如何记录想法")
        XCTAssertFalse(app.staticTexts["先连接你的电脑"].exists, "连接电脑不能阻挡本地想法首页")
        XCTAssertEqual(app.buttons["workspace-tab-projects"].label, "工作空间", "首项导航应使用新产品名称")
        XCTAssertEqual(app.buttons["workspace-tab-command"].label, "Norma", "中间导航应使用 Norma")
        XCTAssertEqual(app.buttons["workspace-tab-tasks"].label, "收件箱", "第三项导航应使用收件箱")
        let home = XCTAttachment(screenshot: app.screenshot())
        home.name = "P01-new-start-home"
        home.lifetime = .keepAlways
        add(home)
        app.buttons["idea-more-modules"].tap()
        XCTAssertTrue(app.buttons["已连接电脑"].waitForExistence(timeout: 8), "设备连接应从更多模块进入")
        XCTAssertFalse(app.buttons["Norma 文字对话"].exists, "更多模块中不能出现独立文字聊天产品")
        app.buttons["完成"].tap()
        XCTAssertTrue(app.buttons["record-idea"].isHittable, "关闭抽屉后应返回可操作的想法库")
    }

    /// 原生语音页录音保存后，重启仍能播放原件并提供待转写重试入口。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   独立测试命名空间 1 个；录音持续约 3 秒，足以产生非空 CAF 并检查播放中的状态切换。
    ///   8 秒等待 = 页面转场约 3 秒 + 模拟器音频与 SQLite 写入余量 5 秒。
    ///   自然播放最多等 15 秒 = 约 3 秒主动录音 + XCTest 截图/事件延迟约 7 秒 + 播放调度余量 5 秒。
    ///
    /// 执行过程：
    ///   1. 打开语音页，明确点录音，处理系统权限后停止并保存。
    ///   2. 打开语音想法检查播放和重试入口，随后终止并重启 App。
    ///   3. 再次打开同一想法，播放到结尾并检查控件自动回到初始状态。
    ///
    /// 预期结果：只保存一条可播放的待转写语音；重试入口可见；停止和自然结束都恢复播放文案；重启未丢录音。
    @MainActor func testVoiceRecordingCanBeSavedAndReopenedForRetry() throws {
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_IDEA_TEST_NAMESPACE"] = UUID().uuidString
        app.launch()
        openIdeas(app)
        app.buttons["record-idea"].tap()
        app.buttons["record-voice-idea"].firstMatch.tap()
        let control = app.buttons["idea-voice-record-control"]
        XCTAssertTrue(control.waitForExistence(timeout: 8), "必须有真正可操作的录音按钮")
        control.tap()
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        for _ in 0..<2 {
            let alert = springboard.alerts.firstMatch
            guard alert.waitForExistence(timeout: 2) else { break }
            let allow = ["Allow", "OK", "允许", "好", "Allow While Using App"]
                .compactMap { title in alert.buttons[title].exists ? alert.buttons[title] : nil }.first
            try XCTUnwrap(allow, "系统权限提示必须有可选择的允许操作").tap()
        }
        XCTAssertTrue(app.staticTexts["正在转写"].waitForExistence(timeout: 8), "授权后应进入真实录音状态")
        let recording = expectation(description: "录音产生音频帧")
        DispatchQueue.main.asyncAfter(deadline: .now() + 3) { recording.fulfill() }
        wait(for: [recording], timeout: 5)
        let recordingShot = XCTAttachment(screenshot: app.screenshot())
        recordingShot.name = "P02-voice-idea-recording"
        recordingShot.lifetime = .keepAlways
        add(recordingShot)
        control.tap()
        let transcript = app.textViews["idea-voice-transcript"]
        XCTAssertTrue(transcript.waitForExistence(timeout: 8), "停止录音后必须仍可编辑转写文字")
        transcript.tap()
        transcript.typeText("从录音中留下的想法")
        XCTAssertEqual(transcript.value as? String, "从录音中留下的想法", "用户修改的转写正文应留在编辑框")
        app.buttons["idea-voice-finish-editing"].tap()
        XCTAssertFalse(app.keyboards.firstMatch.exists, "完成编辑后键盘应收起，保存操作必须可达")
        app.buttons["idea-voice-save"].tap()
        XCTAssertTrue(app.staticTexts["idea-saved-feedback"].waitForExistence(timeout: 8), "落盘后必须显示已记录")
        let savedShot = XCTAttachment(screenshot: app.screenshot())
        savedShot.name = "P01-idea-library-voice"
        savedShot.lifetime = .keepAlways
        add(savedShot)
        let voice = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "语音想法")).firstMatch
        XCTAssertTrue(voice.waitForExistence(timeout: 8), "录音必须出现在想法库")
        XCTAssertTrue(voice.label.contains("从录音中留下的想法"), "语音卡必须显示保存时用户编辑的正文")
        voice.tap()
        let play = app.buttons["idea-play-recording"]
        XCTAssertTrue(play.isEnabled, "已保存语音必须能播放原始录音")
        play.tap()
        XCTAssertEqual(play.label, "停止播放录音", "点击播放后控件应立即显示可停止的状态")
        play.tap()
        XCTAssertEqual(play.label, "播放原始录音", "点击停止后控件应立即恢复播放标签")
        XCTAssertTrue(app.buttons["idea-retry-transcription"].exists, "待转写录音必须有重试入口")
        let retryShot = XCTAttachment(screenshot: app.screenshot())
        retryShot.name = "Voice-idea-retry"
        retryShot.lifetime = .keepAlways
        add(retryShot)
        app.terminate(); app.launch()
        openIdeas(app)
        XCTAssertTrue(voice.waitForExistence(timeout: 8), "重启后必须恢复同一条语音想法")
        XCTAssertTrue(voice.label.contains("从录音中留下的想法"), "重启后用户编辑的转写不能丢失")
        voice.tap()
        XCTAssertTrue(play.isEnabled, "重启后原始录音仍必须可播放")
        play.tap()
        let finished = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label == %@", "播放原始录音"), object: play)
        XCTAssertEqual(XCTWaiter.wait(for: [finished], timeout: 15), .completed,
                       "录音自然结束后播放按钮必须恢复初始文案")
        XCTAssertTrue(app.buttons["idea-retry-transcription"].exists, "重启后仍须允许重试转写")
    }

    /// 打开语音页先展示待录音状态，只有点录音按钮才请求设备权限并开始采集。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   独立测试命名空间 1 个，初始录音数 0；进入语音页前后仍为 0。
    ///   8 秒 = App 启动与页面转场约 3 秒 + 模拟器响应余量 5 秒。
    ///
    /// 执行过程：
    ///   1. 打开想法库，选择语音记录。
    ///   2. 不点录音按钮，检查按钮与保存操作；点编辑按钮输入文字，再点完成。
    ///   3. 尝试关闭并取消放弃，核对正文保留；从返回入口明确放弃后回到想法库。
    ///
    /// 预期结果：录音按钮显示「开始录音」；编辑按钮能打开/收起键盘；无录音时即使输入文字也不能保存语音想法。
    @MainActor func testVoicePageWaitsForExplicitRecordingTap() throws {
        try checkVoicePageEditing(largeText: false)
    }

    /// 最大辅助字号下，语音正文可编辑且键盘不遮住返回/关闭入口。
    ///
    /// 数据构造：1 个隔离空库、AX5 系统字号、1 段未保存正文；8秒=转场3秒+模拟器余量5秒。
    /// 执行过程：进入语音页 → 编辑 → 打开系统键盘 → 结束编辑 → 取消放弃 → 返回并确认放弃。
    /// 预期结果：退出按钮和输入正文位于键盘上方；正文保留；未录音不可保存，放弃不产生想法。
    @MainActor func testVoiceEditorKeepsNavigationAtMaximumTextSize() throws {
        try checkVoicePageEditing(largeText: true)
    }

    @MainActor private func checkVoicePageEditing(largeText: Bool) throws {
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_IDEA_TEST_NAMESPACE"] = UUID().uuidString
        app.launchArguments = ["-UIPreferredContentSizeCategoryName",
                               largeText ? "UICTContentSizeCategoryAccessibilityXXXL" : "UICTContentSizeCategoryL"]
        app.launch()
        openIdeas(app)
        app.buttons["record-idea"].tap()
        app.buttons["record-voice-idea"].firstMatch.tap()
        let record = app.buttons["idea-voice-record-control"]
        XCTAssertTrue(record.waitForExistence(timeout: 8), "语音页必须展示真实录音入口")
        XCTAssertTrue(app.staticTexts["等待录音"].waitForExistence(timeout: 3), "进入语音页应停留在待录音状态")
        XCTAssertEqual(record.label, "开始录音", "进入页面不应自行请求权限或开始录音")
        XCTAssertFalse(app.buttons["idea-voice-save"].isEnabled, "尚未录音时保存必须禁用")
        let edit = app.buttons["idea-voice-finish-editing"]
        edit.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3), "点编辑转写文字必须打开系统键盘")
        XCTAssertTrue(app.frame.contains(app.buttons["idea-voice-back"].frame), "键盘出现不能把返回按钮挤出屏幕")
        XCTAssertTrue(app.frame.contains(app.buttons["idea-voice-close"].frame), "键盘出现不能把关闭按钮挤出屏幕")
        XCTAssertEqual(edit.label, "完成编辑", "进入编辑后按钮应明确提供完成操作")
        let transcript = app.textViews["idea-voice-transcript"]
        transcript.typeText("先记下这句话")
        let keyboard = XCTAttachment(screenshot: app.screenshot())
        keyboard.name = largeText ? "P02-keyboard-AX5" : "P02-keyboard-standard"
        keyboard.lifetime = .keepAlways; add(keyboard)
        let finish = app.buttons["idea-voice-keyboard-done"]
        // AX5 的 body 行高约 64pt，7 个汉字占两行；标准字号一行约 22pt。
        // 加 TextEditor 上内边距 8pt，最低可读高度分别为 64 × 2 + 8 = 136pt / 22 + 8 = 30pt。
        let readableHeight: CGFloat = largeText ? 136 : 30
        XCTAssertGreaterThanOrEqual(transcript.frame.minY, app.scrollViews.firstMatch.frame.minY,
                                    "聚焦正文不能滚出内容区顶部")
        XCTAssertLessThanOrEqual(transcript.frame.minY + readableHeight, finish.frame.minY - 4,
                                 "正在输入的正文必须显示在键盘上方，不能只有完成按钮可见")
        XCTAssertTrue(finish.isHittable, "完成编辑必须固定在键盘上方，不能依赖滚动寻找")
        XCTAssertLessThanOrEqual(finish.frame.maxY, app.keyboards.firstMatch.frame.minY,
                                 "完成编辑不可被系统键盘遮挡")
        finish.tap()
        XCTAssertFalse(app.keyboards.firstMatch.exists, "点完成编辑应收起系统键盘")
        XCTAssertEqual(transcript.value as? String, "先记下这句话", "收起键盘不能清空编辑内容")
        XCTAssertFalse(app.buttons["idea-voice-save"].isEnabled, "仅有文字不能保存成缺少原始录音的语音想法")
        app.buttons["idea-voice-close"].tap()
        XCTAssertTrue(app.buttons["放弃文字"].waitForExistence(timeout: 3), "未保存的编辑正文不能在关闭时直接丢弃")
        XCTAssertTrue(app.buttons["继续编辑"].isHittable, "退出确认必须提供可见且可操作的保留正文入口")
        let confirmation = XCTAttachment(screenshot: app.screenshot())
        confirmation.name = largeText ? "P02-unsaved-confirmation-AX5" : "P02-unsaved-confirmation"
        confirmation.lifetime = .keepAlways; add(confirmation)
        app.buttons["继续编辑"].tap()
        XCTAssertEqual(transcript.value as? String, "先记下这句话", "取消放弃后必须保留正文")
        XCTAssertFalse(app.buttons["放弃文字"].exists, "继续编辑后确认层应关闭，不能仍拦截正文操作")
        app.buttons["idea-voice-back"].tap()
        XCTAssertTrue(app.buttons["继续编辑"].waitForExistence(timeout: 3), "返回入口必须提供与关闭相同的正文保留操作")
        app.buttons["放弃文字"].tap()
        XCTAssertTrue(app.buttons["record-idea"].waitForExistence(timeout: 3), "明确放弃后应返回想法库")
        XCTAssertFalse(app.staticTexts["idea-saved-feedback"].exists, "放弃正文不能出现已保存反馈")
    }

    /// P02 语音页的转写卡保持正文卡片比例，不挤占录音按钮与波形。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   P02 参考图内屏约 1365 px 高，转写卡约 285 px，高度比约 285 / 1365 = 0.21。
    ///   验收上界 0.24 容纳系统字体与 iPhone 屏幕差异；下界 0.16 保证仍可编辑。
    ///
    /// 执行过程：打开独立测试想法库 → 进入语音页 → 读取卡片和屏幕的真实布局框。
    ///
    /// 预期结果：转写卡存在；高度占屏幕 16%–24%，录音按钮仍可见。
    @MainActor func testVoiceTranscriptCardMatchesPrototypeProportion() {
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_IDEA_TEST_NAMESPACE"] = UUID().uuidString
        app.launch()
        openIdeas(app)
        app.buttons["record-idea"].tap()
        app.buttons["record-voice-idea"].firstMatch.tap()
        let card = app.otherElements["idea-voice-transcript-card"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "P02 转写卡必须有可验收的真实布局元素")
        let ratio = card.frame.height / app.frame.height
        XCTAssertGreaterThan(ratio, 0.16, "转写卡不能缩到无法编辑的高度")
        XCTAssertLessThan(ratio, 0.24, "转写卡过高会挤占 P02 的录音控制区域")
        XCTAssertTrue(app.buttons["idea-voice-record-control"].exists, "转写卡下方必须仍能看到录音按钮")
        XCTAssertTrue(app.scrollViews.firstMatch.frame.contains(app.staticTexts["点击开始录音"].frame),
                      "标准字号的录音操作说明必须完整位于滚动区域，不能被底部保存栏截断")
        // 保存栏顶部 = 保存按钮 minY - 17pt 顶部内边距；说明必须在功能层之前结束。
        XCTAssertLessThanOrEqual(app.staticTexts["点击开始录音"].frame.maxY,
                                 app.buttons["idea-voice-save"].frame.minY - 17,
                                 "录音说明不能进入底部材质层的遮挡范围")
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "P02-motion-idle"; shot.lifetime = .keepAlways; add(shot)
    }

    /// 没有连接 Mac 时，文字想法仍可记录、修改、搜索并在重启后恢复。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   一个独立的测试命名空间和 1 条想法；初始正文「首屏太复杂」，修改为「首屏交接更简单」。
    ///   8 秒 = 原生 App 启动与动画 3 秒 + 本地 SQLite 读取与键盘弹出余量 5 秒。
    ///
    /// 执行过程：
    ///   1. 从现有 App 入口打开想法库 → 文字记录 → 保存并确认「已记录」。
    ///   2. 打开刚保存的卡片修改正文 → 展开搜索并查找新词。
    ///   3. 终止并重启 App → 回到想法库查询原记录。
    ///
    /// 预期结果：始终只有一条独立想法；新正文跨重启存在；旧正文不残留；不会出现任务回执。
    @MainActor func testTextIdeaIsOfflineEditableSearchableAndPersistent() throws {
        let app = XCUIApplication()
        app.launchEnvironment["NORMA_IDEA_TEST_NAMESPACE"] = UUID().uuidString
        app.launch()
        openIdeas(app)
        XCTAssertTrue(app.staticTexts["想法库"].waitForExistence(timeout: 8), "入口必须打开原生想法库")
        app.buttons["record-idea"].tap()
        app.buttons["record-text-idea"].firstMatch.tap()
        let editor = app.textViews["idea-body-editor"]
        XCTAssertTrue(editor.waitForExistence(timeout: 8), "文字想法必须有真实可编辑输入框")
        editor.tap(); editor.typeText("首屏太复杂")
        app.buttons["save-idea"].tap()
        XCTAssertTrue(app.staticTexts["idea-saved-feedback"].waitForExistence(timeout: 8), "只有落盘成功才显示已记录")
        let original = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "首屏太复杂")).firstMatch
        XCTAssertTrue(original.waitForExistence(timeout: 8), "保存的正文应立即出现在列表")
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "P01-idea-library-text"; shot.lifetime = .keepAlways; add(shot)
        app.buttons["idea-card-menu"].tap()
        XCTAssertTrue(app.buttons["编辑想法"].firstMatch.waitForExistence(timeout: 3), "卡片更多入口必须提供可执行操作")
        app.buttons["编辑想法"].firstMatch.tap()
        XCTAssertTrue(editor.waitForExistence(timeout: 8), "点击卡片应能修改同一条想法")
        // 5 个汉字 = 首屏太复杂；点首行文字右侧以把插入点放在末尾，再逐字删除。
        editor.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.08)).tap()
        editor.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 5))
        editor.typeText("首屏交接更简单")
        XCTAssertEqual(editor.value as? String, "首屏交接更简单", "保存前编辑框必须只包含新正文")
        app.buttons["save-idea"].tap()
        app.buttons["search-ideas-toggle"].tap()
        let search = app.textFields["idea-search-field"]
        XCTAssertTrue(search.waitForExistence(timeout: 8), "搜索入口必须可见并可输入")
        search.tap(); search.typeText("交接")
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "首屏交接更简单")).firstMatch.exists, "搜索应命中修改后的正文")
        XCTAssertFalse(app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "首屏太复杂")).firstMatch.exists, "编辑后旧正文不能残留为另一条想法")
        app.buttons["idea-sort"].tap()
        XCTAssertTrue(app.buttons["最早记录"].firstMatch.waitForExistence(timeout: 3), "排序入口应提供真实的顺序选择")
        app.buttons["最早记录"].firstMatch.tap()
        app.terminate(); app.launch()
        openIdeas(app)
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "首屏交接更简单")).firstMatch.waitForExistence(timeout: 8), "重启后应恢复编辑过的正文")
        XCTAssertFalse(app.staticTexts["任务已接收"].exists, "记录想法不能生成任务回执")
    }

    @MainActor private func openIdeas(_ app: XCUIApplication) {
        XCTAssertTrue(app.buttons["record-idea"].waitForExistence(timeout: 8), "想法库现在是启动首页")
    }
}
