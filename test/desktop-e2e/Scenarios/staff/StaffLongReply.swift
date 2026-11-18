import E2EKit
import Fixtures
import Foundation

/// 幕僚长富文本回复：含长路径表格、列表和软换行段落，贴底后最后一行完整可见，表格长路径换行可读。
@MainActor
public enum StaffLongReply {
    public nonisolated static let name = "staff-long-reply"

    public static func run(_ session: Session) async throws {
        let root = try StaffFixture.install()
        var project: URL?
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR", "CLAUDE_CONFIG_DIR", "NORMA_BRAIN_ADAPTER_LOG"]
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
        try Catalog.seedWorkspace(database: database, name: "Staff Long Reply")
        try Catalog.suppressFeatureGuide(database: database)
        try VoiceConfigurationFixture.theme(database: database, id: "ink-black")
        let dir = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "Staff Long Reply")
        project = dir
        try session.launch(language: "zh-Hans")

        try await session.step("进入幕僚并收到长富文本回复") {
            try await session.visualClick("点击 Staff Long Reply 工作空间卡片进入")
            try await session.visualClick("点击 E2E Project 项目进入画布")
            try await session.visualClick("点击底栏最左侧的浅紫发圆形头像进入幕僚模式")
            try await session.visualClick("点击中央任务对话底部的消息输入框，不是左侧搜索框")
            try await session.type("LONG-REPLY 请给我长回复")
            try await session.shortcut(key: 36, modifiers: [])
            try await waitFor("回复落盘") {
                try StaffFixture.tasks(root).contains { task in
                    (task["messages"] as? [[String: Any]] ?? []).contains { StaffFixture.replyText($0)?.contains("LONG-REPLY END") == true }
                }
            }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "last-line", text: "回复最后一行「还没验收，是因为……」完整可见，没有被底部输入框盖住或只露半行。"),
                VisualCriterion(id: "table", text: "表格右列的长路径要么换行显示完整，要么可横向滚动，没有被右边缘直接切掉。"),
                VisualCriterion(id: "no-overlap", text: "各段文字之间没有互相重叠。"),
            ]), "01-long-reply.png")
            return "长回复贴底完整可见"
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
