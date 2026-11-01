import E2EKit
import Fixtures
import Foundation

/// 真实 Codex（经 Norma 私有 relay 的远程 TUI）YOLO 卡片：/resume 切换会话与 /fork 不被权限覆盖拒绝，
/// 恢复后仍为 Full Access。使用用户已安装的 Codex 与登录，发送一次极短模型请求以生成可恢复会话。
@MainActor
public enum AgentCodexResumeFork {
    public nonisolated static let name = "agent-codex-resume-fork"
    private static let rejected = "Permission overrides"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
        }
        let workspace = "CodexResumeCheck"

        try await session.step("建立隔离工作空间与项目") {
            setenv("NORMA_E2E_LANGUAGE", "zh-Hans", 1)
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            _ = try await session.wait(Expectation(anyOf: ["工作空间名称", "Workspace name"], timeout: 4))
            let dialog = try await session.capture()
            guard let input = dialog.pick(["网站改版", "Website redesign"], .bottom) else {
                throw E2EError.product("找不到工作空间名称输入框：\(dialog.summary)")
            }
            try await session.click(local: input.center, on: dialog)
            try await session.type(workspace)
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 12))
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            try Catalog.suppressFeatureGuide(database: dataRoot.appendingPathComponent("app.sqlite"))
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return try session.prove(names: [workspace], areaCount: 1, linkedTo: workspace).summary
        }

        try await session.step("进入项目") {
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 10))
            if try await session.capture().contains("输入名字就行") {
                try await session.shortcut(key: 53, modifiers: [])
                try await Task.sleep(nanoseconds: 600_000_000)
            }
            try await session.click(matching: ["1个项目", "1 project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            let frame = try await session.capture()
            if frame.contains("功能引导") || frame.contains("Feature Guide") {
                try await session.shortcut(key: 53, modifiers: [])
                try await Task.sleep(nanoseconds: 600_000_000)
            }
            return try await session.capture().summary
        }

        try await session.step("真实 Codex 卡片就绪并生成一个可恢复会话") {
            let frame = try await session.capture()
            try await session.click(
                local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
                on: frame
            )
            try await session.selectAll()
            try await session.type("/new codex")
            try await session.shortcut(key: 36, modifiers: [])
            let ready = try await session.wait(Expectation(allOf: ["Ask Codex", "YOLO"], absent: ["Codex 启动失败"], timeout: 40))
            try session.save(ready, "01-codex-ready.png")
            try await session.click(matching: ["Ask Codex"], pick: .bottom)
            try await Task.sleep(nanoseconds: 500_000_000)
            try await command(session, "Reply with the reverse of ELPPAENIP, uppercase, nothing else")
            let answered = try await session.wait(Expectation(allOf: ["PINEAPPLE"], timeout: 90))
            try session.save(answered, "02-seed-answered.png")
            try await command(session, "/new")
            _ = try await session.wait(Expectation(allOf: ["Ask Codex"], absent: ["PINEAPPLE"], timeout: 20))
            return "YOLO 卡片就绪，已生成会话并开启新会话"
        }

        try await session.step("/resume 选择器切回刚才的会话") {
            try await command(session, "/resume")
            let picker = try await session.wait(Expectation(allOf: ["Resume a previous session"], timeout: 20))
            try session.save(picker, "03-resume-picker.png")
            try await Task.sleep(nanoseconds: 1_500_000_000)
            try await session.shortcut(key: 36, modifiers: [])
            let resumed = try await session.wait(Expectation(
                allOf: ["PINEAPPLE", "Ask Codex"],
                absent: ["Resume a previous session", rejected],
                timeout: 30
            ))
            try session.save(resumed, "04-resumed.png")
            return "选择器恢复会话，无权限覆盖拒绝"
        }

        try await session.step("/status 显示恢复后的会话仍为 Full Access") {
            try await Task.sleep(nanoseconds: 1_500_000_000)
            try await command(session, "/status")
            let status = try await session.wait(Expectation(allOf: ["Full Access"], absent: [rejected], timeout: 15))
            try session.save(status, "05-status-full-access.png")
            return "Permissions: Full Access"
        }

        try await session.step("/fork 当前会话成功") {
            try await command(session, "/fork")
            let forked = try await session.wait(Expectation(allOf: ["Fork created"], absent: [rejected], timeout: 30))
            try session.save(forked, "06-forked.png")
            return "Fork created，无权限覆盖拒绝"
        }
    }

    private static func command(_ session: Session, _ text: String) async throws {
        try await session.type(text)
        try await Task.sleep(nanoseconds: 400_000_000)
        try await session.shortcut(key: 36, modifiers: [])
    }
}
