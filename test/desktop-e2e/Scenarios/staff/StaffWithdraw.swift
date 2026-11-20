import E2EKit
import Fixtures
import Foundation

/// 真实 App：还没跑起来就撤回。新开的卡关掉；复用的卡只中断，卡片留在画布上。
@MainActor
enum StaffWithdrawLaunch {
    static func prepare(_ session: Session, name: String) async throws -> (root: URL, project: URL?) {
        let root = try StaffFixture.install()
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR", "CLAUDE_CONFIG_DIR", "NORMA_E2E_STAFF_QUESTIONS", "NORMA_E2E_STAFF_ASSIGN"]
        for key in keys { unsetenv(key) }
        try Data(#"{"enabled":true,"backend":"claude","strategy":"speed","speak":false,"summary":"off","autonomy":{"assign_followups":false,"staff_auto_dispatch":true}}"#.utf8).write(to: root.appendingPathComponent("config.json"))
        setenv("ZDOTDIR", root.path, 1)
        setenv("NORMA_BRAIN_DIR", root.path, 1)
        setenv("CLAUDE_CONFIG_DIR", root.appendingPathComponent("claude-config").path, 1)
        setenv("NORMA_E2E_STAFF_ASSIGN", "1", 1)
        try session.launch(language: "zh-Hans")
        _ = try await session.capturePixels()
        let database = try session.databaseURL()
        guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离数据根目录") }
        session.stop()
        try Catalog.seedWorkspace(database: database, name: name)
        try Catalog.suppressFeatureGuide(database: database)
        try StaffAutoDispatch.copyEngineAuthorizations(into: database)
        try VoiceConfigurationFixture.theme(database: database, id: "ink-black")
        let project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: name)
        try session.launch(language: "zh-Hans")
        try await session.visualClick("点击 \(name) 工作空间卡片进入")
        try await session.visualClick("点击 E2E Project 项目进入画布")
        _ = try await session.capturePixels()
        session.stop()
        try StaffAutoDispatch.insertGrace(dataRoot)
        try session.launch(language: "zh-Hans")
        try await session.visualClick("点击 \(name) 工作空间卡片进入")
        try await session.visualClick("点击 E2E Project 项目进入画布")
        try await session.visualClick("点击底栏最左侧的浅紫发圆形头像进入幕僚模式")
        try await session.visualClick("点击中央任务对话底部的消息输入框，不是左侧搜索框")
        return (root, project)
    }

    static func canvasContains(_ dataRoot: URL, node: Int) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        process.arguments = ["-c", """
import json, sqlite3, sys
from pathlib import Path
root, want = Path(sys.argv[1]), int(sys.argv[2])
for db in root.glob('desktop-canvases/*.sqlite'):
    try:
        row = sqlite3.connect(f'file:{db}?mode=ro', uri=True).execute('select json from canvas_snapshots where id=1').fetchone()
    except sqlite3.Error:
        continue
    if not row:
        continue
    ids = [n.get('id') for n in json.loads(row[0]).get('nodes', [])]
    if want in ids:
        raise SystemExit(0)
raise SystemExit(1)
""", dataRoot.path, String(node)]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        guard (try? process.run()) != nil else { return false }
        process.waitUntilExit()
        return process.terminationStatus == 0
    }

    static func waitFor(_ description: String, _ check: () throws -> Bool) async throws {
        let deadline = Date().addingTimeInterval(90)
        while Date() < deadline {
            if try check() { return }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("等待超时：\(description)")
    }
}

@MainActor
public enum StaffWithdrawNewCard {
    public nonisolated static let name = "staff-withdraw-new"

