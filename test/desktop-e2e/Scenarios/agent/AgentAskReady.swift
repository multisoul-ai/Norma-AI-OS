import E2EKit
import Fixtures
import Foundation

/// `ask claude …` 新建卡片后，CLI 立刻发出 SessionStart（真实 Claude 的就绪形态）时，任务仍须送达恰好一次。
@MainActor
public enum AgentAskReady {
    public nonisolated static let name = "agent-ask-ready"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fake: FakeStartupAgentShell.Install?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fake { try? FileManager.default.removeItem(at: fake.root) }
        }
        let workspace = "AskReadyCheck"

        try await session.step("建立隔离工作空间并进入项目") {
            let install = try FakeReadyAgentShell.install()
            fake = install
            setenv("ZDOTDIR", install.root.path, 1)
            setenv("NORMA_E2E_LANGUAGE", "zh-Hans", 1)
            try session.launch(homeDirectory: install.home)
            try await BrainSupport.createWorkspace(session, name: workspace)
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            try session.launch(homeDirectory: install.home)
            let launch = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            try await session.click(local: launch.pick([workspace], .bottom)?.center ?? .zero, on: launch)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 15))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            try await BrainSupport.dismissGuide(session)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            try await BrainSupport.dismissGuide(session)
            return try await session.capture().summary
        }

        try await session.step("ask claude：SessionStart 立即到达，任务恰好送达一次") {
            guard let fake else { throw E2EError.env("假 CLI 目录丢失") }
            let frame = try await session.capture()
            try await session.click(
                local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38), on: frame)
            try await session.selectAll()
            try await session.type("ask claude READYTASK 嘻嘻嘻")
            try await session.shortcut(key: 36, modifiers: [])
            let deadline = Date().addingTimeInterval(45)
            while Date() < deadline, !FakeReadyAgentShell.received(fake).contains("READYTASK") {
                try await Task.sleep(nanoseconds: 500_000_000)
            }
            try await Task.sleep(nanoseconds: 3_000_000_000)
            let received = FakeReadyAgentShell.received(fake)
            guard received.components(separatedBy: "READYTASK 嘻嘻嘻").count - 1 == 1 else {
                try? session.save(try await session.capture(), "ask-ready-failure.png")
                throw E2EError.product("SessionStart 后任务没有恰好送达一次：\(received.debugDescription)")
            }
            return received
        }
    }
}
