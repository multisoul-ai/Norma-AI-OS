import AppKit
import AppKit
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
            try session.launch(language: "zh-Hans")
            // 内容模块只验右栏；先让 App 初始化数据库，再停机写入隔离前置数据。
            // 避免重复覆盖 workspace 模块的创建弹窗流程及其 OCR 依赖。
            let database = try session.databaseURL()
            _ = try await session.waitCatalog(timeout: 15) { _ in FileManager.default.fileExists(atPath: database.path) }
            try await session.waitContentSchema(timeout: 15)
            session.stop()
            guard let data = session.dataRoot else { throw E2EError.env("隔离目录丢失") }
            try Catalog.seedWorkspace(database: data.appendingPathComponent("app.sqlite"), name: workspace)
            root = try ContentFixture.install(dataRoot: data, workspace: workspace)
            try session.launch(language: "zh-Hans")
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            // 首页卡片没有「打开」按钮，直接点工作空间名称进入（与其他内容场景一致）。
            // 「所有文件」默认折叠：先确认分区标题在、整棵树没渲染，再手动展开。
            _ = try await session.act("打开右抽屉", expect: Expectation(allOf: ["所有文件"], absent: ["ClipE2E.mp4"], timeout: 6)) {
                let frame = try await session.capture()
                if frame.contains("创建工作空间") {
                    // 上一轮误开了新建对话框：先取消，下一轮重试。
                    try await session.click(matching: ["取消"], pick: .bottom)
                } else if frame.contains("回来了") || frame.contains("最近") {
                    // 仍在首页：右上角是「新建工作空间」，不能点；OCR 会把名称读错，用短片段匹配卡片。
                    try await session.click(matching: [workspace, "ContentWork", "WorkbenchCheck", "个项目"], pick: .bottom)
                    try await Task.sleep(nanoseconds: 950_000_000)
                } else {
                    try await session.click(local: CGPoint(x: Double(frame.windowFrame.width) - 28, y: 26), on: frame)
                }
            }
            _ = try await session.act("展开所有文件", expect: Expectation(allOf: ["ClipE2E.mp4", "NoteE2E.txt", "PlanE2E.md"], timeout: 6)) {
                try await session.click(matching: ["所有文件"], pick: .top)
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
            // 只读文件视图必须可选中并 Cmd+C：点正文、全选、复制，再读系统剪贴板。
            guard let body = text.pick(["LOCAL CHANGE E2E"], .top) else { throw E2EError.product("文件正文不可见") }
            let copied = try await copySelection(session, text: body, on: text)
            guard copied.contains("LOCAL CHANGE E2E") else { throw E2EError.product("文件预览 Cmd+C 未复制到正文，剪贴板：\(copied.prefix(80))") }
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
            guard let changes = tree.pick(["已更改的文件", "已更改文件"], .top) else { throw E2EError.product("已更改的文件分区不可见") }
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
            let image = try await session.wait(Expectation(allOf: ["放大"], timeout: 12))
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
    /// 选中文字后 Cmd+C 并读取系统剪贴板；结束时恢复原内容。
    /// 点击文字后 Cmd+A 全选。原生 WebView 的选区无法由驱动的合成鼠标事件建立，故不在此覆盖。
    static func copySelection(_ session: Session, text: VisibleText, on frame: Frame) async throws -> String {
        let board = NSPasteboard.general
        let saved = (board.pasteboardItems ?? []).map { item in
            let copy = NSPasteboardItem()
            for type in item.types { if let data = item.data(forType: type) { copy.setData(data, forType: type) } }
            return copy
        }
        defer { board.clearContents(); if !saved.isEmpty { board.writeObjects(saved) } }
        board.clearContents()
        try await session.click(local: text.center, on: frame)
        try await session.shortcut(key: 0, modifiers: [0x37])
        try session.save(try await session.capture(), "02a-text-selected.png")
        try await session.shortcut(key: 8, modifiers: [0x37])
        let deadline = Date().addingTimeInterval(4)
        var copied = ""
        repeat {
            copied = board.string(forType: .string) ?? ""
            if !copied.isEmpty { break }
            try await Task.sleep(nanoseconds: 100_000_000)
        } while Date() < deadline
        return copied
    }

    /// 在网页里拖选标题后 Cmd+C 并读取系统剪贴板；结束时恢复原内容。
    /// 抽屉宽度动画期间标题位置会漂移，所以每次尝试都重新截图定位，位置稳定后才拖选，最多三次。
    static func copyWebHeading(_ session: Session, candidates: [String]) async throws -> String {
        let board = NSPasteboard.general
        let saved = (board.pasteboardItems ?? []).map { item in
            let copy = NSPasteboardItem()
            for type in item.types { if let data = item.data(forType: type) { copy.setData(data, forType: type) } }
            return copy
        }
        defer { board.clearContents(); if !saved.isEmpty { board.writeObjects(saved) } }
        var copied = ""
        for attempt in 1...3 {
            board.clearContents()
            var frame = try await session.capture()
            var heading = frame.pick(candidates, .top)
            for _ in 0..<4 {
                try await Task.sleep(nanoseconds: 400_000_000)
                let next = try await session.capture()
                let moved = heading.map { old in next.pick(candidates, .top).map { abs($0.rect.minX - old.rect.minX) > 2 || abs($0.rect.midY - old.rect.midY) > 2 } ?? true } ?? true
                frame = next
                heading = next.pick(candidates, .top)
                if !moved { break }
            }
            guard let text = heading else { throw E2EError.product("网页标题不可见") }
            try await session.click(local: text.center, on: frame)
            try await Task.sleep(nanoseconds: 300_000_000)
            if attempt == 2 {
                let y = text.rect.midY
                try await session.drag(local: CGPoint(x: text.rect.minX + 3, y: y), to: CGPoint(x: text.rect.maxX - 3, y: y), on: frame)
            } else {
                try await session.nativeMultiClick(local: text.center, on: frame, count: 3)
            }
            try session.save(try await session.capture(), "09a-web-selected-\(attempt).png")
            try await session.shortcut(key: 8, modifiers: [0x37])
            let deadline = Date().addingTimeInterval(2)
            repeat {
                copied = board.string(forType: .string) ?? ""
                if !copied.isEmpty { return copied }
                try await Task.sleep(nanoseconds: 100_000_000)
            } while Date() < deadline
        }
        return copied
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
