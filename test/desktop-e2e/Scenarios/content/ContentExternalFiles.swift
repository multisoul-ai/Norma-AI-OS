import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 普通文件标签绑定项目 A，但资源位于独立目录 B；覆盖恢复、媒体播放与双语主题。
@MainActor
public enum ContentExternalFiles {
    public nonisolated static let name = "content-external-files"

    public static func run(_ session: Session) async throws {
        guard session.visualReview else { throw E2EError.env("此场景使用 --visual 模型看图") }
        let workspace = "ExternalFiles"
        var fixture: URL?
        defer { session.stop(); if let fixture { try? FileManager.default.removeItem(at: fixture) } }
        var database: URL?
        try await session.step("建立项目外文件与普通文件标签") {
            try session.launch(language: "zh-Hans")
            let db = try session.databaseURL()
            database = db
            _ = try await session.waitCatalog(timeout: 20) { _ in FileManager.default.fileExists(atPath: db.path) }
            session.stop()
            let root = URL(fileURLWithPath: "/private/tmp/norma-external-files-\(UUID().uuidString)")
            fixture = root
            let project = root.appendingPathComponent("project")
            let external = root.appendingPathComponent("external files")
            for directory in [project, external] {
                try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            }
            try Data("EXTERNAL TEXT DOCUMENT\n项目外文档正文".utf8).write(to: external.appendingPathComponent("note.txt"))
            try Data("# EXTERNAL MARKDOWN\n\nExternal document body.".utf8).write(to: external.appendingPathComponent("note.md"))
            try Data("<h1>EXTERNAL HTML</h1>".utf8).write(to: external.appendingPathComponent("page.html"))
            try ContentMediaFixture.install(external)
            try Catalog.seedWorkspace(database: db, name: workspace)
            try Catalog.seedArea(database: db, workspaceName: workspace, root: project, displayName: "Source Project")
            try Catalog.suppressFeatureGuide(database: db)
            for (index, file) in ["note.txt", "note.md", "page.html", "PatternE2E.png", "ClipE2E.mp4"].enumerated() {
                try Catalog.seedContentTab(database: db, workspaceName: workspace, sourceAreaName: "Source Project", kind: "file",
                    resource: external.appendingPathComponent(file).path, title: file, position: index, selected: index == 0)
            }
            return "项目与外部目录互不包含；标签无显式 Present 类型"
        }
        guard let database else { throw E2EError.env("隔离数据库缺失") }
        for (language, theme) in [("zh-Hans", "sapphire-night"), ("en", "paper-blue")] {
            try await session.step("\(language)：恢复外部文本标签") {
                session.stop()
                try VoiceConfigurationFixture.theme(database: database, id: theme)
                try session.launch(language: language)
                try await session.resize(width: 1440, height: 800)
                try await session.visualClick("点击工作空间 ExternalFiles 的卡片名称，进入工作空间")
                try await session.visualClick("如果右侧内容抽屉未展开，点击窗口右上角的内容抽屉开关；如果已展开，点击 note.txt 文件标签")
                let divider = try await session.visualLocate("定位右侧内容抽屉左边缘可拖动的分隔线，用于扩大预览区")
                try await session.drag(local: divider.point, to: CGPoint(x: 440, y: divider.point.y), on: divider.frame)
                try await session.visualClick("点击右侧内容抽屉的 note.txt 标签，不点击关闭按钮")
                let frame = try await session.visualCheck([
                    VisualCriterion(id: "text", text: "右侧显示 EXTERNAL TEXT DOCUMENT 与项目外文档正文，没有文件位于项目外部、outside the source project 或持续加载错误")
                ])
                try session.save(frame, "\(language)-text.png")
                return "外部文本恢复并展示"
            }
            for (file, criterion) in [
                ("note.md", "右侧文档以标题格式显示 EXTERNAL MARKDOWN，下方显示 External document body.，没有原始 # 前缀或项目外错误"),
                ("page.html", "右侧网页显示大标题 EXTERNAL HTML，而不是 HTML 源代码或项目外错误"),
                ("PatternE2E.png", "右侧预览显示蓝紫底色和黄色方块的图片，没有破图或项目外错误"),
                ("ClipE2E.mp4", "右侧显示可播放的视频预览和播放控件，没有项目外错误或不支持错误")
            ] {
                try await session.step("\(language)：\(file) 外部预览") {
                    try await session.visualClick("点击右侧内容抽屉的 \(file) 标签，避免点击关闭按钮")
                    let frame = try await session.visualCheck([VisualCriterion(id: "content", text: criterion)])
                    try session.save(frame, "\(language)-\(file).png")
                    let tabs = try session.contentTabs(workspaceName: workspace)
                    guard tabs.contains(where: { $0.title == file && $0.selected && $0.kind == "file" }) else {
                        throw E2EError.product("实际选中标签未持久化：\(file)")
                    }
                    return "外部文件预览可见，标签状态已保存"
                }
            }
            try await session.step("\(language)：视频实际播放") {
                try await session.visualClick("点击右侧视频播放器的播放按钮")
                try await Task.sleep(nanoseconds: 1_200_000_000)
                let first = try await session.visualCheck([VisualCriterion(id: "playing", text: "视频处于播放状态，画面为蓝紫背景与黄色块，控件显示非零已播放时间")])
                try session.save(first, "\(language)-playing-a.png")
                try await Task.sleep(nanoseconds: 1_500_000_000)
                let second = try await session.visualCheck([VisualCriterion(id: "advanced", text: "对比上一张 playing-a，视频时间已推进或已正常播完 12 秒，画面没有解码错误")])
                try session.save(second, "\(language)-playing-b.png")
                return "真实播放器时间推进"
            }
        }
    }
}
