import E2EKit
import Fixtures
import Foundation

/// Agent 卡片启动看门狗：启动后 15 秒内没有收到该卡片的任何 hook（首个是 SessionStart），
/// 标题栏出现「迟迟未收到会话启动信号」警告徽标；收到 hook 即撤销；启动即发 SessionStart 的 CLI 不出现徽标。
/// 假 CLI 与「已安装 hook」的假 HOME 见 `FakeStartupAgentShell`。注意假 codex 有首屏输出，
/// 以此证明判据是 hook 而不是 PTY 输出。
@MainActor
public enum AgentStartupWatchdog {
    public nonisolated static let name = "agent-startup-watchdog"
    private static let badge = ["迟迟未收到会话启动信号"]

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fake: FakeStartupAgentShell.Install?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fake { try? FileManager.default.removeItem(at: fake.root) }
        }
        let workspace = "StartupWatchdogCheck"

        try await session.step("建立隔离工作空间与项目") {
            let install = try FakeStartupAgentShell.install()
            fake = install
            setenv("ZDOTDIR", install.root.path, 1)
            setenv("NORMA_E2E_LANGUAGE", "zh-Hans", 1)
            try session.launch(homeDirectory: install.home)
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
            try session.launch(homeDirectory: install.home)
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return try session.prove(names: [workspace], areaCount: 1, linkedTo: workspace).summary
        }

        try await session.step("进入项目") {
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search workspaces") {
                try await session.click(
                    local: CGPoint(x: Double(launch.windowFrame.width) - 111, y: 540),
                    on: launch
                )
                _ = try await session.wait(Expectation(absent: ["搜索工作空间", "Search workspaces"], timeout: 12))
            }
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            try await dismissGuide(session)
            return try await session.capture().summary
        }

        try await session.step("有首屏但不发 hook 的 Codex：15 秒前没有徽标，之后出现「迟迟未收到会话启动信号」") {
            try await newAgent("/new codex", session)
            try await Task.sleep(nanoseconds: 6_000_000_000)
            let early = try await session.capture()
            try session.save(early, "01-codex-before-threshold.png")
            guard !early.contains(badge[0]) else {
                throw E2EError.product("启动才几秒就出现启动信号徽标，阈值被提前触发：\(early.summary)")
            }
            let stalled = try await session.wait(Expectation(allOf: badge, timeout: 20))
            try session.save(stalled, "02-codex-stalled-badge.png")
            guard stalled.contains(FakeStartupAgentShell.codexBanner) else {
                throw E2EError.product("Codex 首屏没有画出，无法证明判据是 hook 而不是输出：\(stalled.summary)")
            }
            return stalled.summary
        }

        try await session.step("Codex 发出 SessionStart 后徽标消失") {
            guard let fake else { throw E2EError.env("假 CLI 目录丢失") }
            try fake.releaseCodex()
            let recovered = try await session.wait(Expectation(
                allOf: [FakeStartupAgentShell.codexBanner],
                absent: badge,
                timeout: 12
            ))
            try session.save(recovered, "03-codex-recovered.png")
            return recovered.summary
        }

        try await session.step("启动即发 SessionStart 的 Claude：等过 15 秒也不出现徽标") {
            try await newAgent("/new claude", session)
            _ = try await session.wait(Expectation(allOf: [FakeStartupAgentShell.claudeBanner], timeout: 12))
            try await Task.sleep(nanoseconds: 18_000_000_000)
            let healthy = try await session.capture()
            try session.save(healthy, "04-claude-healthy.png")
            guard !healthy.contains(badge[0]) else {
                throw E2EError.product("启动即发 SessionStart 的 Claude 被误报：\(healthy.summary)")
            }
            return healthy.summary
        }
    }

    private static func dismissGuide(_ session: Session) async throws {
        let frame = try await session.capture()
        if frame.contains("Feature Guide") || frame.contains("功能引导") || frame.contains("Skip Guide") || frame.contains("跳过引导") {
            try await session.shortcut(key: 53, modifiers: [])
            try await Task.sleep(nanoseconds: 600_000_000)
        }
    }

    private static func newAgent(_ command: String, _ session: Session) async throws {
        let frame = try await session.capture()
        try await session.click(
            local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
            on: frame
        )
        try await session.selectAll()
        try await session.type(command)
        try await session.shortcut(key: 36, modifiers: [])
        try await Task.sleep(nanoseconds: 1_500_000_000)
    }
}
