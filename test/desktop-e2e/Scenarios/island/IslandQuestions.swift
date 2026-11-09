import E2EKit
import Fixtures
import Foundation

@MainActor
public enum IslandQuestions {
    public nonisolated static let name = "island-questions"

    public static func run(_ session: Session) async throws {
        let fixture = try BrainQuestionFixture.install()
        // This scenario exercises manual replies without a model suggesting or consuming answers.
        try Data(#"{"enabled":false,"backend":"claude","strategy":"speed"}"#.utf8)
            .write(to: fixture.brain.appendingPathComponent("config.json"))
        var project: URL?
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR", "NORMA_E2E_NO_ACTIVATE"]
        let previous = Dictionary(uniqueKeysWithValues: keys.map { ($0, ProcessInfo.processInfo.environment[$0]) })
        defer {
            session.stop()
            if let output = session.output {
                for name in ["consumed.jsonl", "hook.log", "question-failed", "ordinary-input.jsonl"] {
                    if let data = try? Data(contentsOf: fixture.brain.appendingPathComponent(name)) { try? data.write(to: output.appendingPathComponent(name)) }
                }
            }
            for key in keys { if let value = previous[key] ?? nil { setenv(key, value, 1) } else { unsetenv(key) } }
            for path in [fixture.shell, fixture.brain, project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        setenv("ZDOTDIR", fixture.shell.path, 1)
        setenv("NORMA_BRAIN_DIR", fixture.brain.path, 1)
        try await session.step("启动隔离问答卡片，幕僚关闭时仍可手动回答") {
            try session.launch(language: "zh-Hans")
            _ = try await session.capturePixels()
            session.stop()
            guard let root = session.dataRoot else { throw E2EError.env("缺少隔离目录") }
            let db = try session.databaseURL()
            try Catalog.seedWorkspace(database: db, name: "Island questions")
            try Catalog.suppressFeatureGuide(database: db)
            project = try CanvasProjectFixture.install(dataRoot: root, workspaceName: "Island questions")
            try session.launch(language: "zh-Hans")
            try await session.visualClick("点击Island questions工作空间卡片")
            try await session.visualClick("点击E2E Project项目卡片进入画布")
            try await session.visualClick("点击画布底部命令输入框")
            try await session.type("/new claude")
            try await session.shortcut(key: 36, modifiers: [])
            _ = try await session.visualCheck([VisualCriterion(id: "ready", text: "Claude卡片终端显示Question UI fixture ready，没有启动错误")])
            return "真实PTY脚本Agent就绪，不使用LLM"
        }
        guard let island = try await session.nativeWindows().first(where: { $0.width >= 300 && $0.width < 700 && $0.height <= 60 }) else {
            throw E2EError.product("缺少独立岛窗口")
        }
        setenv("NORMA_E2E_NO_ACTIVATE", "1", 1)
        try await session.selectNativeWindow(island.number)
        try await session.step("岛内选择Blue并让原Hook消费一次") {
            try Data().write(to: fixture.brain.appendingPathComponent("question-1"))
            try await waitForAlert(session, number: island.number)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "direct-question", text: "与原型一致：Choose color 1首次出现即展示Blue和Green选项以及自由回复输入框；发送回复按钮与输入框同行；没有先点击回答或自由回复才能显示表单的中间步骤")
            ]), "00-direct-question.png")
            try await session.visualClick("在Choose color 1下选择Blue单选项")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "choice", text: "Blue为选中的单选项，Green未选中；发送回复按钮可用，原题Choose color 1可见")
            ]), "01-choice.png")
            try await session.visualClick("点击发送回复按钮")
            try await waitForAnswer(fixture.brain, cycle: 1, expected: "Blue")
            try session.save(try await session.visualSnapshotCheck([
                VisualCriterion(id: "submitted", text: "岛显示回复已提交到原问题的成功反馈，没有未能确认送达的提示，也没有仍可发送的Choose color 1表单")
            ]), "01-submitted.png")
            try await waitForCompact(session, number: island.number)
            return "原问题1只消费Blue一次"
        }
        try await session.step("岛输入自由文本与空格，原Hook收到完整答案") {
            try Data().write(to: fixture.brain.appendingPathComponent("question-2"))
            try await waitForAlert(session, number: island.number)
            try await session.visualClick("点击自由回复输入框")
            try await session.type("Cobalt blue")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "custom", text: "Choose color 2的自由回复输入框完整显示Cobalt blue，包含空格；发送回复按钮可用；新题上方没有上一题的回复已提交文案")
            ]), "02-custom-reply.png")
            try await session.visualClick("点击发送回复按钮")
            try await waitForAnswer(fixture.brain, cycle: 2, expected: "Cobalt blue")
            try session.save(try await session.visualSnapshotCheck([
                VisualCriterion(id: "submitted", text: "岛显示回复已提交到原问题的成功反馈，没有未能确认送达的提示，也没有仍可发送的Choose color 2表单")
            ]), "02-submitted.png")
            try await waitForCompact(session, number: island.number)
            guard !FileManager.default.fileExists(atPath: fixture.brain.appendingPathComponent("ordinary-input.jsonl").path) else {
                throw E2EError.product("原题回复错误地变成普通PTY任务输入")
            }
            return "自由回复通过生产授权与Hook回执，未派发新任务"
        }
    }

    /// 瞬时截图不证明计时器生效；须另外证明同一原生窗口恢复胶囊尺寸。
    private static func waitForCompact(_ session: Session, number: Int) async throws {
        for _ in 0..<60 {
            if try await session.nativeWindows().contains(where: {
                $0.number == number && $0.width < 400 && $0.height <= 60
            }) { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw E2EError.product("确认反馈未自动收回为Compact")
    }

    private static func waitForAlert(_ session: Session, number: Int) async throws {
        for _ in 0..<150 {
            if try await session.nativeWindows().contains(where: { $0.number == number && $0.height > 60 }) { return }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("提问未使岛弹出")
    }
    private static func waitForAnswer(_ root: URL, cycle: Int, expected: String) async throws {
        let path = root.appendingPathComponent("consumed.jsonl")
        for _ in 0..<150 {
            if let data = try? String(contentsOf: path, encoding: .utf8) {
                let rows = try data.split(separator: "\n").map { try JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any] }
                    .compactMap { $0 }.filter { $0["cycle"] as? Int == cycle }
                if let row = rows.first {
                    guard rows.count == 1, row["tool_use_id"] as? String == "question-ui-tool-\(cycle)",
                        (row["answers"] as? [String: String])?["Choose color \(cycle)"] == expected else {
                        throw E2EError.product("错题、重复或答案丢失：\(rows)")
                    }
                    return
                }
            }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("原Hook未确认第\(cycle)题")
    }
}
