import E2EKit
import Fixtures
import Foundation

/// External ACP fixture holds genuine updates; App/task persistence remain production code.
@MainActor
public enum StaffWorkspaceStreaming {
    public nonisolated static let name = "staff-workspace-streaming"
    public nonisolated static let englishName = "staff-workspace-streaming-en"

    public static func run(_ session: Session, english: Bool = false) async throws {
        let root = try StaffFixture.install()
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR", "CLAUDE_CONFIG_DIR"]
        let previous = Dictionary(uniqueKeysWithValues: keys.map { ($0, ProcessInfo.processInfo.environment[$0]) })
        defer {
            session.stop()
            if let output = session.output {
                try? FileManager.default.copyItem(at: root, to: output.appendingPathComponent("staff-fixture-evidence"))
            }
            for key in keys { if let value = previous[key] ?? nil { setenv(key, value, 1) } else { unsetenv(key) } }
            try? FileManager.default.removeItem(at: root)
        }
        setenv("ZDOTDIR", root.path, 1)
        setenv("NORMA_BRAIN_DIR", root.path, 1)
        setenv("CLAUDE_CONFIG_DIR", root.appendingPathComponent("claude-config").path, 1)
        let language = english ? "en" : "zh-Hans"
        try session.launch(language: language)
        _ = try await session.capturePixels()
        let database = try session.databaseURL()
        session.stop()
        try Catalog.seedWorkspace(database: database, name: "Staff Workspace")
        try Catalog.seedWorkspace(database: database, name: "Other Workspace")
        try Catalog.suppressFeatureGuide(database: database)
        if english { try VoiceConfigurationFixture.theme(database: database, id: "sapphire-night") }
        for name in ["Alpha", "Beta"] {
            let project = root.appendingPathComponent(name)
            try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
            try Data("\(name) project\n".utf8).write(to: project.appendingPathComponent("\(name).md"))
            try Catalog.seedArea(database: database, workspaceName: "Staff Workspace", root: project, displayName: name)
        }
        try session.launch(language: language)
        try await session.step("从工作空间总览直接开始幕僚任务") {
            try await session.visualClick("点击 Staff Workspace 工作空间卡片")
            try await session.visualClick("保持所有项目总览，点击顶部幕僚/Staff分段按钮；不要进入项目")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "workspace", text: "主标题栏仍为所有项目/All projects；幕僚标题栏有Staff Workspace工作空间选择器和所有项目项目筛选器。中央有可用新任务输入框与项目目标选择器，没有必须打开项目的错误。"),
                VisualCriterion(id: "prototype", text: "按原型最终覆盖样式，幕僚铺满主标题栏以下的窗口，无额外浮层边距；灰黑中性表面，56px左右幕僚标题栏和34px页脚。左栏约330px，44px新任务入口、紧凑搜索/筛选按钮，按钮字号约12–13px，没有此前巨大按钮文字。"),
                VisualCriterion(id: "language", text: english ? "控制文案为英文：All projects、Give Norma the next task、Send to Norma；项目名Alpha/Beta保持原文。" : "控制文案为中文：所有项目、把下一件事交给Norma、发送到Norma；项目名Alpha/Beta保持原文。")
            ]), "01-workspace.png")
            try await session.visualClick("点击中央新任务所属项目的下拉框，不是标题栏项目筛选器")
            try await session.visualClick("点击下拉菜单的 Alpha 项目")
            try await send(session, "STREAM-ALPHA")
            try await waitFor("ACP waiting") { try StaffFixture.records(root, "stream-events.jsonl").contains { $0["stage"] as? String == "release-waiting" } }
            try session.save(try await session.visualCheck([VisualCriterion(id: "waiting", text: "用户消息STREAM-ALPHA已出现；其下Norma带正在思考/Thinking或等待处理状态和加载图标，输入框仍可见；没有ALPHA PARTIAL或完整回复，没有虚构计划。")]), "02-waiting.png")
            return "总览可创建任务，首token前反馈可见"
        }
        try await session.step("真实分片和计划状态在结束前可见") {
            try release(root, "release-waiting")
            try await waitFor("ACP partial") { try StaffFixture.records(root, "stream-events.jsonl").contains { $0["stage"] as? String == "release-partial" } }
            try session.save(try await session.visualCheck([VisualCriterion(id: "partial", text: "Norma显示ALPHA PARTIAL: project inspected.且仍在回复；执行计划1/2，Inspect Alpha project已完成，Prepare acceptance result执行中。ALPHA FINAL尚未出现，文字/计划/输入区没有重叠。")]), "03-partial.png")
            guard try normaReplies(root).isEmpty else { throw E2EError.product("模型结束前不应伪造完整历史") }
            try release(root, "release-partial")
            try await waitFor("ACP final held") { try StaffFixture.records(root, "stream-events.jsonl").contains { $0["stage"] as? String == "release-final" } }
            try session.save(try await session.visualCheck([VisualCriterion(id: "plan-complete", text: "流式文字同时有ALPHA PARTIAL和ALPHA FINAL；执行计划为2/2，两行都是已完成/Completed，没有重复的旧计划行；回复仍在等待模型结束。")]), "04-plan-complete.png")
            try release(root, "release-final")
            try await waitFor("durable reply") { try normaReplies(root).count == 1 }
            try session.save(try await session.visualCheck([VisualCriterion(id: "settled", text: "完整ALPHA PARTIAL和ALPHA FINAL只出现一份，等待动画和回复中提示已消失；计划2/2仍可查看。")]), "05-settled.png")
            return "结束前有分片/计划，结束后恰好一条完整回复"
        }
        try await session.step("项目筛选和草稿隔离，所有项目聚合") {
            try await session.visualClick("点击中央底部消息输入框")
            try await session.type("ALPHA UNSENT DRAFT")
            try await session.visualClick("点击幕僚标题栏的所有项目/All projects筛选下拉框")
            try await session.visualClick("点击菜单Beta项目")
            _ = try await session.visualCheck([VisualCriterion(id: "beta-empty", text: "标题栏项目筛选为Beta；左侧没有STREAM-ALPHA任务，中央是Beta新任务输入区，不能残留ALPHA UNSENT DRAFT。主窗口仍在所有项目总览。")])
            try await send(session, "STREAM-BETA")
            try await waitFor("Beta durable") { try normaReplies(root).count == 2 }
            try await session.visualClick("点击幕僚标题栏Beta项目筛选下拉框")
            try await session.visualClick("点击菜单所有项目/All projects")
            try await session.visualClick("点击左侧STREAM-ALPHA任务行")
            try session.save(try await session.visualCheck([VisualCriterion(id: "all-projects", text: "左侧同时显示STREAM-ALPHA和STREAM-BETA，各自副标题为Alpha/Beta；中央选中ALPHA任务，历史是ALPHA回复，输入框恢复ALPHA UNSENT DRAFT，不显示BETA RESULT。")]), "06-all-projects.png")
            let tasks = try StaffFixture.tasks(root)
            guard tasks.count == 2, Set(tasks.compactMap { $0["project_id"] as? String }).count == 2 else {
                throw E2EError.product("两个任务没有隔离到两个项目: \(tasks)")
            }
            try await session.visualClick("点击主窗口右上角侧栏图标，展开右侧内容抽屉")
            try await session.visualClick("点击右侧内容抽屉的文件/Files工具")
            try await session.visualClick("点击文件树的所有文件/All files分区标题展开目录")
            try await session.visualClick("点击右侧Alpha.md文件")
            _ = try await session.visualCheck([
                VisualCriterion(id: "alpha-file", text: "文件工作台项目为Alpha，打开Alpha.md并显示Alpha project；中央仍是Alpha任务。"),
                VisualCriterion(id: "drawer-frame", text: "右侧抽屉从幕僚标题栏下方开始，外框/标签栏为中性灰黑而非紫色壁纸；窗口最顶栏铺满全宽，幕僚/添加项目等操作靠窗口右侧，不在抽屉上方留下空白。文件预览本身沿用原生工作台样式。")
            ])
            try await session.visualClick("点击左侧STREAM-BETA任务行")
            _ = try await session.visualCheck([VisualCriterion(id: "beta-files", text: "文件工作台项目自动跟随为Beta，树中列出Beta.md，不能继续显示Alpha.md旧正文；中央是Beta任务和BETA RESULT。")])
            try await session.visualClick("点击右侧Beta.md文件")
            try session.save(try await session.visualCheck([VisualCriterion(id: "beta-file", text: "Beta.md正文为Beta project，没有打开Alpha项目文件；任务对话和输入区仍可用。")]), "06-project-files.png")
            try await session.visualClick("点击左侧STREAM-ALPHA任务行")
            try await session.visualClick("点击中央底部消息输入框")
            try await session.selectAll(); try await session.deleteSelection()
            try await session.type("STREAM-FAIL")
            try await session.shortcut(key: 36, modifiers: [0x37])
            try session.save(try await session.visualCheck([VisualCriterion(id: "failed", text: "Norma显示本次响应未完成/Response interrupted错误状态，没有永久等待动画；已完成ALPHA回复仍保留。若尚在等待允许wait。")]), "07-failed.png")
            return "项目/任务/草稿隔离，失败停止等待"
        }
        try await session.step("切换工作空间不泄漏任务") {
            try await session.visualClick("点击幕僚标题栏Staff Workspace工作空间下拉框")
            try await session.visualClick("点击菜单Other Workspace")
            try session.save(try await session.visualCheck([VisualCriterion(id: "other-workspace", text: "工作空间变为Other Workspace，没有STREAM-ALPHA或STREAM-BETA旧任务；显示该工作空间没有项目的说明，不冒用原项目。")]), "08-other-workspace.png")
            return "工作空间边界正确"
        }
    }

    private static func send(_ session: Session, _ text: String) async throws {
        try await session.visualClick("点击中央底部任务消息输入框")
        try await session.type(text)
        try await session.shortcut(key: 36, modifiers: [0x37])
    }
    private static func release(_ root: URL, _ name: String) throws {
        try Data().write(to: root.appendingPathComponent(name))
    }
    private static func normaReplies(_ root: URL) throws -> [String] {
        try StaffFixture.tasks(root).flatMap { task in
            (task["messages"] as? [[String: Any]] ?? []).compactMap { ($0["Norma"] as? [String: Any])?["text"] as? String }
        }
    }
    private static func waitFor(_ label: String, _ condition: () throws -> Bool) async throws {
        // 20s = 100 checks × 200ms, enough for local isolated ACP/CLI startup.
        for _ in 0..<100 {
            if try condition() { return }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("Timeout: \(label)")
    }
}
