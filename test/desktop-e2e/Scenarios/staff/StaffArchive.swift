import E2EKit
import Fixtures
import Foundation

/// 真实窗口里归档仍在进行的任务：列表、计数、提示、已归档视图和新消息取消归档。
@MainActor
public enum StaffArchive {
    public nonisolated static let name = "staff-archive"
    public nonisolated static let englishName = "staff-archive-en"

    public static func run(_ session: Session, english: Bool = false) async throws {
        let root = try StaffFixture.install()
        var project: URL?
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR", "CLAUDE_CONFIG_DIR", "NORMA_E2E_STAFF_QUESTIONS", "NORMA_E2E_STAFF_ARCHIVE"]
        let previous = Dictionary(uniqueKeysWithValues: keys.map { ($0, ProcessInfo.processInfo.environment[$0]) })
        defer {
            session.stop()
            for key in keys {
                if let value = previous[key] ?? nil { setenv(key, value, 1) } else { unsetenv(key) }
            }
            for path in [root, project].compactMap({ $0 }) {
                try? FileManager.default.removeItem(at: path)
            }
        }
        setenv("ZDOTDIR", root.path, 1)
        setenv("NORMA_BRAIN_DIR", root.path, 1)
        setenv("CLAUDE_CONFIG_DIR", root.appendingPathComponent("claude-config").path, 1)
        // 问题卡片 120 秒后超时。未派发提议不依赖真实 Agent，会一直停在「需要你」。
        unsetenv("NORMA_E2E_STAFF_QUESTIONS")
        setenv("NORMA_E2E_STAFF_ARCHIVE", "1", 1)
        let language = english ? "en" : "zh-Hans"
        try session.launch(language: language)
        _ = try await session.capturePixels()
        let database = try session.databaseURL()
        guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离数据根目录") }
        session.stop()
        try Catalog.seedWorkspace(database: database, name: "Staff Archive")
        try Catalog.suppressFeatureGuide(database: database)
        // 现行主题目录只有深色；paper-blue 会迁移成 ink-black，不能当作浅色验收。
        try VoiceConfigurationFixture.theme(database: database, id: english ? "sapphire-night" : "ink-black")
        project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "Staff Archive")
        try session.launch(language: language)

        let archive = english ? "Archive" : "归档"
        let archived = english ? "Archived" : "已归档"
        let undo = english ? "Undo" : "撤销"
        let unarchive = english ? "Unarchive" : "取消归档"
        let back = english ? "Back to tasks" : "返回任务列表"
        let activeNote = english ? "still in progress" : "仍在进行"

        try await session.step("进入幕僚并等到任务需要你") {
            try await session.visualClick("点击 Staff Archive 工作空间卡片进入")
            try await session.visualClick("点击 E2E Project 项目进入画布")
            try await session.visualClick("点击顶部幕僚或 Staff 按钮进入幕僚模式")
            try await session.visualClick("点击中央任务对话底部的消息输入框，不是左侧搜索框")
            try await session.type("ARCHIVE-KEEP 请先提出下一步")
            try await session.shortcut(key: 36, modifiers: [])
            try await waitFor("任务进入需要你") {
                guard let task = try kept(root) else { return false }
                let proposals = task["proposals"] as? [[String: Any]] ?? []
                return proposals.contains { $0["dispatch_id"] == nil || $0["dispatch_id"] is NSNull }
            }
            let row = try await session.visualLocate("定位左侧任务列表中标题 ARCHIVE-KEEP 的那一行中心，不要点顶部归档按钮")
            try await session.hover(local: row.point, on: row.frame)
            let hovered = try await session.capturePixels()
            try session.save(try await session.visualSnapshotCheck(hovered, [
                VisualCriterion(id: "row-icon", text: "鼠标停在 ARCHIVE-KEEP 这一行上时，行右侧出现归档图标按钮；浅色或深色背景下图标清晰，不是一直藏着。状态文字可以让开。")
            ]), "01-row-hover.png")
            return "需要你的任务已出现，悬停能看到归档图标"
        }

        try await session.step("从任务头部归档，列表计数去掉它，并给出可撤销提示") {
            try await session.visualClick("点击当前任务标题右侧带文字的\(archive)按钮，不要点左侧行内图标")
            try await waitFor("归档写入任务快照") {
                try kept(root)?["archived"] as? Bool == true
            }
            try await Task.sleep(nanoseconds: 400_000_000)
            let toast = try await session.capturePixels()
            try session.save(try await session.visualSnapshotCheck(toast, [
                VisualCriterion(id: "toast", text: "画面下方有一条提示，包含 ARCHIVE-KEEP，并写明\(activeNote)，右侧有\(undo)按钮。提示和按钮在当前浅色或深色背景下都可读。")
            ]), "02-toast.png")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "list", text: "左侧任务列表不再显示 ARCHIVE-KEEP。筛选里全部或 All 的计数是 0，需要你或 Needs you、执行中或 Running 的计数也是 0。底部有一行\(archived)，数字是 1。")
            ]), "03-hidden.png")
            guard try kept(root)?["archived"] as? Bool == true else {
                throw E2EError.product("归档没有写入任务快照")
            }
            return "归档后任务离开主列表，提示写明仍在进行"
        }

        try await session.step("打开已归档视图，发送新消息后自动取消归档") {
            try await session.visualClick("点击左侧底部的\(archived)入口")
            try await session.visualClick("点击已归档列表中的 ARCHIVE-KEEP")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "archived-view", text: "左侧上方有\(back)，标题是\(archived)。ARCHIVE-KEEP 在列表中。任务头部按钮文字是\(unarchive)。对话上方有说明：已归档或 Archived，并写明发送新消息会取消归档。")
            ]), "04-archived.png")
            try await session.visualClick("点击中央底部消息输入框")
            try await session.type("ARCHIVE-REOPEN")
            try await session.shortcut(key: 36, modifiers: [])
            try await waitFor("新消息取消归档") {
                try kept(root)?["archived"] as? Bool == false
            }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "reopened", text: "对话里能看到 ARCHIVE-REOPEN。任务头部按钮变回\(archive)，不再是\(unarchive)。已归档说明条消失。")
            ]), "05-reopened.png")
            try await session.visualClick("点击\(back)")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "restored-count", text: "回到普通任务列表。ARCHIVE-KEEP 重新出现。全部或 All 的计数是 1。底部\(archived)的数字是 0。")
            ]), "06-counts.png")
            return "新消息取消归档后，计数把任务算回主列表"
        }
    }

    private static func kept(_ root: URL) throws -> [String: Any]? {
        try StaffFixture.tasks(root).first { ($0["title"] as? String)?.contains("ARCHIVE-KEEP") == true }
    }

    private static func waitFor(_ description: String, _ check: () throws -> Bool) async throws {
        let deadline = Date().addingTimeInterval(90)
        while Date() < deadline {
            if try check() { return }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("等待超时：\(description)")
    }
}
