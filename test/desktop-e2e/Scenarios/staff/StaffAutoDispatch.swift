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
            if let output = session.output {
                try? FileManager.default.copyItem(at: root, to: output.appendingPathComponent("staff-fixture-evidence"))
            }
            for key in keys { if let value = previous[key] ?? nil { setenv(key, value, 1) } else { unsetenv(key) } }
            for path in [root, project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        // 幕僚开关打开，主会话的 assign_followups 保持关闭。没写 staff_auto_dispatch 时默认也是开。
        try Data(#"{"enabled":true,"backend":"claude","strategy":"speed","speak":false,"summary":"off","autonomy":{"assign_followups":false,"staff_auto_dispatch":true}}"#.utf8).write(to: root.appendingPathComponent("config.json"))
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
        // 隔离库没有本机已安装引擎的授权记录，名册人名会对不上可派引擎。把本机测试库里已启用的授权行拷过来，不读 token。
        try Self.copyEngineAuthorizations(into: database)
        try VoiceConfigurationFixture.theme(database: database, id: english ? "sapphire-night" : "ink-black")
        project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "Staff Auto")
        try session.launch(language: language)

        let adopt = english ? "Adopt and dispatch" : "采用并派发"
        let withdraw = english ? "Withdraw" : "撤回"

        try await session.step("准备一个可派活的 Agent 并让幕僚提出第一条任务") {
            try await session.visualClick("点击 Staff Auto 工作空间卡片进入")
            try await session.visualClick("点击 E2E Project 项目进入画布")
            // 名册要在派活前就有 Grace。中途重启会让新卡片复用已撤回 Worker 的编号，绑定被拒。
            _ = try await session.capturePixels()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离数据根目录") }
            session.stop()
            try Self.insertGrace(dataRoot)
            try session.launch(language: language)
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
                VisualCriterion(id: "auto-card", text: "第二条「补一条测试」折叠成一行已派发卡，状态文字是「✓ 幕僚已派发」/Dispatched by Staff，卡片右侧有\(withdraw)按钮。第一条 Worker 显示「已返回结果」/Result received，不是幕僚已派发。")
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

        try await session.step("用户点名 Grace 做高风险的事，直接派出，不出现提议") {
            try await session.visualClick("点击中央底部消息输入框")
            try await session.type("NAMED-RISK 让 Grace 推送发布")
            try await session.shortcut(key: 36, modifiers: [])
            try await waitFor("点名后的高风险直接派发") {
                try StaffFixture.tasks(root).contains { task in
                    let proposals = task["proposals"] as? [[String: Any]] ?? []
                    return proposals.contains { proposal in
                        proposal["agent"] as? String == "Grace"
                            && proposal["approved"] as? Bool == true
                            && (proposal["downgrade"] is NSNull || proposal["downgrade"] == nil)
                            && (proposal["prompt"] as? String ?? "").contains("git push")
                    }
                }
            }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "named-dispatch", text: "最新一条是已派发给 Grace 的「点名发布」，状态是「✓ 幕僚已派发」/Dispatched by Staff，有\(withdraw)按钮；不是需要你决定的建议卡，没有\(adopt)按钮。")
            ]), "04-named-dispatch.png")
            return "点名的高风险活直接派给 Grace"
        }
    }

    /// 隔离库没有本机编排凭据的哈希，已安装的 Claude 会被当成未注册，名册上的人就派不出去。
    /// 只把现有凭据的哈希写进隔离库，不打印 token。
    private static func copyEngineAuthorizations(into database: URL) throws {
        let credentials = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/Norma OS/orchestration")
        guard FileManager.default.fileExists(atPath: credentials.path) else {
            throw E2EError.env("本机没有 Norma OS 编排凭据，隔离 App 无法识别已安装引擎")
        }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        process.arguments = ["-c", #"""
import hashlib, json, sqlite3, sys, time
from pathlib import Path
root, dest = Path(sys.argv[1]), sqlite3.connect(sys.argv[2])
now, found = int(time.time()), False
for path in root.glob('*.json'):
    data = json.loads(path.read_text())
    token, provider = data.get('token'), data.get('provider')
    if data.get('managed_by') != 'Norma OS' or not token or not provider:
        continue
    digest = hashlib.sha256(token.encode()).hexdigest()
    dest.execute('insert or replace into orchestration_authorizations values (?,?,?,?,?,?,NULL)',
        (provider, digest, 1, data.get('installed_version') or 'desktop-e2e', now, now))
    found = found or provider == 'claude'
dest.commit()
if not found:
    raise SystemExit(2)
"""#, credentials.path, database.path]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw E2EError.env("写入引擎授权失败，隔离 App 仍无法派给已安装的 Claude")
        }
    }

    /// 名册写在项目库。App 已建库并停掉之后再插入 Grace，重启后点名才能命中。
    private static func insertGrace(_ dataRoot: URL) throws {
        let directory = dataRoot.appendingPathComponent("desktop-project-definitions", isDirectory: true)
        let files = (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? []
        let databases = files.filter { $0.pathExtension == "sqlite" }
        guard databases.count == 1, let database = databases.first else {
            throw E2EError.product("项目 Agent 定义库应恰好一个，实际 \(databases.count) 个")
        }
        try SQLRaw.execute(database: database, statements: [
            ("INSERT INTO agent_definitions (name, name_key, speech_alias, runtime_profile, workflow_description, enabled, sort_order, origin, created_at_unix, updated_at_unix) VALUES ('Grace', 'grace', '格蕾丝', 'claude', '', 1, 0, 'explicit', 1, 1)", []),
        ])
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
