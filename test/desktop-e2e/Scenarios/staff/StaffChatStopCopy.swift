import AppKit
import Darwin
import E2EKit
import Fixtures
import Foundation

/// A real prompt, delayed ACP stream, scoped cancellation, and native clipboard and paste.
@MainActor
public enum StaffChatStopCopy {
    public nonisolated static let name = "staff-chat-stop-copy"
    public nonisolated static let englishName = "staff-chat-stop-copy-en"

    private static let user = "CHAT-STOP This is a long user message with literal *stars* that wraps inside the bubble while retaining the whole sentence for copying to the clipboard.\nUser ends here."
    private static let unselected = "Skip this. "
    private static let reply = "Reply begins with formatted words that wrap naturally as the assistant explains this long piece of sample text with more detail than fits on a single line.\nReply ends here."

    public static func run(_ session: Session, english: Bool = false) async throws {
        let root = try StaffFixture.install()
        let env = ["ZDOTDIR": root.path, "NORMA_BRAIN_DIR": root.path,
                   "CLAUDE_CONFIG_DIR": root.appendingPathComponent("claude-config").path]
        let previous = ProcessInfo.processInfo.environment
        let board = NSPasteboard.general
        let saved = (board.pasteboardItems ?? []).map { item in
            let copy = NSPasteboardItem()
            for type in item.types { if let data = item.data(forType: type) { copy.setData(data, forType: type) } }
            return copy
        }
        defer {
            session.stop()
            board.clearContents()
            if !saved.isEmpty { board.writeObjects(saved) }
            for key in env.keys {
                if let value = previous[key] { setenv(key, value, 1) } else { unsetenv(key) }
            }
            if let output = session.output {
                for name in ["stream-events.jsonl", "prompts.jsonl"] {
                    try? FileManager.default.copyItem(at: root.appendingPathComponent(name), to: output.appendingPathComponent(name))
                }
            }
            try? FileManager.default.removeItem(at: root)
        }
        for (key, value) in env { setenv(key, value, 1) }
        let language = english ? "en" : "zh-Hans"
        try session.launch(language: language)
        _ = try await session.capturePixels()
        let database = try session.databaseURL()
        session.stop()
        let project = root.appendingPathComponent("Chat Project")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        try Catalog.seedWorkspace(database: database, name: "Chat Stop Copy")
        try Catalog.seedArea(database: database, workspaceName: "Chat Stop Copy", root: project, displayName: "Chat Project")
        try Catalog.suppressFeatureGuide(database: database)
        try VoiceConfigurationFixture.theme(database: database, id: english ? "sapphire-night" : "ink-black")
        try session.launch(language: language)
        try await session.step("空态与默认输入区") {
            try await session.resize(width: 1280, height: 860)
            try await session.visualClick("点击 Chat Stop Copy 工作空间卡片")
            try await session.visualClick("点击底栏最左端的 Norma 圆形头像进入幕僚模式")
            try await check(session, "01-empty.png", "幕僚无任务，中央为空态，底部有正常空输入框、添加图片和发送箭头。没有停止按钮。检视任务栏、输入框内边距和按钮对齐。")
            try await focusComposer(session)
            board.clearContents()
            board.setString(user, forType: .string)
            try await session.shortcut(key: 9, modifiers: [0x37])
            try await session.shortcut(key: 36, modifiers: [])
            return "空态已查看，通过 Enter 发送带硬换行和长句的用户消息"
        }
        try await session.step("首段输出前后都能看到停止按钮") {
            try await wait("ACP 等待首段") { try stage(root, "chat-start") }
            try await check(session, "02-waiting.png", "用户长消息在右对齐气泡内清晰折行并保留 User ends here. 独立一行和 *stars* 字面字符。Norma 显示等待文案和加载图标，底部发送箭头旁有方形停止图标，无裁切或重叠。")
            try Data().write(to: root.appendingPathComponent("chat-start"))
            try await wait("ACP 首段") { try stage(root, "chat-partial") }
            try await check(session, "03-streaming.png", "Norma 回复从 Skip this. Reply begins 起，完整折行，formatted words 加粗且没有 Markdown 星号，末尾为 Reply ends here.。Read 步骤处于展开的过程区，停止按钮仍可见。没有 LATE OUTPUT。")
            return "等待与流式阶段都显示停止控件"
        }
        try await session.step("点击停止并保留可见文字") {
            let stop = try await session.visualLocate("定位底部消息输入框右下角的方形停止按钮，位于发送上箭头左侧")
            let clicked = ContinuousClock.now
            try await session.click(local: stop.point, on: stop.frame)
            do {
                try await wait("回复落盘为 Stopped") {
                    try messages(root).contains { ($0["Staff"] as? [String: Any])?["status"] as? String == "Stopped" }
                }
                try await wait("共享 ACP 收到取消或独占适配器已退出") {
                    if try stage(root, "chat-cancelled") { return true }
                    let partial = try StaffFixture.records(root, "stream-events.jsonl").first { $0["stage"] as? String == "chat-partial" }
                    guard let pid = partial?["pid"] as? Int32, pid > 1 else { return false }
                    return kill(pid, 0) == -1 && errno == ESRCH
                }
                try checkStopped(root)
            } catch {
                if let frame = try? await session.capturePixels() {
                    try? session.save(frame, "stop-failure.png")
                }
                throw error
            }
            let latency = clicked.duration(to: .now)
            guard latency < .seconds(2) else { throw E2EError.product("点击后等待停止超过两秒：\(latency)") }
            let stopped = english ? "Generation stopped." : "已停止生成。"
            try await check(session, "04-stopped-collapsed.png", "显示 \(stopped)，保留 Reply begins 到 Reply ends here. 的全部可见内容，formatted words 加粗，Markdown 符号未原样显示。过程已折叠，发送箭头旁的方形停止按钮消失。没有 LATE OUTPUT。")
            try await session.visualClick("点击已停止回复的过程摘要（已处理 1 步或英文对应文案）以展开过程")
            try await check(session, "05-stopped-expanded.png", "已停止的回复展开过程，能分辨 Read 步骤、正文和停止提示。正文和按钮保留且无重叠。")
            try await session.visualClick("再次点击同一条过程摘要以折叠过程")
            return "点击至确认停止用时 \(latency)，落盘保留首段并排除取消后的分片"
        }
        try await session.step("用户长消息跨行复制并粘贴") {
            try await selectAndCopy(session, board: board, first: "This is a long user message", last: "User ends here.", expected: String(user.dropFirst("CHAT-STOP ".count)), reverse: false)
            try await check(session, "06-user-selection.png", "选区从用户气泡内 This is a long user message 开始，跳过 CHAT-STOP 前缀并跨过长句的自动折行，覆盖到 User ends here.。选区留在气泡内，文字与选择背景可辨。")
            try await paste(session)
            try await check(session, "07-user-paste.png", "输入框内粘贴的是刚才选中的用户消息，包含字面 *stars*、长句和另起一行的 User ends here.。文字没有盖住底部按钮。")
            try await session.selectAll()
            try await session.deleteSelection()
            return "使用 ⌘C 和 ⌘V，剪贴板与用户选中的子串逐字相等"
        }
        try await session.step("反向拖选幕僚长消息并以选区开始下一轮") {
            try await selectAndCopy(session, board: board, first: "Reply begins", last: "Reply ends here.", expected: reply, reverse: true)
            try await check(session, "08-reply-selection.png", "已停止回复的选区从 Reply begins 开始，跳过 Skip this. 前缀，覆盖加粗的 formatted words、长内容折行及末段 Reply ends here.。选区、停止提示和过程标题互不遮挡。")
            try await paste(session)
            try await session.shortcut(key: 36, modifiers: [])
            try await wait("第二轮回复") { try messages(root).contains { StaffFixture.replyText($0) == "SECOND TURN READY" } }
            let users = try messages(root).compactMap { ($0["User"] as? [String: Any])?["text"] as? String }
            guard users == [user, reply] else { throw E2EError.product("粘贴后发送的消息与选择内容不同：\(users)") }
            try checkStopped(root)
            try await check(session, "09-next.png", "已发送粘贴的选中文字，幕僚出现 SECOND TURN READY，输入框恢复空态。旧回复没有 LATE OUTPUT。检查当前长气泡和回复之间的间距及边界。")
            return "第二轮发送了逐字匹配的粘贴文本，旧回复仍为 Stopped，新回复正常完成"
        }
    }

