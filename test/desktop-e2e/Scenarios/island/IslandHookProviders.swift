import E2EKit
import Fixtures
import Foundation

/// Kimi、ZCode、InfCode 各验证一次岛内选项和一次终端收卡。stdout 形态按各家 spike。
@MainActor
public enum IslandHookProviders {
    public nonisolated static let name = "island-hook-providers"

    public static func run(_ session: Session) async throws {
        let hook = URL(fileURLWithPath: session.appPath).deletingLastPathComponent().appendingPathComponent("multisoul-agent-hook").path
        let fixture = try HookQuestionFixture.install(hookBinary: hook)
        var project: URL?
        let previous = ProcessInfo.processInfo.environment["ZDOTDIR"]
        defer {
            session.stop()
            if let output = session.output, let data = try? Data(contentsOf: fixture.root.appendingPathComponent("hook.log")) {
                try? data.write(to: output.appendingPathComponent("hook.log"))
            }
            if let previous { setenv("ZDOTDIR", previous, 1) } else { unsetenv("ZDOTDIR") }
            unsetenv("NORMA_E2E_NO_ACTIVATE")
            for path in [fixture.root, project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        setenv("ZDOTDIR", fixture.shell.path, 1)
        try await session.step("启动隔离画布") {
            try await IslandHookSupport.boot(session, language: "zh-Hans", shell: fixture.shell, workspace: "Hook providers", project: &project)
            return "三家脚本共用一个隔离 PATH，不改用户 home 里的 hook"
        }
        try await provider(session, root: fixture.root, command: "/new kimi", provider: "kimi", shot: "kimi") { meta in
            try IslandHookSupport.assertKimiDeny(meta, reason: "The user answered Spike color 1? with Blue.")
        }
        try await provider(session, root: fixture.root, command: "/new zcode", provider: "zcode", shot: "zcode") { meta in
            try IslandHookSupport.assertZCodeAllow(meta, prompt: "Spike color 1?")
        }
        try await provider(session, root: fixture.root, command: "/new infcode", provider: "infcode", shot: "infcode") { meta in
            try IslandHookSupport.assertInfCodeDeny(meta, reason: "The user answered Spike color 1? with Blue.")
        }
    }

    private static func provider(
        _ session: Session,
        root: URL,
        command: String,
        provider: String,
        shot: String,
        assert: ( [String: Any]) throws -> Void
    ) async throws {
        try await session.step("\(provider) 岛内 Blue 到达脚本，终端作答后收卡") {
            try await IslandHookSupport.spawn(session, command: command, provider: provider, root: root)
            let island = try await IslandHookSupport.island(session)
            try IslandHookSupport.trigger(root, provider: provider, cycle: 1, mode: "choice")
            try await IslandHookSupport.waitForAlert(session, number: island)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "appeared", text: "岛展开，可见需要你处理或需要你，待处理至少 1；题目 Spike color 1?，选项 Blue 和 Green")
            ]), "10-\(shot)-appeared.png")
            try await session.visualClick("在 Spike color 1? 下选择 Blue 单选项")
            try await session.visualClick("点击发送回复按钮")
            let answered = try await IslandHookSupport.waitMeta(root, provider: provider, cycle: 1)
            try assert(answered)
            try await IslandHookSupport.waitForCompact(session, number: island)
            try IslandHookSupport.trigger(root, provider: provider, cycle: 2, mode: "terminal")
            try await IslandHookSupport.waitForAlert(session, number: island)
            try IslandHookSupport.close(root, provider: provider, cycle: 2)
            let closed = try await IslandHookSupport.waitMeta(root, provider: provider, cycle: 2)
            try IslandHookSupport.assertEmptyStdout(closed, what: "\(provider) 终端作答")
            try await IslandHookSupport.waitForCompact(session, number: island)
            return "\(provider) 选项回执符合该家 stdout，终端回执后岛收回"
        }
    }
}
