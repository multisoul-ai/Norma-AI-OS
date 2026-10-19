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
            // 内容模块只验右栏；先让 App 初始化数据库，再停机写入隔离前置数据。
            // 避免重复覆盖 workspace 模块的创建弹窗流程及其 OCR 依赖。
            let database = try session.databaseURL()
            _ = try await session.waitCatalog(timeout: 15) { _ in FileManager.default.fileExists(atPath: database.path) }
            session.stop()
            guard let data = session.dataRoot else { throw E2EError.env("隔离目录丢失") }
            try Catalog.seedWorkspace(database: data.appendingPathComponent("app.sqlite"), name: workspace)
            root = try ContentFixture.install(dataRoot: data, workspace: workspace)
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            let frame = try await session.capture()
            // 新建隔离库没有上次打开空间；卡片打开按钮的位置由失败截图核对。
            let open = frame.pick(["打开", "Open"], .bottom)?.center
                ?? CGPoint(x: frame.windowFrame.width - 112, y: 540)
            try await session.click(local: open, on: frame)
            try await Task.sleep(nanoseconds: 950_000_000)
            _ = try await session.act("展开文件工作台", expect: Expectation(allOf: ["ClipE2E.mp4", "NoteE2E.txt", "PlanE2E.md"], timeout: 6)) {
                let frame = try await session.capture()
                try await session.click(local: CGPoint(x: Double(frame.windowFrame.width) - 28, y: 26), on: frame)
            }
            // 动画期间文字已出现，但仍需等抽屉到位再保存视觉证据。
            try await Task.sleep(nanoseconds: 450_000_000)
            try session.save(try await session.capture(), "01-file-tree.png")
            guard try session.contentTabs(workspaceName: workspace).contains(where: { $0.kind == "project_files" }) else { throw E2EError.product("工作台未持久化") }
            return "真实窗口显示目录树和 Git 变更，工作台已恢复"
        }
        try await session.step("查看并编辑文本") {
            try await session.click(matching: ["NoteE2E.txt"], pick: .bottom)
            let text = try await session.wait(Expectation(allOf: ["LOCAL CHANGE E2E"], timeout: 10))
            try session.save(text, "02-text-preview.png")
            // 小按钮的两字标签会被 Vision 漏识；依据同一工具栏的文件标题定位。
            guard let title = text.pick(["NoteE2E.txt"], .top) else { throw E2EError.product("文件标题不可见") }
            try await session.click(local: CGPoint(x: Double(text.windowFrame.width) - 74, y: title.center.y), on: text)
            let editor = try await session.wait(Expectation(allOf: ["LOCAL CHANGE E2E"], timeout: 6))
            guard let textRow = editor.pick(["LOCAL CHANGE E2E"], .top) else { throw E2EError.product("编辑器内容不可见") }
            try await session.click(local: textRow.center, on: editor)
            try await session.shortcut(key: 0, modifiers: [0x37])
            try await session.type("SAVED FROM NORMA E2E")
            // 使用已注册的原生 Cmd-S；磁盘内容断言验证保存已执行。
            try await session.shortcut(key: 1, modifiers: [0x37])
            guard let root else { throw E2EError.env("项目夹具丢失") }
            try await waitUntil("保存未落盘") { try String(contentsOf: root.appendingPathComponent("NoteE2E.txt"), encoding: .utf8) == "SAVED FROM NORMA E2E" }
            guard try String(contentsOf: root.appendingPathComponent("NoteE2E.txt"), encoding: .utf8) == "SAVED FROM NORMA E2E" else { throw E2EError.product("保存未落盘") }
            try session.save(try await session.capture(), "03-file-saved.png")
            return "编辑保存已通过磁盘文件断言"
        }
        try await session.step("暂存并提交真实文件") {
            guard let root else { throw E2EError.env("项目夹具丢失") }
            try await showDirectory(session)
            _ = try await session.wait(Expectation(allOf: ["全部暂存", "提交说明"], timeout: 8))
            try await session.click(matching: ["全部暂存"], pick: .top)
            try await waitUntil("暂存未更新索引") { try ContentFixture.gitOutput(root, ["diff", "--cached", "--name-only"]).contains("NoteE2E.txt") }
            guard try ContentFixture.gitOutput(root, ["diff", "--cached", "--name-only"]).contains("NoteE2E.txt") else { throw E2EError.product("UI 暂存未更新真实索引") }
            try await session.click(matching: ["提交说明"], pick: .top)
            try await session.type("Content E2E commit")
            try await session.click(matching: ["提交已暂存文件"], pick: .top)
            try await waitUntil("提交未生成记录") { try ContentFixture.gitOutput(root, ["log", "-1", "--format=%s"]) == "Content E2E commit" }
            guard try ContentFixture.gitOutput(root, ["log", "-1", "--format=%s"]) == "Content E2E commit" else { throw E2EError.product("UI 提交未生成预期 commit") }
            try session.save(try await session.capture(), "03b-git-committed.png")
            // 窄抽屉的目录当前已打开；返回文档以复用后续目录切换步骤。
            try await session.click(matching: ["NoteE2E.txt"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["SAVED FROM NORMA E2E"], timeout: 8))
            return "真实 Git 索引和提交记录与 UI 操作一致"
        }
        try await session.step("预览 Markdown 与 HTML") {
            try await showDirectory(session)
            _ = try await session.wait(Expectation(allOf: ["PlanE2E.md"], timeout: 6))
            try await session.click(matching: ["PlanE2E.md"], pick: .bottom)
            let markdown = try await session.wait(Expectation(allOf: ["MARKDOWN PREVIEW E2E", "one", "two"], timeout: 12))
            try session.save(markdown, "04-markdown-preview.png")
            try await showDirectory(session)
            try await session.click(matching: ["PageE2E.html"], pick: .bottom)
            let html = try await session.wait(Expectation(allOf: ["HTML PREVIEW E2E"], timeout: 12))
            try session.save(html, "05-html-preview.png")
            return "真实 WebView 显示 Markdown 表格与 HTML 产物"
        }
        try await session.step("预览图片与视频") {
            try await showDirectory(session)
            try await session.click(matching: ["PatternE2E.png"], pick: .bottom)
            let image = try await session.wait(Expectation(allOf: ["放大", "缩小"], timeout: 12))
            try session.save(image, "06-image-preview.png")
            try await session.click(matching: ["放大"], pick: .top)
            try session.save(try await session.capture(), "07-image-zoom.png")
            try await showDirectory(session)
            try await session.click(matching: ["ClipE2E.mp4"], pick: .bottom)
            let video = try await session.wait(Expectation(allOf: ["ClipE2E.mp4"], timeout: 12))
            try session.save(video, "08-video-preview.png")
            // 系统播放器的中央播放按钮已通过截图定位；随后用两帧核验真实解码推进。
            try await session.click(local: CGPoint(x: video.windowFrame.width - 236, y: 497), on: video)
            try await Task.sleep(nanoseconds: 650_000_000)
            try session.save(try await session.capture(), "08a-video-playing.png")
            try await Task.sleep(nanoseconds: 650_000_000)
            try session.save(try await session.capture(), "08b-video-progress.png")
            guard let root, FileManager.default.fileExists(atPath: root.appendingPathComponent("ClipE2E.mp4").path) else { throw E2EError.product("视频夹具丢失") }
            return "图片缩放与系统播放器已操作；关键截图记录视频帧推进，返回标签核验隐藏暂停"
        }
        try await browser(session, workspace: workspace)
    }
    private static func waitUntil(_ message: String, ready: () throws -> Bool) async throws {
        let deadline = Date().addingTimeInterval(12)
        while Date() < deadline {
            if try ready() { return }
            try await Task.sleep(nanoseconds: 150_000_000)
        }
        throw E2EError.product(message)
    }

    static func showDirectory(_ session: Session) async throws {
        _ = try await session.act("展开项目目录", expect: Expectation(allOf: ["ClipE2E.mp4", "PlanE2E.md"], timeout: 5)) {
            let frame = try await session.capture()
            if let title = frame.pick(["目录"], .top) {
                // Vision 可能把目录和文件名合成一行，只点击文字行左端的目录按钮。
                try await session.click(local: CGPoint(x: title.rect.minX + 12, y: title.center.y), on: frame)
            } else {
                // 本场景 Git 回执始终可见；已看图核对目录位于右栏左侧 x+32、y=160。
                try await session.click(local: CGPoint(x: frame.windowFrame.width - 440, y: 160), on: frame)
            }
        }
    }
}
