import E2EKit
import Fixtures
import Foundation

/// 真实 App：幕僚在设置为「自动」时给同一个 Agent 派后续活，卡片带标记并可撤回；高风险降级为提议并说明原因。
@MainActor
public enum StaffAutoDispatch {
    public nonisolated static let name = "staff-auto-dispatch"
    public nonisolated static let englishName = "staff-auto-dispatch-en"

    public static func run(_ session: Session, english: Bool = false) async throws {
        let root = try StaffFixture.install()
        var project: URL?
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR", "CLAUDE_CONFIG_DIR", "NORMA_E2E_STAFF_QUESTIONS", "NORMA_E2E_STAFF_ASSIGN"]
        let previous = Dictionary(uniqueKeysWithValues: keys.map { ($0, ProcessInfo.processInfo.environment[$0]) })
        defer {
            session.stop()
            for key in keys { if let value = previous[key] ?? nil { setenv(key, value, 1) } else { unsetenv(key) } }
            for path in [root, project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        // 设置里的「后续任务」为「自动」，幕僚才可能直接派发。
        try Data(#"{"enabled":true,"backend":"claude","strategy":"speed","speak":false,"summary":"off","autonomy":{"assign_followups":true}}"#.utf8).write(to: root.appendingPathComponent("config.json"))
        setenv("ZDOTDIR", root.path, 1)
        setenv("NORMA_BRAIN_DIR", root.path, 1)
        setenv("CLAUDE_CONFIG_DIR", root.appendingPathComponent("claude-config").path, 1)
        unsetenv("NORMA_E2E_STAFF_QUESTIONS")
        setenv("NORMA_E2E_STAFF_ASSIGN", "1", 1)
        let language = english ? "en" : "zh-Hans"
        try session.launch(language: language)
        _ = try await session.capturePixels()
        let database = try session.databaseURL()
        guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离数据根目录") }
        session.stop()
        try Catalog.seedWorkspace(database: database, name: "Staff Auto")
        try Catalog.suppressFeatureGuide(database: database)
        try VoiceConfigurationFixture.theme(database: database, id: english ? "sapphire-night" : "ink-black")
        project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "Staff Auto")
        try session.launch(language: language)

        let adopt = english ? "Adopt and dispatch" : "采用并派发"
        let withdraw = english ? "Withdraw" : "撤回"

        try await session.step("准备一个可派活的 Agent 并让幕僚提出第一条任务") {
            try await session.visualClick("点击 Staff Auto 工作空间卡片进入")
            try await session.visualClick("点击 E2E Project 项目进入画布")
            try await session.visualClick("点击底部命令输入框")
            try await session.type("/new claude")
            try await session.shortcut(key: 36, modifiers: [])
            try await waitFor("真实 Agent 启动") { try StaffFixture.records(root, "starts.jsonl").count == 1 }
            try await session.visualClick("点击底栏最左侧的浅紫发圆形头像进入幕僚模式")
            try await session.visualClick("点击中央任务对话底部的消息输入框，不是左侧搜索框")
            try await session.type("AUTO-DISPATCH 请生成验收文件")
            try await session.shortcut(key: 36, modifiers: [])
            try await waitFor("第一条提议保存") { try !(task(root)?["proposals"] as? [[String: Any]] ?? []).isEmpty }
            try await session.visualClick("点击提议卡的\(adopt)按钮")
            return "第一条由用户采用并派发"
        }

        try await session.step("第一位 Worker 完成后，幕僚自动给同一 Agent 派后续") {
            try await waitFor("幕僚自动派发第二条") {
                let proposals = try task(root)?["proposals"] as? [[String: Any]] ?? []
                return proposals.count == 2 && proposals[1]["auto"] as? Bool == true && proposals[1]["dispatch_id"] is String
            }
            let current = try task(root) ?? [:]
            let proposals = current["proposals"] as? [[String: Any]] ?? []
            let workers = current["workers"] as? [[String: Any]] ?? []
            guard workers.count == 2, workers[0]["agent"] as? String == workers[1]["agent"] as? String,
                  proposals[1]["approved"] as? Bool == true, current["auto_dispatches"] as? Int == 1 else {
                throw E2EError.product("自动派发的 Worker、批准状态或计数不符：\(workers), \(proposals)")
            }
            try await session.scroll(xFraction: 0.48, yFraction: 0.55, deltaY: -3000)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "auto-card", text: "第二条「补一条测试」的提议折叠成一行已派发卡，状态文字是「✓ 幕僚已派发」/Dispatched by Staff（不是普通的已派发），卡片右侧有\(withdraw)按钮；第一条仍显示普通的已派发。")
            ]), "01-auto-card.png")
            return "第二条为幕僚自动派发，卡片有标记和撤回按钮"
        }

        try await session.step("撤回：Worker 记为被中断，卡片关闭") {
            try await session.visualClick("点击「幕僚已派发」卡片右侧的\(withdraw)按钮")
            try await waitFor("Worker 被记为中断") {
                let workers = try task(root)?["workers"] as? [[String: Any]] ?? []
                return workers.count == 2 && workers[1]["outcome"] as? String == "Interrupted"
            }
            try Data().write(to: root.appendingPathComponent("release-worker"))
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "withdrawn", text: "第二条已派发卡不再有\(withdraw)按钮；对话里出现一条被中断的 Worker 结果，写明你已撤回这项任务；不是错误提示。")
            ]), "02-withdrawn.png")
            return "撤回后 Worker 为 Interrupted，撤回按钮消失"
        }

        try await session.step("高风险后续降级为提议并说明原因") {
            try await session.visualClick("点击中央底部消息输入框")
            try await session.type("ASSIGN-RISK 请收尾发布")
            try await session.shortcut(key: 36, modifiers: [])
            try await waitFor("降级提议保存") {
                let proposals = try task(root)?["proposals"] as? [[String: Any]] ?? []
                return proposals.count == 3 && (proposals[2]["downgrade"] as? String ?? "").hasPrefix("risk:")
            }
            let proposals = try task(root)?["proposals"] as? [[String: Any]] ?? []
            guard proposals[2]["approved"] as? Bool == false, proposals[2]["auto"] as? Bool == false,
                  (try task(root)?["workers"] as? [[String: Any]] ?? []).count == 2 else {
                throw E2EError.product("高风险后续不应批准或派发：\(proposals)")
            }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "downgrade", text: "出现「推送并发布」建议卡，状态是需要你/Your decision，理由下方有一行橙色说明，意思是可能涉及高风险操作需要你决定；有\(adopt)按钮，没有\(withdraw)按钮。")
            ]), "03-downgrade.png")
            return "高风险后续停在建议卡，写明原因，未派发"
        }
    }

    private static func task(_ root: URL) throws -> [String: Any]? { try StaffFixture.tasks(root).first }

    private static func waitFor(_ description: String, _ check: () throws -> Bool) async throws {
        let deadline = Date().addingTimeInterval(90)
        while Date() < deadline {
            if try check() { return }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("等待超时：\(description)")
    }
}