    public static func run(_ session: Session) async throws {
        let prepared = try await StaffWithdrawLaunch.prepare(session, name: "Staff Withdraw")
        let root = prepared.root
        defer {
            session.stop()
            for path in [root, prepared.project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
            unsetenv("NORMA_E2E_STAFF_ASSIGN")
        }
        try await session.step("新开的卡在跑起来之前撤回，卡片从画布消失") {
            try await session.type("WITHDRAW-NEW 请派给 Grace")
            try await session.shortcut(key: 36, modifiers: [])
            try await StaffWithdrawLaunch.waitFor("新卡已派发且还没跑起来") {
                let workers = try StaffFixture.tasks(root).first?["workers"] as? [[String: Any]] ?? []
                let phase = workers.first?["phase"]
                let running = phase as? String == "Running" || phase as? String == "Waiting"
                return workers.count == 1 && workers[0]["reused"] as? Bool != true && !running && workers[0]["card_id"] is Int
            }
            let node = ((try StaffFixture.tasks(root).first?["workers"] as? [[String: Any]])?.first?["card_id"] as? Int) ?? 0
            guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离数据根目录") }
            // 先确认这张卡已经写进画布快照，后面“消失”才不是读到一份还没保存的空快照。
            try await StaffWithdrawLaunch.waitFor("新卡已写进画布快照") {
                StaffWithdrawLaunch.canvasContains(dataRoot, node: node)
            }
            try await session.scroll(xFraction: 0.48, yFraction: 0.55, deltaY: -800)
            try await session.visualClick("点击「新开撤回」这行右侧的撤回按钮")
            try await StaffWithdrawLaunch.waitFor("Worker 记为中断") {
                let phase = (try StaffFixture.tasks(root).first?["workers"] as? [[String: Any]])?.first?["phase"] as? [String: String]
                return phase?["Finished"] == "Interrupted"
            }
            try await StaffWithdrawLaunch.waitFor("画布快照里这张卡消失") {
                !StaffWithdrawLaunch.canvasContains(dataRoot, node: node)
            }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "withdraw-note", text: "对话里有一张 Grace 已中断的卡，写明用户撤回了派给 Grace 的新开撤回。撤回按钮已消失，没有错误提示。")
            ]), "01-withdraw-note.png")
            try await session.visualClick("点击右上角的「返回画布」按钮回到画布")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "new-gone", text: "画布上没有刚派发的 Grace 卡片，画布是空的。没有错误提示。")
            ]), "02-new-gone.png")
            return "新开的卡撤回后从画布消失"
        }
    }
}

@MainActor
public enum StaffWithdrawReuse {
    public nonisolated static let name = "staff-withdraw-reuse"

    public static func run(_ session: Session) async throws {
        let prepared = try await StaffWithdrawLaunch.prepare(session, name: "Staff Reuse")
        let root = prepared.root
        defer {
            session.stop()
            for path in [root, prepared.project].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
            unsetenv("NORMA_E2E_STAFF_ASSIGN")
        }
        try await session.step("复用的卡撤回后仍然留在画布上") {
            try await session.type("WITHDRAW-OPEN 先让 Grace 开工")
            try await session.shortcut(key: 36, modifiers: [])
            try await StaffWithdrawLaunch.waitFor("第一张卡已派发") {
                let workers = try StaffFixture.tasks(root).first?["workers"] as? [[String: Any]] ?? []
                return workers.count == 1 && workers[0]["card_id"] is Int
            }
            try await session.visualClick("点击中央任务对话底部的消息输入框，不是左侧搜索框")
            try await session.type("WITHDRAW-AGAIN 再派给 Grace")
            try await session.shortcut(key: 36, modifiers: [])
            try await StaffWithdrawLaunch.waitFor("第二次派发复用同一张卡") {
                let workers = try StaffFixture.tasks(root).first?["workers"] as? [[String: Any]] ?? []
                guard workers.count == 2, let first = workers[0]["card_id"] as? Int, let second = workers[1]["card_id"] as? Int else { return false }
                return first == second && workers[1]["reused"] as? Bool == true
            }
            let node = ((try StaffFixture.tasks(root).first?["workers"] as? [[String: Any]])?.last?["card_id"] as? Int) ?? 0
            try await session.scroll(xFraction: 0.48, yFraction: 0.55, deltaY: -800)
            try await session.visualClick("点击「再派一次」这行右侧的撤回按钮，不要点「先开一张」")
            try await StaffWithdrawLaunch.waitFor("第二次派发被中断") {
                let workers = try StaffFixture.tasks(root).first?["workers"] as? [[String: Any]] ?? []
                let phase = workers.last?["phase"] as? [String: String]
                return workers.count == 2 && phase?["Finished"] == "Interrupted"
            }
            guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离数据根目录") }
            // 给画布快照留出刷新的时间，再要求这张复用的卡仍在。
            try await Task.sleep(nanoseconds: 4_000_000_000)
            guard StaffWithdrawLaunch.canvasContains(dataRoot, node: node) else {
                throw E2EError.product("复用的卡 \(node) 被关掉了")
            }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "withdraw-note", text: "对话里有一张 Grace 已中断的卡，写明用户撤回了派给 Grace 的再派一次。撤回按钮已消失，没有错误提示。"),
                VisualCriterion(id: "single-worker", text: "任务页顶部标题下方的 Worker 栏只有一张 Grace 卡，状态显示「已中断」，对应刚撤回的这次派发。")
            ]), "01-withdraw-note.png")
            try await session.visualClick("点击右上角的「返回画布」按钮回到画布")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "reuse-kept", text: "画布上 Grace 的卡片还在，没有被关掉。没有错误提示。")
            ]), "02-reuse-kept.png")
            return "复用的卡撤回后仍在画布上"
        }
    }
}
