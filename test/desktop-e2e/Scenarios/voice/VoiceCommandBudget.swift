import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 真实 App 的本地语音指令：一张卡片有两条长中文会话时仍可收到模型回复。
/// 仅替换 ASR 最终转写和模型 HTTP 服务；共享状态、预算、Hook、回复呈现均走生产路径。
@MainActor
public enum VoiceCommandBudget {
    public nonisolated static let name = "voice-command-budget"

    public static func run(_ session: Session) async throws {
        let model = try FakeArkServer()
        let shell = try BudgetAgentShell.install()
        var project: URL?
        defer {
            session.stop()
            model.stop()
            unsetenv("ZDOTDIR")
            try? FileManager.default.removeItem(at: shell)
            if let project { try? FileManager.default.removeItem(at: project) }
        }
        let workspace = "VoiceBudgetCheck"
        try await session.step("建立隔离项目，模型指向本机模拟服务") {
            setenv("ZDOTDIR", shell.path, 1)
            try session.launch(language: "en")
            try await BrainSupport.createWorkspace(session, name: workspace)
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            let database = dataRoot.appendingPathComponent("app.sqlite")
            try Catalog.suppressFeatureGuide(database: database)
            for (key, value) in [
                ("agent.provider.active", "openai"),
                ("agent.provider.openai.base_url", model.completionURL),
                ("agent.provider.openai.model", "fixture"),
                ("agent.provider.openai.credential_ref", "budget-e2e"),
                ("credential.agent-provider.budget-e2e", "synthetic-budget-key"),
                ("voice.desktop.mode", "legacy"),
                ("voice.interaction.mode", "single_voice_command"),
            ] {
                try Catalog.seedSetting(database: database, key: key, value: value)
            }
            try session.launch(language: "en")
            try await BrainSupport.openProjectWithAgents(session, count: 1)
            return "隔离项目和一张真实 Agent 卡片已打开"
        }

        try await session.step("等待 Hook 将两条中文会话写入项目") {
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            let directory = dataRoot.appendingPathComponent("desktop-canvases")
            let deadline = Date().addingTimeInterval(20)
            while true {
                let databases = (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? []
                if let database = databases.first(where: { $0.pathExtension == "sqlite" }),
                   let count = try SQLRaw.scalar(database: database,
                       "SELECT COUNT(*) FROM agent_conversation_tails WHERE length(last_user_message) = 1200 AND length(last_agent_message) = 1200"),
                   count == 1 { break }
                guard Date() < deadline else { throw E2EError.product("Hook 未产生两条各 1200 字符的会话") }
                try await Task.sleep(nanoseconds: 300_000_000)
            }
            return "两条会话各 1200 字符，合计约 7 KiB 中文正文"
        }

        try await session.step("提交语音转写，验证真实请求与界面回复") {
            try await session.voiceTranscript("VOICE_INPUT_MARKER 当前项目有什么进展？")
            let deadline = Date().addingTimeInterval(15)
            while model.completionRequests.isEmpty {
                guard Date() < deadline else { throw E2EError.product("语音指令未到达本机模型") }
                try await Task.sleep(nanoseconds: 200_000_000)
            }
            let requests = model.completionRequests
            guard requests.count == 1, let request = requests.first,
                  let body = try JSONSerialization.jsonObject(with: request) as? [String: Any],
                  let messages = body["messages"] else { throw E2EError.product("应只产生一条完整模型请求") }
            let data = try JSONSerialization.data(withJSONObject: messages, options: [.fragmentsAllowed])
            let text = String(decoding: data, as: UTF8.self)
            for marker in ["VOICE_INPUT_MARKER", "shared_context_data", "USER_TAIL:", "AGENT_TAIL:", "truncated=true"] {
                guard text.contains(marker) else { throw E2EError.product("实际请求缺少 \(marker)") }
            }
            guard request.count + 512 <= 28_672 else { throw E2EError.product("请求超出默认窗口：\(request.count)") }
            try await session.shortcut(key: 14, modifiers: [0x37])
            let reply = try await session.wait(Expectation(allOf: ["VOICE BUDGET OK"], absent: ["available 0", "Turn failed"], timeout: 15))
            try session.save(reply, "voice-command-reply.png")
            if let output = session.output { try request.write(to: output.appendingPathComponent("model-request.json")) }
            return "模型收到 \(request.count) 字节；真实对话显示 VOICE BUDGET OK"
        }
    }
}
