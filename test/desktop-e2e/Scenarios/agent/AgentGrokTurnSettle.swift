import E2EKit
import Fixtures
import Foundation

/// PR #218 真实 App 验收：Grok 卡片在问答回合与普通回合结束后不停在「运行中」，
/// /clear 后旧会话迟到的 Stop（被门禁拒收）不能把卡片重新拉回「运行中」。
/// 脚本 Grok 经生产 hook 二进制上报，不调用真实模型；卡片状态只看真实窗口 OCR。
@MainActor
public enum AgentGrokTurnSettle {
    public nonisolated static let name = "agent-grok-turn-settle"

    public static func run(_ session: Session) async throws {
        let hook = URL(fileURLWithPath: session.appPath).deletingLastPathComponent()
            .appendingPathComponent("multisoul-agent-hook").path
        let fixture = try GrokTurnFixture.install(hookBinary: hook)
        let previous = ProcessInfo.processInfo.environment["ZDOTDIR"]
        var project: URL?
        defer {
            session.stop()
            if let output = session.output,
               let data = try? Data(contentsOf: fixture.log) {
                try? data.write(to: output.appendingPathComponent("hook.log"))
            }
            if let previous { setenv("ZDOTDIR", previous, 1) } else { unsetenv("ZDOTDIR") }
            for path in [fixture.root, project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        let workspace = "GrokTurnSettle"

        try await session.step("隔离启动并进入项目，打开脚本 Grok 卡片") {
            try session.launch(language: "zh-Hans", homeDirectory: fixture.home)
            _ = try await session.capture()
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            let db = try session.databaseURL()
            try Catalog.seedWorkspace(database: db, name: workspace)
            try Catalog.suppressFeatureGuide(database: db)
            project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            setenv("ZDOTDIR", fixture.shell.path, 1)
            try session.launch(language: "zh-Hans", homeDirectory: fixture.home)
            // 首页工作空间卡片压在壁纸上，OCR 读不出卡片名；按「最近访问」第一张卡的位置点击。
            // 1280×800pt 窗口里第一张卡中心约在 (宽/2 − 134, 518)。
            let home = try await session.wait(Expectation(allOf: ["最近访问"], timeout: 25))
            try await session.click(
                local: CGPoint(x: Double(home.windowFrame.width) / 2 - 134, y: 518),
                on: home
            )
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 15))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            let frame = try await session.capture()
            try await session.click(
                local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
                on: frame
            )
            try await session.selectAll()
            try await session.type("/new grok")
            try await session.shortcut(key: 36, modifiers: [])
            let ready = try await session.wait(Expectation(allOf: [GrokTurnFixture.readyBanner], timeout: 20))
            try session.save(ready, "00-grok-ready.png")
            return ready.summary
        }

        try await session.step("问答回合：提问时等待回答，终端作答并 Stop 后卡片是 Done") {
            try fixture.trigger("turn-1")
            let asked = try await session.wait(Expectation(allOf: ["GROK QUESTION ASKED", "等待回答"], timeout: 15))
            try session.save(asked, "01-question-waiting.png")
            try fixture.trigger("answer-1")
            let done = try await session.wait(Expectation(
                allOf: ["GROK TURN-1 DONE", "Done"], absent: ["运行中", "等待回答"], timeout: 15
            ))
            try session.save(done, "02-question-turn-done.png")
            return done.summary
        }

        try await session.step("问答之后的普通回合 hi：Stop 后卡片是 Done，不停在运行中") {
            try fixture.trigger("turn-2")
            let done = try await session.wait(Expectation(
                allOf: ["GROK TURN-2 DONE", "Done"], absent: ["运行中"], timeout: 15
            ))
            try session.save(done, "03-hi-turn-done.png")
            return done.summary
        }

        try await session.step("/clear 后旧会话迟到的 Stop 不把卡片拉回运行中") {
            try fixture.trigger("clear")
            _ = try await session.wait(Expectation(allOf: ["GROK CLEAR DONE"], timeout: 15))
            // 确认窗口 800ms；再留出进程探针最多 3 次延后与 12ms drain 周期，共观察 4s。
            let deadline = Date().addingTimeInterval(4)
            var frame = try await session.capture()
            while Date() < deadline {
                if frame.contains("运行中") {
                    try session.save(frame, "04-late-stop-running.png")
                    throw E2EError.product("被拒收的迟到 Stop 把卡片拉回运行中：\(frame.summary)")
                }
                try await Task.sleep(nanoseconds: 300_000_000)
                frame = try await session.capture()
            }
            try session.save(frame, "04-late-stop-not-running.png")
            return frame.summary
        }
    }
}
