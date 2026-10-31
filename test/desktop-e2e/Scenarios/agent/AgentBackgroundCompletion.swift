import E2EKit
import Fixtures
import Foundation

/// 后台 Bash 未结清时，Claude 卡片不得显示 Done。
/// 假 CLI 先发 run_in_background 与非空 background_tasks 的 Stop，停在 settle 文件出现之前。
/// 结清通知和空 Stop 之后，确认窗口结束才出现 Done。进程保持 sleep，窗口内不发 SessionEnd。
/// 语音默认关闭，完成播报由 drain 单测覆盖：未确认的 Stop 不进 terminal_tasks。
@MainActor
public enum AgentBackgroundCompletion {
    public nonisolated static let name = "agent-background-completion"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fake: FakeBackgroundAgentShell.Install?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fake { try? FileManager.default.removeItem(at: fake.root) }
        }
        let workspace = "BackgroundCompletionCheck"

        try await session.step("建立隔离工作空间与项目") {
            let install = try FakeBackgroundAgentShell.install()
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

        try await session.step("后台任务未结清时卡片保持运行中，不出现 Done") {
            try await newAgent("/new claude", session)
            let held = try await session.wait(Expectation(
                allOf: [FakeBackgroundAgentShell.holdBanner, "运行中"],
                absent: ["Done"],
                timeout: 15
            ))
            try session.save(held, "01-background-held.png")
            guard !held.contains("Done") else {
                throw E2EError.product("后台任务仍在时卡片已显示 Done：\(held.summary)")
            }
            return held.summary
        }

        try await session.step("结清并经过确认窗口后才显示 Done") {
            guard let fake else { throw E2EError.env("假 CLI 目录丢失") }
            try fake.release()
            let done = try await session.wait(Expectation(
                allOf: [FakeBackgroundAgentShell.settledBanner, "Done"],
                timeout: 12
            ))
            try session.save(done, "02-background-done.png")
            guard done.contains("运行中") == false else {
                throw E2EError.product("确认完成后卡片仍显示运行中：\(done.summary)")
            }
            return done.summary
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
