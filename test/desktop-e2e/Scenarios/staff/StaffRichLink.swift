import E2EKit
import Fixtures
import Foundation

/// 幕僚回复里的行内代码路径与外链，点击后在右侧抽屉打开；缺失路径给出提示。
@MainActor
public enum StaffRichLink {
    public nonisolated static let name = "staff-rich-link"

    public static func run(_ session: Session) async throws {
        let root = try StaffFixture.install()
        var project: URL?
        let keys = ["ZDOTDIR", "NORMA_BRAIN_DIR", "CLAUDE_CONFIG_DIR", "NORMA_E2E_RICH_LINK_DIR", "NORMA_BRAIN_ADAPTER_LOG"]
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
        try Catalog.seedWorkspace(database: database, name: "Staff Rich Link")
        try Catalog.suppressFeatureGuide(database: database)
        try VoiceConfigurationFixture.theme(database: database, id: "ink-black")
        let dir = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: "Staff Rich Link")
        project = dir
        // 路径解析会 canonicalize，/private/tmp 下的真实路径才能与项目根匹配。
        let real = dir.resolvingSymlinksInPath()
        try Data("<!doctype html><title>t</title><h1>RICH-LINK PAGE OK</h1>".utf8).write(to: real.appendingPathComponent("rich-link.html"))
        try Data("# RICH-LINK NOTES\n\nnotes body\n".utf8).write(to: real.appendingPathComponent("notes.md"))
        setenv("NORMA_E2E_RICH_LINK_DIR", real.path, 1)
        try session.launch(language: "zh-Hans")

        try await session.step("进入幕僚并收到含路径与外链的回复") {
            try await session.visualClick("点击 Staff Rich Link 工作空间卡片进入")
            try await session.visualClick("点击 E2E Project 项目进入画布")
            try await session.visualClick("点击底栏最左侧的浅紫发圆形头像进入幕僚模式")
            try await session.visualClick("点击中央任务对话底部的消息输入框，不是左侧搜索框")
            try await session.type("RICH-LINK 请给我验收入口")
            try await session.shortcut(key: 36, modifiers: [])
            try await waitFor("回复落盘", diagnostics: {
                let prompts = (try? StaffFixture.records(root, "prompts.jsonl").count) ?? -1
                let summary = ((try? StaffFixture.tasks(root)) ?? []).map { task in
                    (task["messages"] as? [[String: Any]] ?? []).map { message -> String in
                        let data = (try? JSONSerialization.data(withJSONObject: message)) ?? Data()
                        return String(decoding: data.prefix(700), as: UTF8.self)
                    }.joined(separator: " | ")
                }
                let log = (try? String(contentsOf: root.appendingPathComponent("adapter.log"), encoding: .utf8)) ?? "<none>"
                return "prompts.jsonl=\(prompts) tasks=\(summary) adapter.log=\(log.suffix(1200))"
            }) {
                try StaffFixture.tasks(root).contains { task in
                    (task["messages"] as? [[String: Any]] ?? []).contains { StaffFixture.replyText($0)?.contains("RICH-LINK 验收") == true }
                }
            }
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "reply", text: "Norma 回复里 rich-link.html、notes.md、missing.md 三个路径与 Example 外链都有链接样式（链接色或下划线），路径仍保留代码样式。")
            ]), "01-reply.png")
            return "回复渲染出可点击路径与外链"
        }

        try await session.step("点击 HTML 路径，抽屉显示页面") {
            try await session.visualClick("点击回复中的 rich-link.html 路径")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "html", text: "右侧抽屉已展开并显示 RICH-LINK PAGE OK；中央对话仍可见。若仍在加载判 wait。")
            ]), "02-html.png")
            return "HTML 在抽屉里打开"
        }

        try await session.step("点击 Markdown 路径与缺失路径") {
            try await session.visualClick("点击回复中的 notes.md 路径")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "md", text: "抽屉出现第二个标签，显示 RICH-LINK NOTES 标题与 notes body。")
            ]), "03-md.png")
            try await session.visualClick("点击回复中的 missing.md 路径")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "missing", text: "出现文件已移动或不存在的提示；抽屉仍显示 notes.md 内容，没有新增标签。")
            ]), "04-missing.png")
            return "Markdown 打开，缺失路径给出提示"
        }

        try await session.step("点击外链，抽屉浏览器标签") {
            try await session.visualClick("点击回复中的 Example 链接")
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "web", text: "抽屉新增浏览器标签，地址栏含 example.com；没有跳到系统浏览器。")
            ]), "05-web.png")
            return "外链进抽屉浏览器标签"
        }
    }

    private static func waitFor(_ description: String, diagnostics: (() -> String)? = nil, _ check: () throws -> Bool) async throws {
        let deadline = Date().addingTimeInterval(90)
        while Date() < deadline {
            if try check() { return }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("等待超时：\(description) \(diagnostics?() ?? "")")
    }
}