    private static func check(_ session: Session, _ name: String, _ text: String) async throws {
        try session.save(try await session.visualCheck([VisualCriterion(id: "appearance", text: text)]), name)
    }

    private static func focusComposer(_ session: Session) async throws {
        try await session.visualClick("点击中央底部消息输入框的文字区")
    }

    private static func paste(_ session: Session) async throws {
        try await focusComposer(session)
        try await session.shortcut(key: 9, modifiers: [0x37])
    }

    private static func selectAndCopy(_ session: Session, board: NSPasteboard, first: String, last: String, expected: String, reverse: Bool) async throws {
        let start = try await session.visualLocate("在对话正文中定位 \(first) 的第一个字符左边界，鼠标应位于这一行文字垂直中线。忽略任务标题和列表。")
        let end = try await session.visualLocate("在同一条对话消息的最后一行定位 \(last) 句号之后 6 点，保持在该行文字垂直中线。")
        try await session.drag(local: reverse ? end.point : start.point, to: reverse ? start.point : end.point, on: end.frame)
        board.clearContents()
        try await session.shortcut(key: 8, modifiers: [0x37])
        for _ in 0..<10 {
            if board.string(forType: .string) != nil { break }
            try await Task.sleep(nanoseconds: 30_000_000)
        }
        let copied = board.string(forType: .string)
        guard copied == expected else {
            if let frame = try? await session.capturePixels() {
                try? session.save(frame, "copy-failure.png")
            }
            throw E2EError.product("剪贴板内容不符合所选范围：\(String(reflecting: copied))")
        }
    }

    private static func stage(_ root: URL, _ stage: String) throws -> Bool {
        try StaffFixture.records(root, "stream-events.jsonl").contains { $0["stage"] as? String == stage }
    }

    private static func messages(_ root: URL) throws -> [[String: Any]] {
        try StaffFixture.tasks(root).first?["messages"] as? [[String: Any]] ?? []
    }

    private static func checkStopped(_ root: URL) throws {
        let stopped = try messages(root).filter { ($0["Staff"] as? [String: Any])?["status"] as? String == "Stopped" }
        guard stopped.count == 1, let message = stopped.first,
              StaffFixture.replyText(message) == unselected + reply.replacingOccurrences(of: "formatted words", with: "**formatted words**").replacingOccurrences(of: "\n", with: "\n\n") else {
            throw E2EError.product("已停止回复必须保留原始输出并排除迟到消息：\(stopped)")
        }
    }

    private static func wait(_ label: String, _ predicate: () throws -> Bool) async throws {
        for _ in 0..<100 {
            if try predicate() { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw E2EError.product("等待超时：\(label)")
    }
}
