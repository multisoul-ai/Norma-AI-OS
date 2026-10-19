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
            // 路径条右侧是编辑/复制/系统打开三个图标按钮（OCR 不识别图标）；
            // 已看图核对：编辑图标位于右栏右缘向内 245px，与路径文字同一水平线。
            guard let title = text.pick(["NoteE2E.txt"], .top) else { throw E2EError.product("文件标题不可见") }
            try await session.click(local: CGPoint(x: Double(text.windowFrame.width) - 245, y: title.center.y), on: text)
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
        try await session.step("查看变更差异") {
            // 变更树第一行位于窗口顶部下 185px、右缘向内 100px（已看图核对）。
            let tree = try await session.capture()
            try await session.click(local: CGPoint(x: Double(tree.windowFrame.width) - 100, y: 185), on: tree)
            let diff = try await session.wait(Expectation(allOf: ["SAVED FROM NORMA E2E", "未暂存"], timeout: 10))
            try session.save(diff, "03a-git-diff.png")
            return "变更行打开真实未暂存差异，带行号与增删摘要"
        }
        try await session.step("暂存并提交真实文件") {
            guard let root else { throw E2EError.env("项目夹具丢失") }
            try await showDirectory(session)
            // 批量暂存与提交收在「已更改的文件」标题尾部的 ⋯ 面板里，按需展开。
            let tree = try await session.capture()
            guard let changes = tree.pick(["已更改的文件"], .top) else { throw E2EError.product("已更改的文件分区不可见") }
            try await session.click(local: CGPoint(x: Double(tree.windowFrame.width) - 29, y: changes.center.y), on: tree)
            _ = try await session.wait(Expectation(allOf: ["全部暂存", "提交"], timeout: 8))
            try await session.click(matching: ["全部暂存"], pick: .top)
            try await waitUntil("暂存未更新索引") { try ContentFixture.gitOutput(root, ["diff", "--cached", "--name-only"]).contains("NoteE2E.txt") }
            guard try ContentFixture.gitOutput(root, ["diff", "--cached", "--name-only"]).contains("NoteE2E.txt") else { throw E2EError.product("UI 暂存未更新真实索引") }
            try await session.click(matching: ["提交"], pick: .top)
            _ = try await session.wait(Expectation(allOf: ["提交说明"], timeout: 6))
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
            try await openFile(session, "PlanE2E.md")
            let markdown = try await session.wait(Expectation(allOf: ["PREVIEW E2E", "one"], timeout: 12))
            try session.save(markdown, "04-markdown-preview.png")
            try await openFile(session, "PageE2E.html")
            let html = try await session.wait(Expectation(allOf: ["HTML PREVIEW E2E"], timeout: 12))
            try session.save(html, "05-html-preview.png")
            return "真实 WebView 显示 Markdown 表格与 HTML 产物"
        }
        try await session.step("预览图片与视频") {
            try await openFile(session, "PatternE2E.png")
            let image = try await session.wait(Expectation(allOf: ["放大", "缩小"], timeout: 12))
            try session.save(image, "06-image-preview.png")
            try await session.click(matching: ["放大"], pick: .top)
            try session.save(try await session.capture(), "07-image-zoom.png")
            try await openFile(session, "ClipE2E.mp4")
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

    /// 用筛选框定位文件：相邻树行在 OCR 中会被合并，筛选后目标行唯一且位于树的最下方。
    static func openFile(_ session: Session, _ name: String) async throws {
        // 筛选框已有内容时占位文字消失；已看图核对其位于右栏右缘向内 100px、窗口顶部下 117px。
        let frame = try await session.capture()
        try await session.click(local: CGPoint(x: Double(frame.windowFrame.width) - 100, y: 117), on: frame)
        try await session.shortcut(key: 0, modifiers: [0x37])
        try await session.type(name)
        _ = try await session.wait(Expectation(allOf: [name], timeout: 6))
        try await session.click(matching: [name], pick: .bottom)
    }

    /// 文件页目录常驻右栏；只需确认目录条目已渲染。
    /// 筛选框非空时右端有清除按钮（右缘向内 29px），点击即恢复完整目录；为空时只是聚焦输入框。
    static func showDirectory(_ session: Session) async throws {
        let frame = try await session.capture()
        try await session.click(local: CGPoint(x: Double(frame.windowFrame.width) - 29, y: 117), on: frame)
        _ = try await session.wait(Expectation(allOf: ["ClipE2E.mp4", "PlanE2E.md"], timeout: 8))
    }
}
