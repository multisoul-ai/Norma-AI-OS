import E2EKit
import Fixtures
import Foundation

/// OpenCode 不发 SessionStart。第一次粘贴不确认，15 秒后需要你；重新投递仍是同一张卡。
@MainActor
public enum StaffOpenCodeDispatch {
    public nonisolated static let name = "staff-opencode-dispatch"

    public static func run(_ session: Session) async throws {
        let root = try StaffFixture.install()
        var project: URL?
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR", "NORMA_E2E_STAFF_AGENT"]
        let previous = Dictionary(uniqueKeysWithValues: keys.map { ($0, ProcessInfo.processInfo.environment[$0]) })
        defer {
            session.stop()
            if let output = session.output {
                try? FileManager.default.copyItem(at: root, to: output.appendingPathComponent("staff-fixture-evidence"))
            }
            for key in keys { if let value = previous[key] ?? nil { setenv(key, value, 1) } else { unsetenv(key) } }
            for path in [root, project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        setenv("ZDOTDIR", root.path, 1)
        setenv("NORMA_BRAIN_DIR", root.path, 1)
        setenv("NORMA_E2E_STAFF_AGENT", "OpenCode", 1)
        try session.launch(language: "zh-Hans")
        _ = try await session.capturePixels()
        let database = try session.databaseURL()
        guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离数据根目录") }
        session.stop()
        try Catalog.seedWorkspace(database: database, name: "Staff M1")
        try Catalog.suppressFeatureGuide(database: database)
        project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "Staff M1")
        try session.launch(language: "zh-Hans")
        try await session.visualClick("点击 Staff M1 工作空间卡片进入")
        try await session.visualClick("点击 E2E Project 项目进入画布")
        _ = try await session.capturePixels()
        session.stop()
        try insertOpenCodeAgent(dataRoot)
        try session.launch(language: "zh-Hans")

        try await session.step("进入幕僚并采用 OpenCode 提议") {
            try await session.visualClick("点击 Staff M1 工作空间卡片进入")
            try await session.visualClick("点击 E2E Project 项目进入画布")
            try await session.visualClick("点击顶部幕僚或 Staff 按钮进入幕僚模式")
            try await session.visualClick("点击中央任务对话底部的消息输入框")
            try await session.type("OPENCODE-DISPATCH 请创建验收文件")
            try await session.shortcut(key: 36, modifiers: [])
            try await waitFor("提议保存") {
                let proposals = try task(root)["proposals"] as? [[String: Any]] ?? []
                return !proposals.isEmpty
            }
            try await session.visualClick("点击提议卡的采用并派发按钮")
            try await waitFor("OpenCode 卡片已启动且没有 SessionStart") {
                let starts = try StaffFixture.records(root, "starts.jsonl")
                return starts.count == 1 && starts[0]["session_start"] as? Bool == false
            }
            return "OpenCode Worker 已启动，未发送 SessionStart"
        }

        try await session.step("第一次粘贴未确认，15 秒后需要你") {
            let before = try task(root)
            let workers = before["workers"] as? [[String: Any]] ?? []
            guard workers.count == 1 else { throw E2EError.product("派发时 Worker 数量不是 1：\(workers)") }
            let card = workers[0]["card_id"] as? Int
            let dispatch = workers[0]["dispatch_id"] as? String
            try await waitFor("送达超时转为需要你") {
                let worker = (try task(root)["workers"] as? [[String: Any]])?.first
                return worker?["state"] as? String == "NeedsAttention"
            }
            let hooks = try StaffFixture.records(root, "hooks.jsonl")
            guard !hooks.contains(where: { $0["event"] as? String == "SessionStart" || $0["event"] as? String == "UserPromptSubmit" }) else {
                throw E2EError.product("超时前不应有 SessionStart 或 UserPromptSubmit：\(hooks)")
            }
            guard try StaffFixture.records(root, "received.jsonl").count == 1 else {
                throw E2EError.product("宽限后应恰好粘贴一次")
            }
            try await session.scroll(xFraction: 0.60, yFraction: 0.55, deltaY: -700)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "needs-you", text: "任务里只有一张 Worker 卡，状态是需要你，不是执行失败或执行中。对话里有重新投递和在画布中查看两个按钮。")
            ]), "01-needs-you.png")
            try await session.visualClick("点击重新投递按钮")
            try await waitFor("第二次粘贴被 UserPromptSubmit 确认") {
                let worker = (try task(root)["workers"] as? [[String: Any]])?.first
                let confirmed = try StaffFixture.records(root, "hooks.jsonl").filter { $0["event"] as? String == "UserPromptSubmit" }.count == 1
                return confirmed && worker?["prompt_delivered"] as? Bool == true
            }
            let after = try task(root)
            let afterWorkers = after["workers"] as? [[String: Any]] ?? []
            guard afterWorkers.count == 1,
                  afterWorkers[0]["card_id"] as? Int == card,
                  afterWorkers[0]["dispatch_id"] as? String == dispatch,
                  try StaffFixture.records(root, "received.jsonl").count == 2 else {
                throw E2EError.product("重新投递必须留在原卡：\(afterWorkers)")
            }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "running", text: "仍是同一张 Worker 卡，状态变成执行中。没有第二张 Worker，也没有执行失败。")
            ]), "02-running.png")
            return "重新投递确认到原卡，Worker 仍为一张"
        }
    }

    /// Agent 定义在项目库，不在 app.sqlite。先让 App 建库，停止后再写入 Leo / OpenCode。
    private static func insertOpenCodeAgent(_ dataRoot: URL) throws {
        let directory = dataRoot.appendingPathComponent("desktop-project-definitions", isDirectory: true)
        let files = (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? []
        let databases = files.filter { $0.pathExtension == "sqlite" }
        guard databases.count == 1, let database = databases.first else {
            throw E2EError.product("项目 Agent 定义库应恰好一个，实际 \(databases.count) 个")
        }
        try SQLRaw.execute(database: database, statements: [
            ("INSERT INTO agent_definitions (name, name_key, speech_alias, runtime_profile, workflow_description, enabled, sort_order, origin, created_at_unix, updated_at_unix) VALUES ('Leo', 'leo', '利奥', 'opencode', '', 1, 0, 'explicit', 1, 1)", []),
        ])
    }

    private static func task(_ root: URL) throws -> [String: Any] {
        try StaffFixture.tasks(root).first ?? [:]
    }

    private static func waitFor(_ description: String, _ check: () throws -> Bool) async throws {
        let deadline = Date().addingTimeInterval(45)
        while Date() < deadline {
            if try check() { return }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("等待超时：\(description)")
    }
}
