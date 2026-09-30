import XCTest

final class ChatInteractionFlowTests: XCTestCase {
    /// 冷启动原卡进入运行后，项目订阅断流不能清空正在编辑的草稿或丢失键盘焦点。
    /// 数据构造：原卡7初始无run，随后恢复run-7/session-7；输入Keep my draft；
    ///   8s = 5s网络读取超时 + 3s原生UI调度余量，足以触发真实重连。
    /// 执行过程：打开原卡→恢复运行→输入→暂停项目响应→继续输入→恢复→再次断线至重试耗尽→主动重读。
    /// 预期结果：断流保留历史、草稿及焦点；发送需重新验证权限；恢复接收新正文；终止重试后不残留恢复提示。
    @MainActor func testReconnectKeepsDraftAndKeyboardOnOriginalCard() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else { throw XCTSkip("仅隔离聊天fixture") }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        let flags = ["ui-chat-interactions", "ui-chat-ready", "ui-card-management", "ui-chat-cold-session"].map { directory.appendingPathComponent($0) }
        let paused = directory.appendingPathComponent("ui-chat-project-paused")
        let final = directory.appendingPathComponent("ui-chat-final")
        for flag in flags { try Data().write(to: flag) }
        defer { for flag in flags + [paused, final] { try? FileManager.default.removeItem(at: flag) } }
        let app = XCUIApplication(); app.launch()
        let card = app.buttons["agent-card-7"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "只打开隔离原卡")
        card.tap()
        let input = app.descendants(matching: .any)["chat-message-input"].firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 8), "原生输入框应可用")
        try FileManager.default.removeItem(at: flags[3])
        XCTAssertTrue(app.staticTexts["初始历史末条"].waitForExistence(timeout: 8), "须先绑定恢复后的真实会话")
        input.tap(); input.typeText("Keep my draft")
        XCTAssertTrue(app.buttons["chat-send-message"].isEnabled, "断流前需确认控制权限已有效")
        try Data().write(to: paused)
        XCTAssertTrue(app.staticTexts["无法发送，请检查连接与权限。"].waitForExistence(timeout: 8), "真实订阅超时后发送须暂停")
        XCTAssertTrue(app.staticTexts["初始历史末条"].exists, "项目和历史重新验证期间须保留已确认的对话")
        XCTAssertFalse(app.progressIndicators["正在读取对话…"].exists, "已有历史时不能用加载遮罩替换内容")
        XCTAssertTrue(app.buttons["重新读取对话"].isEnabled, "恢复期间仍应允许用户主动重新读取")
        XCTAssertEqual(input.value as? String, "Keep my draft", "网络重连不能切回旧run并让输入消失")
        XCTAssertTrue(input.isEnabled, "离线草稿仍应可编辑")
        XCTAssertTrue(app.keyboards.firstMatch.exists, "后台重连不能销毁输入框并收起键盘")
        input.typeText(" offline")
        XCTAssertEqual(input.value as? String, "Keep my draft offline", "重连等待期间仍须接收用户输入")
        XCTAssertFalse(app.buttons["chat-send-message"].isEnabled, "保持编辑不能绕过失效的发送权限")
        try Data().write(to: final)
        try FileManager.default.removeItem(at: paused)
        let restored = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in app.buttons["chat-send-message"].isEnabled }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [restored], timeout: 8), .completed, "重新验证同一会话后应恢复发送")
        XCTAssertEqual(input.value as? String, "Keep my draft offline", "恢复连接不能覆盖离线编辑")
        XCTAssertTrue(app.staticTexts["最终整理好的回复"].waitForExistence(timeout: 8), "历史重连必须接收新正文，不能停留在缓存内容")
        XCTAssertFalse(app.staticTexts["正在重新连接对话…"].exists, "重新验证后的历史应结束恢复提示")
        XCTAssertFalse(FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-chat-sent.json").path), "重连不能自动提交草稿")

        try Data().write(to: paused)
        XCTAssertTrue(app.staticTexts["无法发送，请检查连接与权限。"].waitForExistence(timeout: 8), "再次断线仍须暂停发送")
        let exhausted = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            app.staticTexts["电脑暂时无法处理请求。已发送的操作请查询结果。"].firstMatch.exists
                && !app.staticTexts["正在重新连接对话…"].exists
        }, object: nil)
        // 32s = 最多4次读取 × 5s网络上限 + (1+2+4)s退避 + 5s原生调度余量。
        XCTAssertEqual(XCTWaiter.wait(for: [exhausted], timeout: 32), .completed, "重试已耗尽时必须结束恢复提示，不能让用户一直等待")
        XCTAssertEqual(input.value as? String, "Keep my draft offline", "重试耗尽也不能丢弃草稿")
        XCTAssertFalse(app.buttons["chat-send-message"].isEnabled, "最终网络错误仍不能放行发送")
        try FileManager.default.removeItem(at: paused)
        app.buttons["重新读取对话"].tap()
        XCTAssertTrue(app.staticTexts["最终整理好的回复"].waitForExistence(timeout: 8), "主动重读应恢复原会话")
        XCTAssertEqual(input.value as? String, "Keep my draft offline", "主动重读也不能覆盖输入")
        XCTAssertFalse(app.staticTexts["正在重新连接对话…"].exists, "恢复后的页面不能遗留重连提示")
        XCTAssertFalse(FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-chat-sent.json").path), "主动恢复只能读取，不能发送草稿")
    }

    /// 打开已知会话时，历史读取不应串行等待项目订阅的首个响应。
    /// 数据构造：隔离原卡7/run-7/session-7；暂停项目响应，历史源正常且权限为只读。
    ///   2s 调度预算小于服务4s超时，可区分并行读取与超时后重试。
    /// 执行过程：打开项目 → 暂停快照 → 点击卡片 → 检查历史请求 → 恢复快照 → 验证完整正文。
    /// 预期结果：项目等待期间已请求历史；恢复后正文可见；不能发送或出现错会话内容。
    @MainActor func testHistoryStartsBeforeProjectSnapshotReturns() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("仅隔离配对服务")
        }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        let paused = directory.appendingPathComponent("ui-chat-project-paused")
        let started = directory.appendingPathComponent("ui-chat-history-started")
        defer {
            try? FileManager.default.removeItem(at: paused)
            try? FileManager.default.removeItem(at: started)
        }
        let app = XCUIApplication(); app.launch()
        app.buttons["workspace-tab-projects"].tap()
        let card = app.buttons["agent-card-7"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "8s覆盖订阅和模拟器启动，只操作隔离原卡")
        try Data().write(to: paused)
        card.tap()
        let requested = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            FileManager.default.fileExists(atPath: started.path)
        }, object: nil)
        let result = XCTWaiter.wait(for: [requested], timeout: 2)
        XCTAssertEqual(result, .completed, "历史读取不能等待项目响应后才启动")
        try FileManager.default.removeItem(at: paused)
        guard result == .completed else { return }
        XCTAssertTrue(app.staticTexts["来自原会话 session-7 的完整回复"].waitForExistence(timeout: 8), "核验后应显示完整原会话")
        XCTAssertFalse(app.buttons["chat-send-message"].isEnabled, "并行只读请求不得提升发送权限")
        XCTAssertFalse(app.staticTexts["来自原会话 session-8 的完整回复"].exists, "不能混入另一会话")
    }

    /// 历史会话尚无运行进程时，原生聊天可保存短草稿，重进页面后由用户发送。
    ///
    /// 数据构造：原卡 7/session-7，启动前 run=null；hi 为 2 字节，远小于 64KiB 草稿上限。
    ///   8s = 2s 项目订阅周期 + 6s 原生导航及 TLS 调度余量，不是性能指标。
    /// 执行过程：隔离 WSS 返回未启动原卡 → 输入 hi → 核对无保存状态提示 → 重启 App → 点击发送。
    /// 预期结果：正常保存不显示提示、按钮可用；重启保留正文但不自动发送；准确回执后清空。
    @MainActor func testColdOriginalCardSavesDraftAndSendsAfterReopen() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("仅使用独立配对的原生聊天 fixture")
        }
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        let flags = ["ui-decision-deck", "ui-chat-interactions", "ui-card-management", "ui-chat-cold-session"].map { directory.appendingPathComponent($0) }
        for flag in flags { try Data().write(to: flag) }
        defer { for flag in flags { try? FileManager.default.removeItem(at: flag) } }
        let sent = directory.appendingPathComponent("ui-chat-sent.json")
        let app = XCUIApplication(); app.launch()
        app.buttons["workspace-tab-projects"].tap()
        let card = app.buttons["agent-card-7"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "必须能查看尚未启动的原卡")
        card.tap()
        let input = app.descendants(matching: .any)["chat-message-input"].firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 8), "后台原卡应能输入内容")
        input.tap(); input.typeText("hi")
        XCTAssertFalse(app.staticTexts["draft-save-status"].exists, "输入时不应显示草稿保存状态")
        XCTAssertFalse(app.staticTexts["正在保存草稿…"].exists, "后台保存不应占用输入区")
        XCTAssertFalse(app.staticTexts["草稿已保存到此手机"].exists, "保存成功不应显示常驻提示")
        XCTAssertFalse(app.staticTexts["draft-storage-error"].exists, "不能出现截图中的全局草稿失败提示")
        XCTAssertTrue(app.buttons["chat-send-message"].isEnabled, "草稿保存后，原卡的发送按钮应可用")
        app.terminate(); app.launch()
        XCTAssertTrue(card.waitForExistence(timeout: 8), "重启后应回到同一项目")
        card.tap()
        XCTAssertTrue(input.waitForExistence(timeout: 8), "重启后应恢复输入区")
        XCTAssertEqual(input.value as? String, "hi", "短草稿必须从本地磁盘恢复")
        XCTAssertFalse(FileManager.default.fileExists(atPath: sent.path), "保存和重启不能自动发送")
        let send = app.buttons["chat-send-message"]
        XCTAssertTrue(send.isEnabled, "恢复草稿不应锁住发送")
        send.tap()
        let cleared = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in input.value as? String == "" }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [cleared], timeout: 8), .completed, "收到准确回执后必须清空本条正文")
        let records = try JSONSerialization.jsonObject(with: Data(contentsOf: sent)) as? [[String: String]]
        XCTAssertEqual(records?.count, 1, "一次点击只能发送一次原文")
        XCTAssertEqual(records?.first?["text"], "hi", "必须发送用户输入的准确短消息")
        XCTAssertFalse(app.staticTexts["draft-storage-error"].exists, "运行开始后不应留下草稿失败状态")
    }

    /// Claude 已返回空屏时应结束连接进度，等待后续正文仍能正常更新。
    ///
    /// 数据构造：原卡 7 正在运行，真实 WSS 返回 24 行空字符串；
    ///   6s = 2 个 2s 心跳周期 + 2s UI 调度余量，足以区分等待响应和已收到空屏。
    /// 执行过程：进入原卡 → 确认服务已读取空屏 → 核验无永久 spinner → 输出正文。
    /// 预期结果：已连接不继续显示“正在连接实时输出”；正文自动出现且不会发送命令。
    @MainActor func testClaudeEmptyOutputFinishesLoadingAndReceivesText() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("只使用显式隔离聊天服务")
        }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        let flags = ["ui-chat-live", "ui-chat-empty-output", "ui-chat-empty-read"].map { dir.appendingPathComponent($0) }
        try Data().write(to: flags[0]); try Data().write(to: flags[1])
        defer { for flag in flags { try? FileManager.default.removeItem(at: flag) } }
        let app = XCUIApplication(); app.launch()
        app.buttons["workspace-tab-projects"].tap()
        let card = app.buttons["agent-card-7"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "必须进入明确原会话的 Claude 卡")
        card.tap()
        let read = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            FileManager.default.fileExists(atPath: flags[2].path)
        }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [read], timeout: 6), .completed, "须确证终端已经返回空屏，不能把未连接当作空闲")
        XCTAssertTrue(app.descendants(matching: .any)["workspace-loading"].firstMatch.waitForNonExistence(timeout: 6), "已收到空屏仍持续加载，会让 Claude 对话看起来卡住")
        XCTAssertFalse(app.staticTexts["chat-live-output"].exists, "空屏不能伪造 Agent 正文")
        try FileManager.default.removeItem(at: flags[1])
        let output = app.staticTexts["chat-live-output"]
        XCTAssertTrue(output.waitForExistence(timeout: 8), "空闲结束后应自动接收原卡实时正文")
        XCTAssertTrue(output.label.contains("正在写第一句"), "必须展示后续真实屏幕的内容")
        XCTAssertFalse(app.descendants(matching: .any)["workspace-loading"].firstMatch.exists, "正文到达后不应恢复旧加载状态")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-chat-sent.json").path), "读取空屏不能产生自动发送")
    }

    /// Claude就绪时被电脑明确拒绝的发送不能永久禁用输入，也不能丢草稿或自动重发。
    /// 数据构造：隔离原卡7/原session-7；一条短草稿；首个操作desktop_input_pending，第二次outcome_unknown，第三次用户重试才成功。
    /// 执行过程：就绪卡输入 → 正式TLS拒绝 → 核验原文与按钮恢复 → 结果未知并重启仍锁定 → 明确核实后用户重试成功。
    /// 预期结果：就绪不被旧拒绝请求锁住；未知结果保留保护；没有自动重发，成功前草稿不清空，最终仅接收一次原文。
    @MainActor func testReadyClaudeCanRetryAfterExplicitRejection() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui",
              let path = ProcessInfo.processInfo.environment["NORMA_TEST_QR"] else { throw XCTSkip("仅隔离聊天fixture") }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        let flags = ["ui-chat-interactions", "ui-chat-ready", "ui-card-management"].map { dir.appendingPathComponent($0) }
        let rejection = dir.appendingPathComponent("ui-chat-rejection")
        for flag in flags { try Data().write(to: flag) }
        try Data("desktop_input_pending".utf8).write(to: rejection)
        defer {
            for flag in flags + [rejection] { try? FileManager.default.removeItem(at: flag) }
        }
        let app = XCUIApplication(); app.launch()
        // 8s = 2s订阅心跳 + 6s原生导航/TLS余量，失败回执不应等待额外25s。
        let card = app.buttons["agent-card-7"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "只打开隔离Claude原卡")
        XCTAssertEqual(card.label, "检查登录测试，Claude，就绪", "必须复现就绪卡，而非借忙碌状态禁用发送")
        card.tap()
        let input = app.descendants(matching: .any)["chat-message-input"].firstMatch
        XCTAssertTrue(input.waitForExistence(timeout: 8), "必须进入原会话输入区")
        let message = "Keep this draft for retry"
        input.tap()
        if input.value as? String != message { input.typeText(message) }
        let send = app.buttons["chat-send-message"]
        XCTAssertTrue(send.isEnabled, "就绪且已授权的Claude首条输入应可发送")
        send.tap()
        let enabled = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-chat-rejected-operation").path) && send.isEnabled
        }, object: nil)
        let recovered = XCTWaiter.wait(for: [enabled], timeout: 8)
        XCTAssertEqual(recovered, .completed, "明确拒绝已结束，不能让待确认记录永久锁住就绪卡")
        XCTAssertEqual(input.value as? String, message, "拒绝不是成功，草稿必须完整保留")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-chat-sent.json").path), "不能自动重发或伪造已接收")
        guard recovered == .completed else {
            // 红测也经现有正式结果页清理准确的已拒绝请求，避免污染后续配对验收。
            app.buttons["查看这条消息的发送结果"].tap()
            let done = app.buttons["完成"]
            if done.waitForExistence(timeout: 8) { done.tap() }
            return
        }
        XCTAssertTrue(app.staticTexts["电脑终端中有待处理的输入，请先处理后再发送。"].exists, "应直接解释被拒绝原因")
        let rejectedID = try String(contentsOf: dir.appendingPathComponent("ui-chat-rejected-operation"), encoding: .utf8)
        let rejectedShot = XCTAttachment(screenshot: app.screenshot())
        rejectedShot.name = "ready-claude-rejection-recovered"; rejectedShot.lifetime = .keepAlways; add(rejectedShot)
        try Data("outcome_unknown".utf8).write(to: rejection)
        send.tap()
        let result = app.buttons["查看这条消息的发送结果"]
        XCTAssertTrue(result.waitForExistence(timeout: 8), "未知结果必须保留查询入口")
        XCTAssertFalse(send.isEnabled, "不能把未知结果当作拒绝来放行重发")
        XCTAssertEqual(input.value as? String, message, "未知结果不能清除未确认的草稿")
        app.terminate(); app.launch()
        XCTAssertTrue(card.waitForExistence(timeout: 8), "重启应回到同一个就绪项目")
        card.tap()
        XCTAssertTrue(result.waitForExistence(timeout: 8), "重启后仍必须保护未知结果")
        XCTAssertFalse(send.isEnabled, "重启不能绕过未决发送的保护")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("ui-chat-sent.json").path), "恢复只能查询，不能自动重发")
        result.tap()
        XCTAssertTrue(app.staticTexts["结果尚无法确认，请在电脑上核对"].waitForExistence(timeout: 8), "结果页必须明确显示未知")
        // 本fixture只记录请求，从未启动Agent；已核实无执行后才走正式清除确认。
        app.buttons["已在电脑上核实，清除记录"].tap()
        app.buttons["清除手机上的记录"].tap()
        XCTAssertTrue(send.waitForExistence(timeout: 8), "清除确认后返回原输入页")
        try FileManager.default.removeItem(at: rejection)
        send.tap()
        let cleared = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in input.value as? String == "" }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [cleared], timeout: 8), .completed, "用户主动重试成功后才清空草稿")
        let records = try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("ui-chat-sent.json"))) as? [[String: String]]
        XCTAssertEqual(records?.count, 1, "只有用户重试应进入原Agent一次")
        XCTAssertEqual(records?.first?["text"], message, "重试必须保留原文")
        XCTAssertNotEqual(records?.first?["operation_id"], rejectedID, "已拒绝操作不能复用，用户重试须有新幂等标识")
    }

    /// Agent尚未将回复写入历史时，聊天内直接显示原卡输出并接收改变的行。
    /// 数据构造：24行隔离终端、1行持续变化正文，历史先空缺该回复，结束后才写入最终正文。
    /// 执行过程：打开运行卡 → 看见第一句 → 原屏幕更新第二句 → 正式历史写入并结束。
    /// 预期结果：无需切换终端或刷新；旧行不重复，完成后最终消息替代实时预览。
    @MainActor func testChatShowsTerminalDeltasBeforeHistoryIsWritten() throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "notification-ui",
              let path = ProcessInfo.processInfo.environment["NORMA_TEST_QR"] else { throw XCTSkip("仅使用隔离聊天fixture") }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        for name in ["ui-chat-growth", "ui-chat-more"] {
            try? FileManager.default.removeItem(at: dir.appendingPathComponent(name))
        }
        let flags = ["ui-chat-interactions", "ui-chat-live", "ui-chat-live-delta", "ui-chat-final"].map { dir.appendingPathComponent($0) }
        try Data().write(to: flags[0]); try Data().write(to: flags[1])
        defer { for file in flags { try? FileManager.default.removeItem(at: file) } }
        let app = XCUIApplication(); app.launch(); app.buttons["workspace-tab-projects"].tap()
        let card = app.buttons["agent-card-7"]
        // 8s=2s既有订阅心跳+6s原生导航及TLS调度余量，非物理网络性能断言。
        XCTAssertTrue(card.waitForExistence(timeout: 8), "必须打开原卡")
        card.tap()
        let output = app.staticTexts["chat-live-output"]
        XCTAssertTrue(output.waitForExistence(timeout: 8), "历史未落盘时仍应有实时输出")
        guard output.exists else { return }
        XCTAssertTrue(output.label.contains("正在写第一句"), "必须显示来自原屏幕的第一句")
        XCTAssertFalse(app.staticTexts["最终整理好的回复"].exists, "不能提前伪造正式消息")
        try Data().write(to: flags[2])
        let changed = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in output.label.contains("正在写第二句") }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [changed], timeout: 8), .completed, "生成中持续接收改变行")
        XCTAssertFalse(output.label.contains("正在写第一句"), "屏幕重绘不能重复追加旧行")
        try Data().write(to: flags[3])
        XCTAssertTrue(app.staticTexts["最终整理好的回复"].waitForExistence(timeout: 8), "结束后必须展示正式历史")
        XCTAssertTrue(output.waitForNonExistence(timeout: 8), "完成后不能残留生成中预览")
    }
    /// 正式聊天初次打开停在最新记录，发送成功清空输入，同一回复增长时持续跟随底部。
    /// 数据构造：隔离TLS/SQLite与现有Claude卡7；24条各约4行历史超过一屏；后续同ID回复增加32行。
    /// 执行过程：打开原卡 → 核对初始末条 → 发送一句原文 → 核对回执清空 → 服务增量增长同一回复。
    /// 预期结果：无需手动滑动就能看到最新内容；只收到一次准确消息，回复增长不重复且输入不残留。
    @MainActor func testChatFollowsLiveReplyAndClearsSuccessfulMessage() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("仅使用显式隔离聊天fixture")
        }
        let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
        let enabled = dir.appendingPathComponent("ui-chat-interactions")
        let growth = dir.appendingPathComponent("ui-chat-growth")
        let more = dir.appendingPathComponent("ui-chat-more")
        try Data().write(to: enabled)
        try Data().write(to: dir.appendingPathComponent("ui-card-management"))
        defer {
            for file in [enabled, growth, more] { try? FileManager.default.removeItem(at: file) }
        }
        let app = XCUIApplication(); app.launch(); app.buttons["workspace-tab-projects"].tap()
        // 8s = 2s订阅心跳 + 6s真实TLS、授权更新与导航余量，不作为网络性能阈值。
        let card = app.buttons["agent-card-7"]
        XCTAssertTrue(card.waitForExistence(timeout: 8), "必须进入有准确run/session的原Claude卡")
        card.tap()
        let initial = app.staticTexts["初始历史末条"]
        XCTAssertTrue(initial.waitForExistence(timeout: 8), "初次读取应直接呈现历史末条")
        XCTAssertTrue(initial.isHittable, "历史末条应自动滚到可见区域")
        let input = app.descendants(matching: .any)["chat-message-input"].firstMatch
        let message = "请继续检查连接"
        input.tap(); input.typeText(message)
        let send = app.buttons["chat-send-message"]
        XCTAssertTrue(send.isEnabled, "正式授权后原卡可以发送")
        send.tap()
        let cleared = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            input.value as? String == ""
        }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [cleared], timeout: 8), .completed, "准确成功回执应清空已发送输入")
        let first = app.staticTexts["实时回复第一段"]
        XCTAssertTrue(first.waitForExistence(timeout: 8), "生成未结束前应收到第一段")
        XCTAssertTrue(first.isHittable, "发送后首段应自动进入可见区域")
        try Data().write(to: growth)
        let latest = app.staticTexts.matching(NSPredicate(format: "label ENDSWITH %@", "实时回复最新部分")).firstMatch
        XCTAssertTrue(latest.waitForExistence(timeout: 8), "同一消息的正文增长必须持续渲染")
        let composerTop = input.frame.minY
        // 20pt = 列表16pt底部padding + 4pt布局取整余量；末段不能藏在输入区下方。
        XCTAssertLessThanOrEqual(latest.frame.maxY, composerTop + 20, "回复增长必须跟随底部，不能仍停在回复开头")
        XCTAssertGreaterThan(latest.frame.maxY, app.segmentedControls["chat-content-tabs"].frame.maxY, "末段应在当前可视区域")
        XCTAssertFalse(first.exists, "增长应替换原ID内容，不能留下重复第一段消息")
        if app.buttons["收起键盘"].exists { app.buttons["收起键盘"].tap() }
        let list = app.scrollViews["chat-message-list"]
        list.swipeDown()
        let jump = app.buttons["chat-latest-message"]
        XCTAssertTrue(jump.waitForExistence(timeout: 3), "向上阅读后应提供回到最新入口，3s覆盖惯性滚动")
        try Data().write(to: more)
        let further = app.staticTexts.matching(NSPredicate(format: "label ENDSWITH %@", "继续生成的末段")).firstMatch
        XCTAssertTrue(further.waitForExistence(timeout: 8), "暂停跟随期间仍须接收增量")
        XCTAssertTrue(jump.exists, "更新不能强制退出历史阅读")
        XCTAssertGreaterThan(further.frame.maxY, input.frame.minY, "阅读旧内容时不能被新输出拉到底部")
        jump.tap()
        XCTAssertLessThanOrEqual(further.frame.maxY, input.frame.minY + 20, "主动点击后应回到实时末尾")
        XCTAssertFalse(jump.exists, "回到底部后应恢复自动跟随")
        let records = try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("ui-chat-sent.json"))) as? [[String: String]]
        XCTAssertEqual(records?.count, 1, "一次发送只能写入一次命令")
        XCTAssertEqual(records?.first?["text"], message, "原文必须完整且未重复")
        let shot = XCTAttachment(screenshot: app.screenshot()); shot.name = "chat-live-following"; shot.lifetime = .keepAlways; add(shot)
    }
}
