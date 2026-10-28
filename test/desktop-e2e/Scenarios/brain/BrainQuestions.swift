import E2EKit
import Fixtures
import Foundation

/// 真实 Norma UI + 生产 Hook 桥；Agent 与幕僚使用确定性脚本，不计入真实 Provider 验收。
@MainActor
public enum BrainQuestions {
    public nonisolated static let name = "brain-questions-ui"

    public static func run(_ session: Session) async throws {
        var shell: URL?
        var brain: URL?
        var project: URL?
        defer {
            session.stop()
            if let brain, let output = session.output {
                for name in ["consumed.jsonl", "question-commands.jsonl", "question-prompt.log", "hook.log", "question-brain-error", "question-failed", "ordinary-input.jsonl"] {
                    if let data = try? Data(contentsOf: brain.appendingPathComponent(name)) {
                        try? data.write(to: output.appendingPathComponent(name))
                    }
                }
            }
            for path in [shell, brain, project].compactMap({ $0 }) {
                try? FileManager.default.removeItem(at: path)
            }
        }
        try await session.step("建立隔离项目、脚本化问答Agent与幕僚") {
            let fixture = try BrainQuestionFixture.install()
            shell = fixture.shell; brain = fixture.brain
            setenv("ZDOTDIR", fixture.shell.path, 1)
            setenv("NORMA_BRAIN_DIR", fixture.brain.path, 1)
            setenv("NORMA_E2E_LANGUAGE", "zh-Hans", 1)
            try session.launch()
            try await BrainSupport.createWorkspace(session, name: "Questions")
            session.stop()
            guard let root = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            project = try CanvasProjectFixture.install(dataRoot: root, workspaceName: "Questions")
            let theme = ProcessInfo.processInfo.environment["NORMA_QUESTION_UI_THEME"] ?? "sapphire-night"
            guard ["paper-blue", "sapphire-night"].contains(theme) else { throw E2EError.env("未知问答测试主题") }
            try Catalog.seedSetting(database: try session.databaseURL(), key: "appearance.theme.v1", value: theme)
            try session.launch()
            try await BrainSupport.openProjectWithAgents(session, count: 1)
            try await BrainSupport.openOverview(session)
            return "原卡片已创建；问答尚未开始"
        }
        guard let brain else { throw E2EError.env("问答夹具目录丢失") }
        try await session.step("采用幕僚建议：原Hook收到Green，消费后问题消失") {
            try Data().write(to: brain.appendingPathComponent("question-1"))
            let before = try await session.wait(Expectation(allOf: ["Choose color 1", "采用答案", "Green"], timeout: 45))
            try session.save(before, "01-question-suggestion.png")
            guard try consumed(brain).isEmpty else { throw E2EError.product("用户采用前已发送答案") }
            try await session.click(matching: ["采用答案"], pick: .top)
            try await waitForAnswer(brain, cycle: 1, expected: "Green")
            let after = try await session.wait(Expectation(absent: ["Choose color 1"], timeout: 10))
            try session.save(after, "02-question-consumed.png")
            return "采用只回答原题一次，消费后解除待答"
        }
        try await session.step("编辑建议为Blue，提交完整原题答案") {
            try Data().write(to: brain.appendingPathComponent("question-2"))
            _ = try await session.wait(Expectation(allOf: ["Choose color 2", "采用答案"], timeout: 45))
            try await session.click(matching: ["编辑"], pick: .top)
            _ = try await session.wait(Expectation(allOf: ["提交答案", "Blue"], timeout: 6))
            try await session.click(matching: ["Blue"], pick: .top)
            // Let the selected control paint before capturing its visual state.
            try await Task.sleep(nanoseconds: 300_000_000)
            let edited = try await session.capture()
            try session.save(edited, "03-question-edited.png")
            // With an expanded question, all four groups exceed a short drawer's height.
            // Scroll its outer padding so the event does not target an inner group scroller.
            try await session.resize(width: edited.windowFrame.width, height: 640)
            try await session.scroll(local: CGPoint(x: 2, y: 250), pixels: -900, on: try await session.capture())
            let bottom = try await session.wait(Expectation(allOf: ["写下一个想法"], timeout: 6))
            try session.save(bottom, "06-short-overview-bottom.png")
            try await session.resize(width: edited.windowFrame.width, height: edited.windowFrame.height)
            try await session.scroll(local: CGPoint(x: 2, y: 250), pixels: 900, on: try await session.capture())
            _ = try await session.wait(Expectation(allOf: ["Choose color 2", "提交答案"], timeout: 6))
            try await session.click(matching: ["提交答案"], pick: .top)
            try await waitForAnswer(brain, cycle: 2, expected: "Blue")
            _ = try await session.wait(Expectation(absent: ["Choose color 2"], timeout: 10))
            return "编辑值取代建议值，原Hook消费Blue"
        }
        try await session.step("明确委托当前问题，幕僚只回答一次，不派发任务") {
            try Data().write(to: brain.appendingPathComponent("question-3"))
            _ = try await session.wait(Expectation(allOf: ["Choose color 3", "交给幕僚回答"], timeout: 45))
            try await session.click(matching: ["交给幕僚回答"], pick: .top)
            try await waitForAnswer(brain, cycle: 3, expected: "Green")
            let after = try await session.wait(Expectation(absent: ["Choose color 3"], timeout: 10))
            try session.save(after, "04-delegated-consumed.png")
            return "一次明确委托对应一次原题回答"
        }
        try await session.step("自定义答案逐字输入，空格不被概览导航吞掉") {
            try Data().write(to: brain.appendingPathComponent("question-4"))
            _ = try await session.wait(Expectation(allOf: ["Choose color 4", "采用答案"], timeout: 45))
            try await session.click(matching: ["编辑"], pick: .top)
            _ = try await session.wait(Expectation(allOf: ["自定义答案", "提交答案"], timeout: 6))
            try await session.click(matching: ["自定义答案"], pick: .top)
            _ = try await session.wait(Expectation(allOf: ["输入答案"], timeout: 6))
            try await session.click(matching: ["输入答案"], pick: .top)
            // type traverses KeyDown + text input + KeyUp, never clipboard insertion.
            try await session.type("Cobalt blue")
            let edited = try await session.wait(Expectation(allOf: ["Cobalt blue", "提交答案"], timeout: 6))
            try session.save(edited, "05-custom-question-edited.png")
            try await session.click(matching: ["提交答案"], pick: .top)
            try await waitForAnswer(brain, cycle: 4, expected: "Cobalt blue")
            _ = try await session.wait(Expectation(absent: ["Choose color 4"], timeout: 10))
            let ordinary = brain.appendingPathComponent("ordinary-input.jsonl")
            guard !FileManager.default.fileExists(atPath: ordinary.path) else {
                throw E2EError.product("回答原题时出现普通PTY任务输入")
            }
            guard try consumed(brain).count == 4 else { throw E2EError.product("四道题未恰好消费四次") }
            return "自由文本含空格被原Hook精确消费，四轮均无新任务输入"
        }
    }

    private static func consumed(_ root: URL) throws -> [[String: Any]] {
        let path = root.appendingPathComponent("consumed.jsonl")
        guard FileManager.default.fileExists(atPath: path.path) else { return [] }
        return try String(contentsOf: path, encoding: .utf8).split(separator: "\n").map { line in
            guard let item = try JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any] else {
                throw E2EError.product("消费记录不是有效JSON")
            }
            return item
        }
    }

    private static func waitForAnswer(_ root: URL, cycle: Int, expected: String) async throws {
        let deadline = Date().addingTimeInterval(30)
        while Date() < deadline {
            let records = try consumed(root).filter { $0["cycle"] as? Int == cycle }
            if !records.isEmpty {
                guard records.count == 1,
                      records[0]["tool_use_id"] as? String == "question-ui-tool-\(cycle)",
                      (records[0]["answers"] as? [String: String])?["Choose color \(cycle)"] == expected else {
                    throw E2EError.product("答案重复、错题或内容错误：\(records)")
                }
                return
            }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("原Hook未消费第\(cycle)题答案")
    }
}
