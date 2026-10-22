import AppKit
import E2EKit
import Fixtures
import Foundation

/// 想法输入区：⇧回车换行、正文里的链接成为可点标签、剪贴板里的图片作为附件随想法保存。
/// 换行与粘贴经控制通道送入，验证的是输入框、想法栏与存储的链路。
@MainActor
public enum WorkspaceIdeasCompose {
    public nonisolated static let name = "workspace-ideas-compose"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fakeShell: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fakeShell { try? FileManager.default.removeItem(at: fakeShell) }
        }
        let workspace = "IdeasCompose"
        let multi = "第一行\n第二行 https://example.com/docs"
        let withImage = "带图的想法"

        try await session.step("建立含项目区的隔离工作空间") {
            let shell = try FakeAgentShell.install()
            fakeShell = shell
            setenv("ZDOTDIR", shell.path, 1)
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace", "Create your first"], timeout: 25))
            try await session.act("打开创建对话框", attempts: 4, expect: Expectation(anyOf: ["工作空间名称", "Workspace name"], timeout: 3.5)) {
                let launch = try await session.capture()
                try await session.click(local: CGPoint(x: Double(launch.windowFrame.width) / 2, y: 515), on: launch)
            }
            let dialog = try await session.capture()
            guard let input = dialog.pick(["网站改版", "Website redesign"], .bottom) else {
                throw E2EError.product("找不到工作空间名称输入框：\(dialog.summary)")
            }
            try await session.click(local: input.center, on: dialog)
            try await session.type(workspace)
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 12))
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            try session.launch()
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return "工作空间已就绪"
        }

        try await session.step("进入项目并打开想法栏，折叠其余三栏") {
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search workspaces") {
                try await session.click(local: CGPoint(x: Double(launch.windowFrame.width) - 111, y: 540), on: launch)
                _ = try await session.wait(Expectation(absent: ["搜索工作空间", "Search workspaces"], timeout: 12))
            }
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            let entered = try await session.capture()
            if entered.contains("Feature Guide") || entered.contains("Skip Guide") {
                try await session.shortcut(key: 53, modifiers: [])
                _ = try await session.wait(Expectation(absent: ["Feature Guide", "Skip Guide"], timeout: 6))
            }
            try await session.shortcut(key: 34, modifiers: [0x37, 0x38])
            _ = try await session.wait(Expectation(anyOf: ["个想法，回车", "回车添加"], timeout: 8))
            for title in ["处理", "当前项目", "其他项目"] {
                let frame = try await session.capture()
                guard let heading = frame.texts.filter({ $0.value.contains(title) && $0.center.x < 440 }).first else {
                    throw E2EError.product("找不到栏标题「\(title)」：\(frame.summary)")
                }
                try await session.click(local: heading.center, on: frame)
                try await Task.sleep(nanoseconds: 400_000_000)
            }
            return try await session.capture().summary
        }

        func focusInput() async throws {
            let frame = try await session.capture()
            guard let input = frame.pick(["个想法，回车", "回车添加"], .top) else {
                throw E2EError.product("找不到想法输入框：\(frame.summary)")
            }
            try await session.click(local: input.center, on: frame)
        }

        try await session.step("⇧回车换行、回车添加：库里的正文保留换行，链接显示为标签") {
            try await focusInput()
            try await session.type("第一行")
            try await session.shortcut(key: 36, modifiers: [0x38])
            try await session.type("第二行 https://example.com/docs")
            let typing = try await session.capture()
            try session.save(typing, "01-multiline-input.png")
            try await session.shortcut(key: 36, modifiers: [])
            let after = try await session.wait(Expectation(allOf: ["example.com"], timeout: 8))
            try session.save(after, "02-link-chip.png")
            let stored = try Catalog.ideas(database: try session.databaseURL()).map(\.body)
            guard stored == [multi] else {
                throw E2EError.product("库里应只有一条两行正文 \(multi.debugDescription)，实际 \(stored.map { $0.debugDescription })")
            }
            return "已记下两行想法"
        }

        try await session.step("粘贴剪贴板里的图片：出现附件，回车后图片随想法保存") {
            let png = try Self.makePNG()
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setData(png, forType: .png)
            try await focusInput()
            try await session.shortcut(key: 9, modifiers: [0x37])
            try await Task.sleep(nanoseconds: 1_200_000_000)
            let imagesDir = try session.databaseURL().deletingLastPathComponent().appendingPathComponent("idea-images")
            let pending = (try? FileManager.default.contentsOfDirectory(atPath: imagesDir.path)) ?? []
            guard pending.count == 1 else {
                throw E2EError.product("粘贴后应在 idea-images 下保存 1 个文件，实际 \(pending)")
            }
            try session.save(try await session.capture(), "03-pending-image.png")
            try await session.type(withImage)
            try await session.shortcut(key: 36, modifiers: [])
            // 行标题的 OCR 对小字不稳定：以库里出现带图想法为准，再留一张图给人看。
            var saved: [String] = []
            for _ in 0..<16 where saved.isEmpty {
                try await Task.sleep(nanoseconds: 500_000_000)
                saved = try Catalog.ideaImages(database: try session.databaseURL())[withImage] ?? []
            }
            try session.save(try await session.capture(), "04-image-row.png")
            guard saved == pending else {
                throw E2EError.product("想法应记下已粘贴的图片 \(pending)，实际 \(saved)")
            }
            guard FileManager.default.contents(atPath: imagesDir.appendingPathComponent(saved[0]).path) == png else {
                throw E2EError.product("保存的图片内容应与剪贴板里的一致")
            }
            return "图片 \(saved[0])"
        }

        try await session.step("剪贴板只有文字时照常粘贴文字，不产生附件") {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString("粘贴的文字", forType: .string)
            try await focusInput()
            try await session.shortcut(key: 9, modifiers: [0x37])
            try await Task.sleep(nanoseconds: 1_000_000_000)
            try session.save(try await session.capture(), "05-pasted-text.png")
            let imagesDir = try session.databaseURL().deletingLastPathComponent().appendingPathComponent("idea-images")
            let files = (try? FileManager.default.contentsOfDirectory(atPath: imagesDir.path)) ?? []
            guard files.count == 1 else { throw E2EError.product("粘贴文字不应新增图片文件：\(files)") }
            try await session.shortcut(key: 36, modifiers: [])
            var found = false
            for _ in 0..<16 where !found {
                try await Task.sleep(nanoseconds: 500_000_000)
                found = try Catalog.ideas(database: try session.databaseURL()).contains { $0.body == "粘贴的文字" }
            }
            guard found else { throw E2EError.product("粘贴的文字应作为想法记下") }
            let images = try Catalog.ideaImages(database: try session.databaseURL())["粘贴的文字"] ?? []
            guard images.isEmpty else { throw E2EError.product("纯文字想法不应带图片：\(images)") }
            return "文字已粘贴并记下"
        }
    }

    private static func makePNG() throws -> Data {
        let size = 64
        guard let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
            samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
            bytesPerRow: 0, bitsPerPixel: 0
        ), let context = NSGraphicsContext(bitmapImageRep: rep) else {
            throw E2EError.env("无法创建测试图片")
        }
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = context
        NSColor.systemOrange.setFill()
        NSBezierPath(rect: NSRect(x: 0, y: 0, width: size, height: size)).fill()
        NSGraphicsContext.restoreGraphicsState()
        guard let data = rep.representation(using: .png, properties: [:]) else {
            throw E2EError.env("无法编码测试图片")
        }
        return data
    }
}
