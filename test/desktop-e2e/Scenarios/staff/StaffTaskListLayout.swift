import E2EKit
import Fixtures
import Foundation

/// 通过真实输入创建同一分组的三条任务，验收侧栏密度和状态反馈。
@MainActor
public enum StaffTaskListLayout {
    public nonisolated static let name = "staff-task-list-layout"
    public nonisolated static let englishName = "staff-task-list-layout-en"

    public static func run(_ session: Session, english: Bool = false) async throws {
        let root = try StaffFixture.install()
        var project: URL?
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR", "CLAUDE_CONFIG_DIR"]
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
        let language = english ? "en" : "zh-Hans"
        try session.launch(language: language)
        _ = try await session.capturePixels()
        let database = try session.databaseURL()
        guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离数据根目录") }
        session.stop()
        try Catalog.seedWorkspace(database: database, name: "Task List Layout")
        try Catalog.suppressFeatureGuide(database: database)
        try VoiceConfigurationFixture.theme(database: database, id: english ? "sapphire-night" : "ink-black")
        project = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "Task List Layout")
        try session.launch(language: language)

        let newTask = english ? "New task" : "新任务"
        let waiting = english ? "Waiting" : "等待中"
        let noMatches = english ? "No matching tasks" : "没有找到匹配的任务"
        let rowLayout = VisualCriterion(id: "row-layout", text: "任务列表的 \(waiting) 分组下连续三行，每行头像、标题、辅助信息和时间完整地位于同一行范围内，背景与相邻行有明确留白，文字与头像、时间互不遮挡。")
        let metadata = VisualCriterion(id: "metadata", text: "每行主标题明显比辅助信息醒目，三行的次要时间对齐。长标题在一行内省略，标题下方保留任务辅助信息。每行不重复分组状态 \(waiting)。")
        try await session.step("创建三条等待中的任务，检查默认态、选中态和长标题") {
            try await session.visualClick("点击 Task List Layout 工作空间卡片进入")
            try await session.visualClick("点击 E2E Project 项目进入画布")
            try await session.visualClick("点击底栏最左侧的浅紫发圆形头像进入幕僚模式")
            let titles = ["Map STREAM-BETA", "Notes STREAM-BETA",
                          "Design 系统设计与可访问性验收清单 for the workspace STREAM-BETA"]
            for (index, title) in titles.enumerated() {
                try await session.visualClick("点击幕僚任务侧栏顶部的 \(newTask) 按钮，打开新任务草稿")
                try await session.type(title)
                try await session.shortcut(key: 36, modifiers: [])
                let deadline = Date().addingTimeInterval(30)
                var ready = false
                while Date() < deadline {
                    let tasks = try StaffFixture.tasks(root)
                    if tasks.count == index + 1,
                       tasks.contains(where: {
                           let messages = $0["messages"] as? [[String: Any]] ?? []
                           return ($0["title"] as? String)?.hasPrefix(String(title.prefix(10))) == true
                               && messages.contains { StaffFixture.replyText($0)?.contains("BETA RESULT") == true }
                       }) {
                        ready = true
                        break
                    }
                    try await Task.sleep(nanoseconds: 200_000_000)
                }
                guard ready else {
                    throw E2EError.product("第 \(index + 1) 条任务没有通过真实发送创建")
                }
            }
            try session.save(try await session.visualCheck([
                rowLayout, metadata,
                VisualCriterion(id: "selected", text: "Design 开头的长标题任务持续选中，柔和蓝色背景完整包住头像、两行文字和时间。Map、Notes 两行处于默认态。")
            ]), "01-selected.png")
            return "三条任务按相同高度排布，长标题不挤压右侧信息"
        }

        try await session.step("悬停时显示归档，位置稳定，点击空筛选显示空态") {
            let row = try await session.visualLocate("定位侧栏里 Notes 开头的任务标题中心")
            try await session.hover(local: row.point, on: row.frame)
            try session.save(try await session.visualCheck([
                rowLayout, metadata,
                VisualCriterion(id: "hover", text: "Notes 行有独立中性悬停色和归档图标。图标在行内部且与文字和时间分开。Design 行仍是蓝色选中态，三行布局与悬停前一致。")
            ]), "02-hover.png")
            try await session.visualClick("点击任务侧栏筛选栏中的 \(english ? "Running" : "执行中")，计数为 0")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "empty", text: "侧栏筛选已切换到 \(english ? "Running" : "执行中")。任务行消失，显示 \(noMatches)。")
            ]), "03-empty.png")
            try await session.visualClick("点击任务侧栏筛选栏中的 \(english ? "All" : "全部")，计数为 3")
            return "悬停和空筛选均有可见反馈"
        }

        try await session.step("窄窗折叠，展开浮层后仍有完整的任务行") {
            try await session.resize(width: 750, height: 820)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "collapsed", text: "窄窗口仅显示对话，左侧无任务列表。顶部靠右有三横线任务列表开关。")
            ]), "04-collapsed.png")
            try await session.visualClick("点击顶部靠右的三横线任务列表开关，展开侧栏")
            try session.save(try await session.visualCheck([rowLayout, metadata]), "05-expanded.png")
            return "窄窗展开后，任务行的对齐和留白仍完整"
        }
    }
}
