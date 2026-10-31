import E2EKit
import Fixtures
import Foundation

/// Cmd+K 聊天路由：普通消息交幕僚且回复回到 Dock；`ask claude …` 新建卡片并送达任务；
/// `ask <名称> …` 给已有卡片再发一次；未知名称报错且不建卡。
/// 目标 Agent 收到的内容以假 claude 的 stdin 记录为准，不以界面提示为完成证据。
@MainActor
public enum BrainDockChat {
    public nonisolated static let name = "brain-dock-chat"
    public nonisolated static let englishName = "brain-dock-chat-en"

    public static func run(_ session: Session, english: Bool = false) async throws {
        var projectRoot: URL?
        var fakeShell: URL?
        var brainDir: URL?
        defer {
            if let brainDir, let output = session.output {
                for file in ["received.jsonl", "user-requests.log"] {
                    try? FileManager.default.copyItem(
                        at: brainDir.appendingPathComponent(file), to: output.appendingPathComponent("dock-chat-\(file)"))
                }
            }
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
            if let brainDir { try? FileManager.default.removeItem(at: brainDir) }
        }
        let workspace = "DockChatCheck"

        try await session.step("建立隔离工作空间，开启脚本化幕僚") {
            let cli = URL(fileURLWithPath: session.appPath).deletingLastPathComponent().appendingPathComponent("norma")
            guard FileManager.default.isExecutableFile(atPath: cli.path) else {
                throw E2EError.env("缺少配套 norma CLI；请先 cargo build -p norma")
            }
            fakeShell = try BrainFixture.installFakeAgents()
            brainDir = try BrainDockChatFixture.installBrainDirectory()
            setenv("ZDOTDIR", fakeShell!.path, 1)
            setenv("NORMA_BRAIN_DIR", brainDir!.path, 1)
            setenv("NORMA_E2E_LANGUAGE", english ? "en" : "zh-Hans", 1)
            try session.launch()
            try await BrainSupport.createWorkspace(session, name: workspace)
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return try session.prove(names: [workspace], areaCount: 1, linkedTo: workspace).summary
        }

        try await session.step("进入项目") {
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

        let first = "FIRSTALPHA 整理安装步骤"
        try await session.step("ask claude 新建卡片，任务完整送达恰好一次") {
            try await dock("ask claude \(first)", session)
            try await waitForReceived(brainDir, containing: "FIRSTALPHA", count: 1, timeout: 60)
            try await Task.sleep(nanoseconds: 3_000_000_000)
            let lines = try received(brainDir)
            guard lines.filter({ $0.contains(first) }).count == 1 else {
                throw E2EError.product("新建卡片后任务未恰好送达一次：\(lines)")
            }
            return "收到：\(lines)"
        }

        try await session.step("ask 已有名称：同一张卡再收到一次，不新建卡") {
            let database = try definitions(session)
            let before = try SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM agent_definitions") ?? -1
            guard let agent = try SQLRaw.text(database: database, "SELECT name FROM agent_definitions ORDER BY id DESC LIMIT 1") else {
                throw E2EError.product("没有找到刚创建的 Agent")
            }
            try await dock("ask \(agent) SECONDBETA 补测试", session)
            try await waitForReceived(brainDir, containing: "SECONDBETA", count: 1, timeout: 60)
            try await Task.sleep(nanoseconds: 3_000_000_000)
            let lines = try received(brainDir)
            guard lines.filter({ $0.contains("SECONDBETA 补测试") }).count == 1 else {
                throw E2EError.product("已有 Agent 未恰好收到一次：\(lines)")
            }
            let after = try SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM agent_definitions") ?? -2
            guard after == before else { throw E2EError.product("ask 已有名称不应新建 Agent：\(before)→\(after)") }
            return "名称 \(agent)，Agent 数 \(after)"
        }

        try await session.step("ask 未知名称报错且不建卡") {
            let database = try definitions(session)
            let before = try SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM agent_definitions") ?? -1
            let linesBefore = try received(brainDir).count
            try await dock("ask Nobodyxyz hello", session)
            let frame = try await session.wait(Expectation(
                anyOf: english ? ["Nobodyxyz"] : ["Nobodyxyz"], timeout: 10))
            try session.save(frame, "dock-chat-unknown.png")
            guard frame.contains(english ? "No agent named" : "没有名为") else {
                throw E2EError.product("未知名称没有显示本地化错误：\(frame.summary)")
            }
            let after = try SQLRaw.scalar(database: database, "SELECT COUNT(*) FROM agent_definitions") ?? -2
            guard after == before, try received(brainDir).count == linesBefore else {
                throw E2EError.product("未知名称不应建卡或派发：\(before)→\(after)")
            }
            return "Agent 数保持 \(after)"
        }

        try await session.step("普通消息交幕僚，幕僚回复回到 Dock") {
            try await dock("今天进展如何 QQ7", session)
            guard let brainDir else { throw E2EError.env("大脑目录丢失") }
            let log = brainDir.appendingPathComponent("user-requests.log")
            let deadline = Date().addingTimeInterval(60)
            while Date() < deadline, !((try? String(contentsOf: log, encoding: .utf8))?.contains("QQ7") ?? false) {
                try await Task.sleep(nanoseconds: 500_000_000)
            }
            guard (try? String(contentsOf: log, encoding: .utf8))?.contains("QQ7") ?? false else {
                throw E2EError.product("幕僚没有收到普通消息")
            }
            let reply = try await session.wait(Expectation(allOf: ["幕僚收到"], timeout: 30))
            try session.save(reply, "dock-chat-brain-reply.png")
            return "幕僚回复已显示在 Dock：\(reply.summary)"
        }
    }

    /// 项目的 Agent 定义库（每个项目一个，本场景只有一个）。
    private static func definitions(_ session: Session) throws -> URL {
        guard let root = session.dataRoot?.appendingPathComponent("desktop-project-definitions"),
              let file = try FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)
                .first(where: { $0.pathExtension == "sqlite" })
        else { throw E2EError.env("找不到项目 Agent 定义库") }
        return file
    }

    private static func dock(_ text: String, _ session: Session) async throws {
        let frame = try await session.capture()
        try await session.click(
            local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38), on: frame)
        try await session.selectAll()
        try await session.type(text)
        try await session.shortcut(key: 36, modifiers: [])
        try await Task.sleep(nanoseconds: 1_500_000_000)
    }

    private static func received(_ dir: URL?) throws -> [String] {
        guard let dir else { throw E2EError.env("大脑目录丢失") }
        let text = (try? String(contentsOf: dir.appendingPathComponent("received.jsonl"), encoding: .utf8)) ?? ""
        return text.split(separator: "\n").map(String.init)
    }

    private static func waitForReceived(_ dir: URL?, containing needle: String, count: Int, timeout: TimeInterval) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if try received(dir).filter({ $0.contains(needle) }).count >= count { return }
            try await Task.sleep(nanoseconds: 500_000_000)
        }
        throw E2EError.product("\(Int(timeout)) 秒内目标 Agent 没有收到含 \(needle) 的任务：\(try received(dir))")
    }
}
