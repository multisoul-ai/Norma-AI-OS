import E2EKit
import Fixtures
import Foundation

/// 岛内幕僚页：全局对话、汇报按钮与回复回传。脚本化适配器验证协议；`island-staff-real` 使用真实后端。
@MainActor
public enum IslandStaff {
    public nonisolated static let name = "island-staff"
    public nonisolated static let realName = "island-staff-real"

    public static func run(_ session: Session, real: Bool) async throws {
        let brain = try IslandStaffFixture.install(real: real)
        let shell = try IslandNavigationFixture.install()
        let keys = ["ZDOTDIR", "CLAUDE_CONFIG_DIR", "NORMA_BRAIN_DIR", "NORMA_E2E_NO_ACTIVATE"]
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
        try await session.step("启动隔离工作空间与运行中的脚本 Agent") {
            try session.launch(language: "zh-Hans")
            _ = try await session.capturePixels()
            session.stop()
            guard let root = session.dataRoot else { throw E2EError.env("缺少隔离目录") }
            let db = try session.databaseURL()
            try Catalog.seedWorkspace(database: db, name: "Island staff")
            try Catalog.suppressFeatureGuide(database: db)
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
                VisualCriterion(id: "staff", text: "幕僚页签选中；面板有一条空对话提示、一个输入框（占位“问问幕僚…”）、实心的“发送”按钮和灰色的“汇报”按钮同一行；没有启用引导；文字不重叠、不被裁切")
            ]), "01-staff-chat.png")
            return "对话输入、发送、汇报均可见"
        }
        try await session.step("发送消息并收到回传的文字回复") {
            try await session.visualClick("点击幕僚输入框")
            try await session.type("你好幕僚")
            try await session.shortcut(key: 36, modifiers: [])
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "reply", text: real
                    ? "对话里先有“你：你好幕僚”，随后出现一条标为“幕僚”的真实模型回复文字；回复不是空白或错误"
                    : "对话里有“你：你好幕僚”，下方出现标为“幕僚”的回复“幕僚收到：你好幕僚”")
            ]), "02-reply.png")
            return "用户消息进入唯一大脑，reply 回到岛内对话"
        }
        try await session.step("汇报按钮捕获跨空间概览并得到回复") {
            try await session.visualClick("点击汇报按钮")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "report", text: real
                    ? "对话中出现新的幕僚回复，内容是对当前工作状态的口语总结；语音不可用时显示“语音不可用，这条回复仅显示文字。”而不是假字幕"
                    : "对话中新增幕僚回复“目前有一个智能体在运行…”；语音不可用时显示“语音不可用，这条回复仅显示文字。”而不是假字幕；汇报按钮恢复可点击")
            ]), "03-report.png")
            return "汇报请求已回传；语音状态如实显示"
        }
        try await session.step("核对大脑收到的提示") {
            if real { return "真实后端：提示内容以截图为准" }
            let log = IslandStaffFixture.prompts(brain)
            guard log.contains("【用户】你好幕僚"), log.contains("--kind reply") else {
                throw E2EError.product("用户请求提示缺少原文或 --kind reply：\(log)")
            }
            guard log.contains("【用户要求汇报】"), log.contains("Island staff/E2E Project") else {
                throw E2EError.product("汇报提示缺少有界概览的来源标注：\(log)")
            }
            return "提示含原文、reply 指令与带来源的有界概览"
        }
    }
}
