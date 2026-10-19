import E2EKit
import Fixtures
import Foundation

@MainActor
public enum ContentWorkbench {
    public nonisolated static let name = "content-workbench"

    public static func run(_ session: Session) async throws {
        let workspace = "ContentWorkbenchCheck"
        var root: URL?
        defer { session.stop(); if let root { try? FileManager.default.removeItem(at: root) } }
        try await session.step("建立隔离项目与文件工作台") {
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            _ = try await session.wait(Expectation(anyOf: ["工作空间名称", "Workspace name"], timeout: 5))
            let dialog = try await session.capture()
            guard let input = dialog.pick(["网站改版", "Website redesign"], .bottom) else { throw E2EError.product("缺少工作空间输入框") }
            try await session.click(local: input.center, on: dialog)
            try await session.type(workspace)
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 12))
            session.stop()
            guard let data = session.dataRoot else { throw E2EError.env("隔离目录丢失") }
            root = try ContentFixture.install(dataRoot: data, workspace: workspace)
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            let frame = try await session.capture()
            if frame.contains("搜索工作空间") || frame.contains("Search workspaces") { try await session.click(matching: ["打开", "Open"], pick: .bottom) }
            _ = try await session.wait(Expectation(anyOf: ["当前对话", "Content Project"], timeout: 12))
            let before = try await session.capture()
            if !before.contains("全部文件") { try await session.click(local: CGPoint(x: Double(before.windowFrame.width) - 28, y: 26), on: before) }
            let tree = try await session.wait(Expectation(allOf: ["全部文件", "NoteE2E.txt", "本地变更"], timeout: 15))
            try session.save(tree, "01-file-tree.png")
            guard try session.contentTabs(workspaceName: workspace).contains(where: { $0.kind == "project_files" }) else { throw E2EError.product("工作台未持久化") }
            return "真实窗口显示目录树和 Git 变更，工作台已恢复"
        }
        try await session.step("查看并编辑文本") {
            try await session.click(matching: ["NoteE2E.txt"], pick: .bottom)
            let text = try await session.wait(Expectation(allOf: ["LOCAL CHANGE E2E", "编辑"], timeout: 10))
            try session.save(text, "02-text-preview.png")
            try await session.click(matching: ["编辑"], pick: .top)
            let editor = try await session.wait(Expectation(allOf: ["保存", "LOCAL CHANGE E2E"], timeout: 6))
            guard let textRow = editor.pick(["LOCAL CHANGE E2E"], .top) else { throw E2EError.product("编辑器内容不可见") }
            try await session.click(local: textRow.center, on: editor)
            try await session.shortcut(key: 0, modifiers: [0x37])
            try await session.type("SAVED FROM NORMA E2E")
            try await session.click(matching: ["保存"], pick: .top)
            _ = try await session.wait(Expectation(allOf: ["文件已保存"], timeout: 10))
            guard let root, try String(contentsOf: root.appendingPathComponent("NoteE2E.txt"), encoding: .utf8) == "SAVED FROM NORMA E2E" else { throw E2EError.product("保存未落盘") }
            try session.save(try await session.capture(), "03-file-saved.png")
            return "编辑保存已通过磁盘文件断言"
        }
        try await session.step("暂存并提交真实文件") {
            guard let root else { throw E2EError.env("项目夹具丢失") }
            try await session.click(matching: ["目录"], pick: .top)
            _ = try await session.wait(Expectation(allOf: ["全部暂存", "提交说明"], timeout: 8))
            try await session.click(matching: ["全部暂存"], pick: .top)
            _ = try await session.wait(Expectation(allOf: ["Git 操作完成"], timeout: 10))
            guard try ContentFixture.gitOutput(root, ["diff", "--cached", "--name-only"]).contains("NoteE2E.txt") else { throw E2EError.product("UI 暂存未更新真实索引") }
            try await session.click(matching: ["提交说明"], pick: .top)
            try await session.type("Content E2E commit")
            try await session.click(matching: ["提交已暂存文件"], pick: .top)
            _ = try await session.wait(Expectation(allOf: ["Git 操作完成"], timeout: 15))
            guard try ContentFixture.gitOutput(root, ["log", "-1", "--format=%s"]) == "Content E2E commit" else { throw E2EError.product("UI 提交未生成预期 commit") }
            try session.save(try await session.capture(), "03b-git-committed.png")
            // 窄抽屉的目录当前已打开；返回文档以复用后续目录切换步骤。
            try await session.click(matching: ["NoteE2E.txt"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["SAVED FROM NORMA E2E"], timeout: 8))
            return "真实 Git 索引和提交记录与 UI 操作一致"
        }
        try await session.step("预览 Markdown 与 HTML") {
            try await session.click(matching: ["目录"], pick: .top)
            _ = try await session.wait(Expectation(allOf: ["PlanE2E.md"], timeout: 6))
            try await session.click(matching: ["PlanE2E.md"], pick: .bottom)
            let markdown = try await session.wait(Expectation(allOf: ["MARKDOWN PREVIEW E2E", "one", "two"], timeout: 12))
            try session.save(markdown, "04-markdown-preview.png")
            try await session.click(matching: ["目录"], pick: .top)
            try await session.click(matching: ["PageE2E.html"], pick: .bottom)
            let html = try await session.wait(Expectation(allOf: ["HTML PREVIEW E2E"], timeout: 12))
            try session.save(html, "05-html-preview.png")
            return "真实 WebView 显示 Markdown 表格与 HTML 产物"
        }
        try await session.step("预览图片与视频") {
            try await session.click(matching: ["目录"], pick: .top)
            try await session.click(matching: ["PatternE2E.png"], pick: .bottom)
            let image = try await session.wait(Expectation(allOf: ["适应窗口", "实际尺寸", "放大"], timeout: 12))
            try session.save(image, "06-image-preview.png")
            try await session.click(matching: ["放大"], pick: .top)
            try session.save(try await session.capture(), "07-image-zoom.png")
            try await session.click(matching: ["目录"], pick: .top)
            try await session.click(matching: ["ClipE2E.mp4"], pick: .bottom)
            let video = try await session.wait(Expectation(allOf: ["ClipE2E.mp4"], timeout: 12))
            try session.save(video, "08-video-preview.png")
            guard let root, FileManager.default.fileExists(atPath: root.appendingPathComponent("ClipE2E.mp4").path) else { throw E2EError.product("视频夹具丢失") }
            return "图片缩放入口与系统视频控件可见；播放进度和隐藏暂停仍需看图核验"
        }
    }
}
