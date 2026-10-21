import E2EKit
import Foundation

/// 终端里 Agent 回复的反引号文件路径：点击后在右抽屉打开（含空格路径与 Markdown 预览）。
@MainActor
public enum ContentTerminalPathLink {
    public nonisolated static let name = "content-terminal-path-link"

    public static func run(_ session: Session) async throws {
        let workspace = "TerminalPathLinkCheck"
        var root: URL?
        defer { session.stop(); if let root { try? FileManager.default.removeItem(at: root) } }
        try await session.step("建立含 Markdown 文件的隔离项目") {
            try session.launch(language: "zh-Hans")
            let database = try session.databaseURL()
            _ = try await session.waitCatalog(timeout: 15) { _ in FileManager.default.fileExists(atPath: database.path) }
            session.stop()
            guard let data = session.dataRoot else { throw E2EError.env("隔离目录丢失") }
            try Catalog.seedWorkspace(database: data.appendingPathComponent("app.sqlite"), name: workspace)
            let dir = URL(fileURLWithPath: "/private/tmp/norma-path-link-e2e-\(UUID().uuidString)", isDirectory: true)
            try FileManager.default.createDirectory(at: dir.appendingPathComponent("docs/my notes"), withIntermediateDirectories: true)
            try Data("# TERMINAL LINK E2E\n".utf8).write(to: dir.appendingPathComponent("docs/PlanLinkE2E.md"))
            try Data("# SPACED PATH E2E\n".utf8).write(to: dir.appendingPathComponent("docs/my notes/spaced note.md"))
            root = dir
            try Catalog.seedArea(database: data.appendingPathComponent("app.sqlite"), workspaceName: workspace, root: dir, displayName: "Link Project")
            try session.launch(language: "zh-Hans")
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            let frame = try await session.capture()
            let open = frame.pick(["打开", "Open"], .bottom)?.center
                ?? CGPoint(x: frame.windowFrame.width - 112, y: 540)
            try await session.click(local: open, on: frame)
            try await Task.sleep(nanoseconds: 1_200_000_000)
            return "隔离项目已打开，右抽屉尚无内容标签"
        }
        try await session.step("终端输出模拟 Agent 回复") {
            try await session.click(matching: ["Link Project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["Link Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            // ⌘1 在项目画布新建 Terminal 并聚焦（总览视图下该快捷键不创建卡片，因此先进入项目）。
            try await session.shortcut(key: 18, modifiers: [0x37])
            try await Task.sleep(nanoseconds: 3_000_000_000)
            try await session.type("printf 'Fixed `docs/PlanLinkE2E.md:1` and `docs/my notes/spaced note.md` ok\\n'")
            try await session.shortcut(key: 36, modifiers: [])
            let reply = try await session.wait(Expectation(allOf: ["PlanLinkE2E.md"], timeout: 12))
            try session.save(reply, "01-terminal-reply.png")
            return "终端已打印带反引号路径的模拟回复"
        }
        try await session.step("点击相对路径在右抽屉打开") {
            // 命令回显在上、输出在下：取最下方的匹配即输出行。
            try await session.click(matching: ["PlanLinkE2E.md"], pick: .bottom)
            let shown = try await session.wait(Expectation(allOf: ["TERMINAL LINK E2E"], timeout: 10))
            try session.save(shown, "02-path-opened.png")
            guard try session.contentTabs(workspaceName: workspace).contains(where: { $0.kind == "file" }) else {
                throw E2EError.product("点击路径后没有在右抽屉创建文件标签")
            }
            return "相对路径点击后右抽屉显示 Markdown 预览，并持久化文件标签"
        }
        try await session.step("点击含空格路径") {
            try await session.click(matching: ["spaced note.md"], pick: .bottom)
            let shown = try await session.wait(Expectation(allOf: ["SPACED PATH E2E"], timeout: 10))
            try session.save(shown, "03-spaced-path-opened.png")
            return "含空格路径整体可点击，右抽屉显示对应文件"
        }
    }
}
