import E2EKit
import Fixtures
import Foundation

/// 幕僚工具步骤列表：Read、cd、rg 每一行在工具名旁显示文件、目录或查询路径。
@MainActor
public enum StaffToolStepPaths {
    public nonisolated static let name = "staff-tool-step-paths"

    public static func run(_ session: Session) async throws {
        let root = try StaffFixture.install()
        var project: URL?
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR", "CLAUDE_CONFIG_DIR", "NORMA_E2E_TOOL_STEPS_DIR", "NORMA_BRAIN_ADAPTER_LOG"]
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
        setenv("NORMA_BRAIN_ADAPTER_LOG", root.appendingPathComponent("adapter.log").path, 1)
        try session.launch(language: "zh-Hans")
        _ = try await session.capturePixels()
        let database = try session.databaseURL()
        guard let dataRoot = session.dataRoot else { throw E2EError.env("缺少隔离数据根目录") }
        session.stop()
        try Catalog.seedWorkspace(database: database, name: "Staff Tool Steps")
        try Catalog.suppressFeatureGuide(database: database)
        try VoiceConfigurationFixture.theme(database: database, id: "ink-black")
        let dir = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "Staff Tool Steps")
        project = dir
        let real = dir.resolvingSymlinksInPath()
        setenv("NORMA_E2E_TOOL_STEPS_DIR", real.path, 1)
        try session.launch(language: "zh-Hans")

        try await session.step("进入幕僚并触发三个工具步骤") {
            try await session.visualClick("点击 Staff Tool Steps 工作空间卡片进入")
            try await session.visualClick("点击 E2E Project 项目进入画布")
            try await session.visualClick("点击底栏最左侧的浅紫发圆形头像进入幕僚模式")
            try await session.visualClick("点击中央任务对话底部的消息输入框，不是左侧搜索框")
            try await session.type("TOOL-STEPS 请读文件并查一下")
            try await session.shortcut(key: 36, modifiers: [])
            try await waitFor("回复落盘") {
                try StaffFixture.tasks(root).contains { task in
                    (task["messages"] as? [[String: Any]] ?? []).contains { StaffFixture.replyText($0)?.contains("TOOL-STEPS 验收") == true }
                }
            }
            return "回复完成，三步工具调用已落盘"
        }

        try await session.step("展开步骤列表，每行带文件或目录") {
            try await session.visualClick("点击回复上方「已处理 3 步」折叠标题展开步骤列表")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "read", text: "步骤列表里有一行工具名 Read，右侧紧跟弱化的路径，以 notes.md:3 结尾。"),
                VisualCriterion(id: "cd", text: "有一行工具名 cd，紧跟的目录以 docs/plans 结尾。"),
                VisualCriterion(id: "rg", text: "有一行工具名 rg，紧跟 docs/plans。"),
                VisualCriterion(id: "layout", text: "三行都有完成对勾和耗时，路径不与耗时重叠，没有整行换行或被截成空白。"),
            ]), "01-steps.png")
            return "Read、cd、rg 三行都显示路径"
        }
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
