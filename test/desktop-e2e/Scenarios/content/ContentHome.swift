import E2EKit
import Fixtures
import Foundation

@MainActor
public enum ContentHome {
    public nonisolated static let name = "content-home"

    public static func run(_ session: Session) async throws {
        let workspace = "ContentHomeCheck"
        var root: URL?
        defer { session.stop(); if let root { try? FileManager.default.removeItem(at: root) } }
        try await session.step("无标签时打开右抽屉") {
            try session.launch()
            let database = try session.databaseURL()
            _ = try await session.waitCatalog(timeout: 15) { _ in FileManager.default.fileExists(atPath: database.path) }
            session.stop()
            guard let data = session.dataRoot else { throw E2EError.env("隔离目录丢失") }
            let sqlite = data.appendingPathComponent("app.sqlite")
            try Catalog.seedWorkspace(database: sqlite, name: workspace)
            let project = URL(fileURLWithPath: "/private/tmp/norma-content-home-e2e-\(UUID().uuidString)", isDirectory: true)
            try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
            try Data("HOME E2E\n".utf8).write(to: project.appendingPathComponent("NoteE2E.txt"))
            root = project
            // 只播种项目，不播种内容标签，右栏保持空态。
            try Catalog.seedArea(database: sqlite, workspaceName: workspace, root: project, displayName: "Home Project")
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            let frame = try await session.capture()
            let open = frame.pick(["打开", "Open"], .bottom)?.center
                ?? CGPoint(x: frame.windowFrame.width - 112, y: 540)
            try await session.click(local: open, on: frame)
            try await Task.sleep(nanoseconds: 950_000_000)
            let drawer = try await session.capture()
            if !drawer.contains("搜索或输入网址") {
                try await session.click(local: CGPoint(x: Double(drawer.windowFrame.width) - 28, y: 26), on: drawer)
            }
            _ = try await session.wait(Expectation(allOf: ["搜索或输入网址", "工具", "推荐"], timeout: 8))
            try await Task.sleep(nanoseconds: 450_000_000)
            let home = try await session.capture()
            try session.save(home, "01-home.png")
            guard try session.contentTabs(workspaceName: workspace).isEmpty else { throw E2EError.product("空态不应有内容标签") }
            return "右抽屉空态显示地址栏、工具与推荐"
        }
        try await session.step("工具磁贴文案齐全") {
            let frame = try await session.wait(Expectation(allOf: ["变更", "终端", "文件", "项目进度", "网页", "更多工具"], absent: ["打开网页"], timeout: 6))
            try session.save(frame, "02-home-tools.png")
            return "六个工具磁贴可见，独立“打开网页”按钮已移除"
        }
        try await session.step("左右抽屉随语言即时切换，输入草稿保持原文") {
            try await session.click(matching: ["搜索或输入网址"], pick: .right)
            try await session.type("用户草稿 example.com")
            try await session.shortcut(key: 43, modifiers: [0x37])
            try await SettingsSupport.language(session, label: "English", saved: "en")
            try await session.shortcut(key: 43, modifiers: [0x37])
            let english = try await session.wait(Expectation(
                allOf: ["Tools", "Recommended", "Project progress", "Not started", "用户草稿 example.com"],
                absent: ["项目进度", "待开始", "手动添加"], timeout: 8))
            try session.save(english, "05-drawers-english.png")
            try await session.click(matching: ["用户草稿 example.com"], pick: .right)
            try await session.selectAll()
            try await session.shortcut(key: 51, modifiers: [])
            _ = try await session.wait(Expectation(allOf: ["Search or enter a URL"], absent: ["搜索或输入网址"]))
            try await session.type("file: /private/用户草稿.md")
            try await session.shortcut(key: 36, modifiers: [])
            _ = try await session.wait(Expectation(allOf: ["Use an HTTP or HTTPS URL"], absent: ["浏览器支持"]))
            try await session.shortcut(key: 43, modifiers: [0x37])
            try await SettingsSupport.language(session, label: "简体中文", saved: "zh-CN")
            try await session.shortcut(key: 43, modifiers: [0x37])
            let chinese = try await session.wait(Expectation(
                allOf: ["项目进度", "待开始", "浏览器支持 HTTP"], absent: ["Project progress", "Use an HTTP"], timeout: 8))
            try session.save(chinese, "06-drawers-chinese.png")
            try await session.click(matching: ["file:"], pick: .right)
            try await session.selectAll()
            try await session.shortcut(key: 51, modifiers: [])
            _ = try await session.wait(Expectation(allOf: ["搜索或输入网址"], absent: ["Search or enter a URL"]))
            return "两侧抽屉、已有占位符和缓存错误即时切换；用户草稿不翻译"
        }
        try await session.step("未实现工具给出就地提示") {
            try await session.click(matching: ["项目进度"], pick: .bottom)
            let frame = try await session.wait(Expectation(allOf: ["即将支持"], timeout: 6))
            try session.save(frame, "03-home-upcoming.png")
            guard try session.contentTabs(workspaceName: workspace).isEmpty else { throw E2EError.product("展望入口不应打开标签") }
            return "提示出现且未打开标签"
        }
        try await session.step("文件磁贴打开文件工作台") {
            try await Task.sleep(nanoseconds: 2_300_000_000)
            // 推荐区也有“文件”占位，磁贴在其上方，取最上面的匹配。
            try await session.click(matching: ["文件"], pick: .top)
            let frame = try await session.wait(Expectation(allOf: ["NoteE2E.txt"], absent: ["搜索或输入网址"], timeout: 10))
            try session.save(frame, "04-home-files-opened.png")
            guard try session.contentTabs(workspaceName: workspace).contains(where: { $0.kind == "project_files" }) else { throw E2EError.product("文件标签未持久化") }
            return "文件磁贴打开真实文件树"
        }
    }
}
