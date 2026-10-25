import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 豆包语音按卡片名/语音别名派任务：由代码直呼，未命中名字才回落到模型。
/// 构造：隔离工作空间 + 项目 + 两张 Codex 卡片（假 CLI）；豆包凭据是假 Key，
/// Chat/TTS 指向本机假方舟（`/chat` 计数）；转写经控制通道 `doubao_transcript` 注入。
/// 预期：卡片名与别名各自派给对应的 Agent（`agent_dispatch_tasks` 的 definition_id 与正文），
/// 且假方舟 Chat 0 次；“今天天气”和“名字后紧跟字母”都不派任务、Chat 各增 1 次。
@MainActor
public enum VoiceDoubaoDirect {
    public nonisolated static let name = "voice-doubao-direct"
    private static let workspace = "DBDIRECT"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fakeShell: URL?
        let ark = try FakeArkServer()
        defer {
            ark.stop()
            session.stop()
            unsetenv("NORMA_DOUBAO_CHAT_URL")
            unsetenv("NORMA_DOUBAO_TTS_URL")
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
        }

        try await session.step("建立含项目的工作空间，预置豆包假凭据并指向本机假方舟") {
            let shell = try FakeAgentShell.install()
            fakeShell = shell
            setenv("ZDOTDIR", shell.path, 1)
            setenv("NORMA_DOUBAO_CHAT_URL", ark.chatURL, 1)
            setenv("NORMA_DOUBAO_TTS_URL", ark.ttsURL, 1)
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["Create Workspace", "Create your first"], timeout: 25))
            try await session.act("打开创建对话框", attempts: 4, expect: Expectation(anyOf: ["工作空间名称", "Workspace name"], timeout: 3.5)) {
                let launch = try await session.capture()
                try await session.click(local: CGPoint(x: Double(launch.windowFrame.width) / 2, y: 515), on: launch)
            }
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
            let database = dataRoot.appendingPathComponent("app.sqlite")
            try Catalog.suppressFeatureGuide(database: database)
            for (key, value) in [
                ("voice.desktop.mode", "doubao"),
                ("voice.doubao.credential_ref", "doubao-e2e"),
                ("credential.agent-provider.doubao-e2e", "synthetic-e2e-ark-key"),
            ] {
                try Catalog.seedSetting(database: database, key: key, value: value)
            }
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return "假方舟 127.0.0.1:\(ark.port)；豆包凭据已写入隔离库"
        }

        try await session.step("进入项目并创建两张 Codex 卡片") {
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search workspaces") {
                try await session.click(local: CGPoint(x: Double(launch.windowFrame.width) - 111, y: 540), on: launch)
                _ = try await session.wait(Expectation(absent: ["搜索工作空间", "Search workspaces"], timeout: 12))
            }
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            for _ in 0..<2 {
                let frame = try await session.capture()
                try await session.click(local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38), on: frame)
                try await session.selectAll()
                try await session.type("/new codex")
                try await session.shortcut(key: 36, modifiers: [])
                try await Task.sleep(nanoseconds: 2_500_000_000)
            }
            let definitions = try await eventually(timeout: 10) {
                let rows = try session.agentDefinitions().filter { $0.profile == "codex" }
                return rows.count == 2 ? rows : nil
            }
            return "名册：\(definitions.map { "\($0.name)/\($0.alias)" })"
        }

        let definitions = try session.agentDefinitions().filter { $0.profile == "codex" }
        guard definitions.count == 2, !definitions[0].alias.isEmpty, !definitions[1].alias.isEmpty else {
            throw E2EError.product("应有两个带语音别名的 Codex 定义：\(definitions)")
        }
        let (first, second) = (definitions[0], definitions[1])

        try await session.step("按卡片名派任务：派给对应 Agent，且没有问模型") {
            try await session.doubaoTranscript("\(first.name)，新任务：整理 README 目录")
            let rows = try await eventually(timeout: 20) {
                let rows = try session.dispatchTasks()
                return rows.isEmpty ? nil : rows
            }
            guard rows.count == 1, rows[0].definitionId == first.id, rows[0].taskText.contains("整理 README 目录") else {
                throw E2EError.product("应只派给 \(first.name)(\(first.id))：\(rows)")
            }
            guard ark.chatRequests == 0 else { throw E2EError.product("命中卡片名不应请求模型，Chat 次数 \(ark.chatRequests)") }
            return "任务→\(first.name)，Chat=0"
        }

        try await session.step("按语音别名派任务：映射到另一个 Agent") {
            try await session.doubaoTranscript("\(second.alias)，新任务：补充 CHANGELOG 说明")
            let rows = try await eventually(timeout: 20) {
                let rows = try session.dispatchTasks()
                return rows.count >= 2 ? rows : nil
            }
            guard rows.count == 2, rows[1].definitionId == second.id, rows[1].taskText.contains("补充 CHANGELOG 说明") else {
                throw E2EError.product("别名 \(second.alias) 应派给 \(second.name)(\(second.id))：\(rows)")
            }
            guard ark.chatRequests == 0 else { throw E2EError.product("命中别名不应请求模型，Chat 次数 \(ark.chatRequests)") }
            return "任务→\(second.name)（别名 \(second.alias)），Chat=0"
        }

        try await session.step("未命中名字与名字后无分隔符：不派任务，回落到模型") {
            try await session.doubaoTranscript("今天天气怎么样")
            _ = try await eventually(timeout: 20) { ark.chatRequests >= 1 ? true : nil }
            try await session.doubaoTranscript("\(first.name)ine 的配置怎么写")
            _ = try await eventually(timeout: 20) { ark.chatRequests >= 2 ? true : nil }
            try await Task.sleep(nanoseconds: 1_500_000_000)
            let rows = try session.dispatchTasks()
            guard rows.count == 2 else { throw E2EError.product("回落到模型的两句话不应新增派发：\(rows)") }
            guard ark.chatRequests == 2 else { throw E2EError.product("两句未命中应各请求一次模型，实际 \(ark.chatRequests)") }
            return "Chat=2，派发仍为 2 条"
        }
    }

    private static func eventually<T>(timeout: TimeInterval, _ probe: () throws -> T?) async throws -> T {
        let deadline = Date().addingTimeInterval(timeout)
        while true {
            if let value = try probe() { return value }
            guard Date() < deadline else { throw E2EError.product("\(Int(timeout)) 秒内条件未满足") }
            try await Task.sleep(nanoseconds: 300_000_000)
        }
    }
}
