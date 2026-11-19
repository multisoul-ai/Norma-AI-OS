import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// Captured OpenCode form + rejection replay; production App/CLI/Gate, no TaskStore injection.
@MainActor
public enum StaffProtocolRecovery {
    public nonisolated static let name = "staff-protocol-recovery"
    public nonisolated static let englishName = "staff-protocol-recovery-en"

    public static func run(_ session: Session, english: Bool = false) async throws {
        let root = try StaffProtocolFixture.install()
        var project: URL?
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR", "CLAUDE_CONFIG_DIR"]
        let previous = Dictionary(uniqueKeysWithValues: keys.map { ($0, ProcessInfo.processInfo.environment[$0]) })
        defer {
            session.stop()
            if let output = session.output { try? FileManager.default.copyItem(at: root, to: output.appendingPathComponent("staff-fixture-evidence")) }
            for key in keys { if let value = previous[key] ?? nil { setenv(key, value, 1) } else { unsetenv(key) } }
            for path in [root, project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        setenv("ZDOTDIR", root.path, 1)
        setenv("NORMA_BRAIN_DIR", root.path, 1)
        setenv("CLAUDE_CONFIG_DIR", root.appendingPathComponent("claude-config").path, 1)
        let language = english ? "en" : "zh-Hans"
        try session.launch(language: language)
        _ = try await session.capturePixels()
        let database = try session.databaseURL()
        guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离数据根目录") }
        session.stop()
        try Catalog.seedWorkspace(database: database, name: "Staff M1")
        try Catalog.suppressFeatureGuide(database: database)
        project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "Staff M1")
        try session.launch(language: language)
        guard let output = session.output else { throw E2EError.env("缺少录像目录") }
        let recording = try StaffRecording(session: session, first: try await session.capturePixels(), url: output.appendingPathComponent("staff-protocol-recovery.mp4"))
        do {
            try await session.step("原生 OpenCode 问答表单进入真实卡片") {
                try await session.visualClick("点击 Staff M1 工作空间进入")
                try await session.visualClick("点击 E2E Project 项目进入画布")
                try await session.visualClick("点击底部命令输入框")
                try await session.type("/new claude")
                try await session.shortcut(key: 36, modifiers: [])
                try await waitFor("Worker 名册准备好") { try StaffFixture.records(root, "starts.jsonl").count == 1 }
                try await session.visualClick("点击底栏最左侧的浅紫发圆形头像进入幕僚")
                try await session.visualClick("点击中央对话底部的消息输入框")
                try await session.type("PROTOCOL Please ask about theme, then propose a review.")
                try await session.shortcut(key: 36, modifiers: [])
                try await session.visualClick("点击 Which interface theme do you prefer? 问答卡里的 Dark 选项；若仍在加载，判 wait")
                // Capture selected state, then submit immediately: model review must not consume the question's 120 s TTL.
                let answeredForm = try await session.capturePixels()
                try await session.click(local: CGPoint(x: 480, y: 562), on: answeredForm)
                try await waitFor("纠正后生成提议") { try StaffFixture.records(root, "recovered.jsonl").count == 1 }
                try session.save(try await session.visualSnapshotCheck(answeredForm, [
                    VisualCriterion(id: "native-form", text: "完整问题 Which interface theme do you prefer? 可读，不是只有 Questions；Dark 单选已选中，Light 未选中；保留自定义输入，且只有一道题。"),
                    VisualCriterion(id: "locale", text: english ? "提交回答和跳过控件为英文 Submit answers / Skip。" : "提交回答和跳过控件为中文。")
                ]), "01-native-question.png")
                return "OpenCode q0 表单正常回答；原工具只收到 decline，下一轮读到 Dark"
            }
            try await session.step("格式错误有明确双语反馈并自动纠正") {
                try await session.scroll(xFraction: 0.60, yFraction: 0.55, deltaY: -3000)
                // 过程摘要只落盘步骤标题、状态与耗时，不存工具输出；回合运行中过程区自动展开。
                try await session.scroll(xFraction: 0.60, yFraction: 0.55, deltaY: -3000)
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "format", text: english ? "同一条 Norma 回复的过程区里，组合命令那一步标为 Denied，不是 Completed；步骤标题里没有任何文件路径或命令参数。" : "同一条 Norma 回复的过程区里，组合命令那一步标为已拒绝，不是已完成；步骤标题里没有任何文件路径或命令参数。"),
                    VisualCriterion(id: "retry", text: "同一条回复里还有两条独立的已完成步骤（更新任务阶段、提出方案）；没有仍显示 The user rejected permission 的回复气泡。")
                ]), "02-format-recovered.png")
                let commands = try StaffFixture.records(root, "commands.jsonl")
                guard commands.count == 4, commands.filter({ $0["allowed"] as? Bool == false }).count == 1,
                      (try StaffFixture.tasks(root).first?["proposals"] as? [[String: Any]])?.count == 1 else {
                    throw E2EError.product("命令应为名册+拒绝组合+两条独立纠正，且只生成一条提议")
                }
                try Data().write(to: root.appendingPathComponent("release-recovery"))
                try await waitFor("最终回复落盘") {
                    let messages = try StaffFixture.tasks(root).first?["messages"] as? [[String: Any]] ?? []
                    return messages.contains { StaffFixture.replyText($0) == "Dark selected. Review proposal ready." }
                }
                // 回合完成后过程区折叠为“已处理 N 步”，回复正文与提议卡按顺序排在其后。
                try await session.scroll(xFraction: 0.60, yFraction: 0.55, deltaY: 180)
                try session.save(try await session.visualCheck([
                    VisualCriterion(id: "proposal", text: "最终回复 Dark selected. Review proposal ready. 可见，Review next 提议卡已出现且可采用。未自动派发 Worker，没有用户拒绝授权的错误回复。")
                ]), "03-proposal.png")
                return "组合未执行，两条纠正各执行一次，提议正常生成"
            }
            try await recording.finish()
        } catch { try? await recording.finish(); throw error }
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
