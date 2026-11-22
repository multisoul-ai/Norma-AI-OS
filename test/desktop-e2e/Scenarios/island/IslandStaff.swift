import E2EKit
import Fixtures
import Foundation

/// 岛内幕僚页：全局对话、汇报按钮与回复回传。脚本化适配器验证协议；`island-staff-real` 使用真实后端。
@MainActor
public enum IslandStaff {
    public nonisolated static let name = "island-staff"
    public nonisolated static let realName = "island-staff-real"
    public nonisolated static let voiceName = "island-staff-voice"

    public static func run(_ session: Session, real: Bool, speech: Bool = false) async throws {
        if speech {
            let model = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(
                "Library/Application Support/Norma OS/models"
            )
            guard FileManager.default.fileExists(atPath: model.appendingPathComponent("tts/qwen3-tts-0.6b-customvoice-8bit/installation.json").path),
                  FileManager.default.fileExists(atPath: model.appendingPathComponent("a1/runtime.json").path) else {
                throw E2EError.env("需要本机已安装 Qwen3-TTS 与 Local A1 引擎")
            }
        }
        let cli = URL(fileURLWithPath: session.appPath).deletingLastPathComponent().appendingPathComponent("norma")
        guard real || FileManager.default.isExecutableFile(atPath: cli.path) else {
            throw E2EError.env("缺少与 App 同目录的 norma CLI")
        }
        let brain = try IslandStaffFixture.install(real: real, appPath: session.appPath)
        let shell = try IslandNavigationFixture.install()
        let keys = ["ZDOTDIR", "CLAUDE_CONFIG_DIR", "NORMA_BRAIN_DIR", "NORMA_E2E_NO_ACTIVATE", "NO_COLOR"]
        let previous = Dictionary(uniqueKeysWithValues: keys.map { ($0, ProcessInfo.processInfo.environment[$0]) })
        var project: URL?
        defer {
            session.stop()
            if let output = session.output {
                for name in ["prompts.log"] {
                    if let data = try? Data(contentsOf: brain.appendingPathComponent(name)) { try? data.write(to: output.appendingPathComponent(name)) }
                }
            }
            for key in keys { if let value = previous[key] ?? nil { setenv(key, value, 1) } else { unsetenv(key) } }
            for path in [brain, shell, project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        setenv("ZDOTDIR", shell.path, 1)
        // 真实后端需要用户已有的 Claude 登录配置；脚本化模式才隔离配置目录。
        if real { unsetenv("CLAUDE_CONFIG_DIR") } else { setenv("CLAUDE_CONFIG_DIR", shell.appendingPathComponent("claude-config").path, 1) }
        setenv("NORMA_BRAIN_DIR", brain.path, 1)
        if speech { setenv("NO_COLOR", "1", 1) }
        try await session.step("启动隔离工作空间与运行中的脚本 Agent") {
            try session.launch(language: "zh-Hans")
            _ = try await session.capturePixels()
            session.stop()
            guard let root = session.dataRoot else { throw E2EError.env("缺少隔离目录") }
            let db = try session.databaseURL()
            try Catalog.seedWorkspace(database: db, name: "Island staff")
            try Catalog.suppressFeatureGuide(database: db)
            if speech {
                let source = FileManager.default.homeDirectoryForCurrentUser
                    .appendingPathComponent("Library/Application Support/Norma OS/models/tts/qwen3-tts-0.6b-customvoice-8bit")
                let models = root.appendingPathComponent("models/tts")
                try FileManager.default.createDirectory(at: models, withIntermediateDirectories: true)
                try FileManager.default.createSymbolicLink(at: models.appendingPathComponent(source.lastPathComponent), withDestinationURL: source)
                try SQLRaw.execute(database: db, statements: [
                    ("voice.desktop.mode", "legacy"), ("voice.tts.active_provider_id", "local"),
                    ("voice.tts.model_id", "qwen3-tts-0.6b-customvoice-8bit"), ("voice.tts.voice_id", "ryan"),
                    ("voice.agent_task_announcements_enabled", "false"),
                ].map { (sql: "INSERT INTO settings(key,value,updated_at_unix) VALUES(?1,?2,0) ON CONFLICT(key) DO UPDATE SET value=excluded.value", bindings: [$0.0, $0.1]) })
            }
            project = try CanvasProjectFixture.install(dataRoot: root, workspaceName: "Island staff")
            try session.launch(language: "zh-Hans")
            try await session.visualClick("点击Island staff工作空间卡片")
            try await session.visualClick("点击E2E Project项目卡片进入画布")
            try await session.visualClick("点击画布底部命令输入框")
            try await session.type("/new claude")
            try await session.shortcut(key: 36, modifiers: [])
            _ = try await session.visualCheck([VisualCriterion(id: "live", text: "Claude卡片终端显示ISLAND LIVE，卡片运行中，没有启动错误")])
            return "运行中的脚本 Agent 让概览有可汇报内容"
        }
        guard let island = try await session.nativeWindows().first(where: { $0.width >= 300 && $0.width < 700 && $0.height <= 60 }) else {
            throw E2EError.product("缺少独立岛窗口")
        }
        setenv("NORMA_E2E_NO_ACTIVATE", "1", 1)
        try await session.selectNativeWindow(island.number)
        try await session.step("幕僚页显示对话输入、发送与汇报") {
            let compact = try await session.capturePixels()
            try await session.hover(local: CGPoint(x: compact.windowFrame.width / 2, y: 17), on: compact, dwell: 250_000_000)
            try await session.visualClick("点击幕僚页签")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "staff", text: "与原型一致：幕僚页签选中；头部有紫色渐变圆形头像、粗体“幕僚”与灰色“盯着 1 个工作空间”，右侧是紫色胶囊“▶ 语音汇报进度”；其下是深灰圆角摘要卡片（暂无汇报提示）；下方有“和幕僚说”灰标题与三枚描边圆角气泡；底部是深色胶囊输入框（占位“给幕僚发消息…”）和右侧圆形向上箭头发送钮；没有启用引导；文字不重叠、不被裁切")
            ]), "01-staff-chat.png")
            return "对话输入、发送、汇报均可见"
        }
        try await session.step("发送消息并收到回传的文字回复") {
            try await session.visualClick("点击底部“给幕僚发消息…”输入框")
            try await session.type("你好幕僚")
            try await session.shortcut(key: 36, modifiers: [])
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "reply", text: real
                    ? "摘要卡片里先有灰色“你：你好幕僚”，随后出现一条白色的真实模型回复文字；回复不是空白或错误"
                    : "摘要卡片里有灰色“你：你好幕僚”，下一行出现白色回复“幕僚收到：你好幕僚”；卡片文字清晰不被裁切")
            ]), "02-reply.png")
            return "用户消息进入唯一大脑，reply 回到岛内对话"
        }
        try await session.step("汇报按钮捕获跨空间概览并得到回复") {
            try await session.visualClick("点击紫色“语音汇报进度”按钮")
            if speech {
                try await verifySpokenReport(session)
            }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "report", text: speech
                    ? "摘要卡片新增回复“目前有一个智能体在运行…”，文字清晰无裁切；没有橙色‘语音不可用’。汇报按钮恢复可点击"
                    : real
                    ? "摘要卡片出现新的幕僚回复，内容是对当前工作状态的口语总结；语音不可用时显示橙色“语音不可用，这条回复仅显示文字。”而不是假字幕"
                    : "摘要卡片新增回复“目前有一个智能体在运行…”；语音不可用时显示橙色“语音不可用，这条回复仅显示文字。”而不是假字幕；头部副标题出现“上次汇报 刚刚”；紫色汇报按钮恢复可点击")
            ]), "03-report.png")
            return speech ? "自动播报关闭时，Ryan 手动汇报收到 macOS 播完回调" : "汇报请求已回传，语音状态如实显示"
        }
        try await session.step("核对大脑收到的提示") {
            if real { return "真实后端：提示内容以截图为准" }
            let log = IslandStaffFixture.prompts(brain)
            guard log.contains("【用户】你好幕僚"), log.contains("--kind reply") else {
                throw E2EError.product("用户请求提示缺少原文或 --kind reply：\(log)")
            }
            guard log.contains("【用户要求汇报】"), log.contains("Island staff/E2E Project"), log.contains("--reply-to ") else {
                throw E2EError.product("汇报提示缺少有界概览的来源标注或请求编号 --reply-to：\(log)")
            }
            return "提示含原文、reply 指令与带来源的有界概览"
        }
    }

    private static func verifySpokenReport(_ session: Session) async throws {
        guard let output = session.output else { throw E2EError.env("缺少 App 日志目录") }
        let url = output.appendingPathComponent("app.log")
        let deadline = Date().addingTimeInterval(60)
        while Date() < deadline {
            let log = (try? String(contentsOf: url, encoding: .utf8)) ?? ""
            if log.contains("agent announcement playback finished node_id=0 outcome=Report completed=true") {
                let syntheses = log.components(separatedBy: "\n").filter { $0.contains("Qwen3-TTS request sent") }
                guard syntheses.count == 1, let request = syntheses.first, request.contains("voice=\"ryan\""),
                      request.contains("input_chars=28"), request.contains("input_has_kana=false") else {
                    throw E2EError.product("应仅提交一次 28 字的中文 Ryan 合成，实际共 \(syntheses.count) 次")
                }
                return
            }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("手动汇报没有完整播放，见 app.log")
    }
}
