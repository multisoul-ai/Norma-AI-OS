import E2EKit
import Fixtures
import Foundation

/// 真实 App：手动 Ctrl+C 后，豆包模型选择 reply_agent；同一 PTY 必须执行下一条完整指令。
@MainActor
public enum AgentVoiceResume {
    public nonisolated static let name = "agent-voice-resume"

    public static func run(_ session: Session) async throws {
        var project: URL?
        var fake: FakeStartupAgentShell.Install?
        let ark = try FakeArkServer()
        defer {
            session.stop()
            ark.stop()
            unsetenv("NORMA_DOUBAO_CHAT_URL")
            unsetenv("NORMA_DOUBAO_TTS_URL")
            if let project { try? FileManager.default.removeItem(at: project) }
            if let fake { try? FileManager.default.removeItem(at: fake.root) }
        }
        try await session.step("建立隔离项目和可中断的真实 PTY") {
            let install = try InterruptibleAgentShell.install()
            fake = install
            setenv("ZDOTDIR", install.root.path, 1)
            setenv("NORMA_E2E_LANGUAGE", "zh-Hans", 1)
            setenv("NORMA_DOUBAO_CHAT_URL", ark.chatURL, 1)
            setenv("NORMA_DOUBAO_TTS_URL", ark.ttsURL, 1)
            try session.launch(homeDirectory: install.home)
            try await BrainSupport.createWorkspace(session, name: "VoiceResume")
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离目录丢失") }
            project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "VoiceResume")
            let database = dataRoot.appendingPathComponent("app.sqlite")
            try Catalog.suppressFeatureGuide(database: database)
            for (key, value) in [
                ("voice.desktop.mode", "doubao"),
                ("voice.doubao.credential_ref", "doubao-e2e"),
                ("credential.agent-provider.doubao-e2e", "synthetic-e2e-ark-key"),
            ] { try Catalog.seedSetting(database: database, key: key, value: value) }
            try session.launch(homeDirectory: install.home)
            let home = try await session.wait(Expectation(allOf: ["VoiceResume"], timeout: 20))
            try await session.click(local: home.pick(["VoiceResume"], .bottom)?.center ?? .zero, on: home)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 15))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            try await BrainSupport.dismissGuide(session)
            let frame = try await session.capture()
            try await session.click(local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38), on: frame)
            try await session.type("ask claude INITIALTASK")
            try await session.shortcut(key: 36, modifiers: [])
            _ = try await session.wait(Expectation(allOf: ["EXECUTED 1", "运行中"], timeout: 45))
            guard InterruptibleAgentShell.messages(install) == ["INITIALTASK"] else {
                throw E2EError.product("初始指令没有恰好执行一次")
            }
            return "初始任务已在 CLI 执行"
        }

        try await session.step("审批等待期间积压一条旧消息，确认尚未写入 CLI") {
            guard let fake, let target = try session.agentDefinitions().first else {
                throw E2EError.env("目标智能体丢失")
            }
            try Data().write(to: fake.root.appendingPathComponent("request-approval"))
            _ = try await session.wait(Expectation(allOf: ["APPROVAL WAIT", "等待"], timeout: 10))
            try enqueueReply(ark, agent: target.name, id: "old-reply-1", message: "OBSOLETE DIRECTION")
            try await session.doubaoTranscript("先按旧方向修改，但保持现有审批等待。")
            let deadline = Date().addingTimeInterval(20)
            while Date() < deadline, !hasReplyStatus(ark, id: "old-reply-1", status: "queued") {
                try await Task.sleep(nanoseconds: 200_000_000)
            }
            guard hasReplyStatus(ark, id: "old-reply-1", status: "queued"),
                  InterruptibleAgentShell.messages(fake) == ["INITIALTASK"] else {
                throw E2EError.product("旧回复必须真实排队，不能提前执行")
            }
            try session.save(try await session.capture(), "voice-resume-old-queued.png")
            return "生产工具回执 queued；CLI 尚未收到旧方向"
        }

        try await session.step("用户在卡片内按 Ctrl+C，仅结束当前任务") {
            try await session.click(matching: ["EXECUTED 1"], pick: .bottom)
            // macOS keyCode 8=C、59=左 Control；通过生产键盘路径触发 ManualInterrupt。
            try await session.shortcut(key: 8, modifiers: [59])
            let interrupted = try await session.wait(Expectation(allOf: ["TASK INTERRUPTED", "已结束"], timeout: 10))
            try session.save(interrupted, "voice-resume-interrupted.png")
            return interrupted.summary
        }

        try await session.step("语音续发：同一 CLI 执行完整正文，没有重复交接模板") {
            guard let fake, let target = try session.agentDefinitions().first else {
                throw E2EError.env("目标智能体丢失")
            }
            let original = "继续刚才的修改，审批按钮应该放在外面，不要展开后才显示。保留按钮文案，不改变审批权限。"
            let legacy = "Original user request (verbatim):\nUse the full request below to preserve this assignment's background, requirements, examples and constraints. Do not take over work assigned to other agents.\n"
            let message = "把审批按钮移到卡片外层。\n\n\(legacy)背景：原型留白太多。\n\n\(legacy)背景：原型留白太多。"
            try enqueueReply(ark, agent: target.name, id: "resume-tool-1", message: message)
            try await session.doubaoTranscript(original)
            let executed = try await session.wait(Expectation(allOf: ["EXECUTED 2"], timeout: 25))
            // 回复队列每帧推进、Enter 延迟 100ms；观察 3s，覆盖至少 30 个 Enter 延迟窗口。
            try await Task.sleep(nanoseconds: 3_000_000_000)
            let received = InterruptibleAgentShell.messages(fake)
            let expected = "把审批按钮移到卡片外层。\n\n---\n\n背景：原型留白太多。\n\n---\n\n\(original)"
            guard received == ["INITIALTASK", expected],
                  FileManager.default.fileExists(atPath: fake.root.appendingPathComponent("executed-2").path) else {
                throw E2EError.product("中断后必须执行一次完整的新指令，实际：\(received)")
            }
            guard try session.agentDefinitions().count == 1 else { throw E2EError.product("不能新建第二张智能体卡片") }
            guard hasReplyStatus(ark, id: "resume-tool-1", status: "sent"), let output = session.output else {
                throw E2EError.product("新指令必须有生产工具 sent 回执")
            }
            try JSONEncoder().encode(received).write(to: output.appendingPathComponent("received-messages.json"))
            try FileManager.default.copyItem(at: fake.root.appendingPathComponent("identity.json"), to: output.appendingPathComponent("cli-identity.json"))
            let requests = try ark.chatRequestBodies.map { try JSONSerialization.jsonObject(with: $0) }
            try JSONSerialization.data(withJSONObject: requests, options: [.prettyPrinted, .sortedKeys]).write(to: output.appendingPathComponent("model-requests.json"))
            try session.save(executed, "voice-resume-executed.png")
            return "同一 CLI 只执行初始与新指令；旧方向没有迟到执行；原话/背景/约束保留且模板为零"
        }
    }

    private static func enqueueReply(_ ark: FakeArkServer, agent: String, id: String, message: String) throws {
        let arguments = try JSONSerialization.data(withJSONObject: ["agent": agent, "message": message])
        let response: [String: Any] = ["choices": [["finish_reason": "tool_calls", "message": [
            "role": "assistant", "content": NSNull(), "tool_calls": [[
                "id": id, "type": "function", "function": [
                    "name": "reply_agent", "arguments": String(decoding: arguments, as: UTF8.self),
                ],
            ]],
        ]]]]
        ark.enqueueChatResponse(String(decoding: try JSONSerialization.data(withJSONObject: response), as: UTF8.self))
    }

    private static func hasReplyStatus(_ ark: FakeArkServer, id: String, status: String) -> Bool {
        ark.chatRequestBodies.contains { data in
            guard let request = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let messages = request["messages"] as? [[String: Any]] else { return false }
            return messages.contains { message in
                guard message["role"] as? String == "tool", message["tool_call_id"] as? String == id,
                      let content = message["content"] as? String,
                      let result = try? JSONSerialization.jsonObject(with: Data(content.utf8)) as? [String: Any] else { return false }
                return result["reply_status"] as? String == status
            }
        }
    }
}
