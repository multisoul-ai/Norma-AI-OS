import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 从真实终端点击外部路径打开，不预置文件标签；覆盖双语主题与 OS 打开边界。
@MainActor
public enum ContentExternalFiles {
    public nonisolated static let name = "content-external-files"
    public nonisolated static let videoName = "content-external-video"

    public static func run(_ session: Session, videoOnly: Bool = false) async throws {
        guard session.visualReview else { throw E2EError.env("此场景使用 --visual 模型看图") }
        let fixture = try ExternalFilesFixture.install()
        let oldPath = ProcessInfo.processInfo.environment["PATH"] ?? "/usr/bin:/bin"
        let oldZdot = ProcessInfo.processInfo.environment["ZDOTDIR"]
        setenv("ZDOTDIR", fixture.home.path, 1)
        let oldLog = ProcessInfo.processInfo.environment["NORMA_EXTERNAL_OPEN_LOG"]
        setenv("PATH", "\(fixture.root.appendingPathComponent("bin").path):\(oldPath)", 1)
        setenv("NORMA_EXTERNAL_OPEN_LOG", fixture.receipts.path, 1)
        defer {
            session.stop()
            setenv("PATH", oldPath, 1)
            if let oldZdot { setenv("ZDOTDIR", oldZdot, 1) } else { unsetenv("ZDOTDIR") }
            if let oldLog { setenv("NORMA_EXTERNAL_OPEN_LOG", oldLog, 1) } else { unsetenv("NORMA_EXTERNAL_OPEN_LOG") }
            try? FileManager.default.removeItem(at: fixture.root)
        }
        var database: URL?
        try await session.step("建立真实文件与空内容标签的两个工作空间") {
            try session.launch(language: "zh-Hans", homeDirectory: fixture.home)
            let db = try session.databaseURL()
            database = db
            _ = try await session.waitCatalog(timeout: 20) { _ in FileManager.default.fileExists(atPath: db.path) }
            session.stop()
            for name in ["ExternalFiles-ZH", "ExternalFiles-EN"] { try fixture.seed(database: db, workspace: name) }
            return "项目外路径含中文和空格；没有预置 file 标签；PDF/Word 使用真实文档"
        }
        guard let database else { throw E2EError.env("隔离数据库缺失") }
        let variants = [("zh-Hans", "sapphire-night", "ExternalFiles-ZH"), ("en", "ink-black", "ExternalFiles-EN")]
        for (language, theme, workspace) in variants {
            try await session.step("\(language)：进入项目并让终端输出外部文件路径") {
                session.stop()
                try VoiceConfigurationFixture.theme(database: database, id: theme)
                try session.launch(language: language, homeDirectory: fixture.home)
                try await session.resize(width: 1440, height: 800)
                try await session.visualClick("点击工作空间 \(workspace) 的卡片名称进入，不要点击另一工作空间")
                try await session.visualClick("点击 Source Project 项目标题进入项目画布")
                try await session.shortcut(key: 18, modifiers: [0x37])
                try await Task.sleep(nanoseconds: 1_500_000_000)
                try await session.type("/bin/sh \(fixture.script.path)")
                try await session.shortcut(key: 36, modifiers: [])
                _ = try await session.visualCheck([VisualCriterion(id: "terminal", text: "真实终端显示 EXTERNAL PATH LINKS，以及项目外含中文和空格的 note.txt、note.md、page.html、PatternE2E.png、ClipE2E.mp4、ReportE2E.pdf、OfficeE2E.docx 路径；没有命令执行错误")])
                guard try session.contentTabs(workspaceName: workspace).isEmpty else { throw E2EError.product("路径点击前不应已有内容标签") }
                return "实际 PTY 执行脚本，文件路径已输出，内容标签为空"
            }
            let files = videoOnly ? ["ClipE2E.mp4"] : ExternalFilesFixture.inlineFiles
            for (index, file) in files.enumerated() {
                try await session.step("\(language)：点击外部 \(file) 路径并验证预览") {
                    try await session.visualClick("点击左侧真实终端输出中的 \(file) 文件路径；不要点击右侧文件标签或终端标题，路径可能换行")
                    let frame = try await session.visualCheck([VisualCriterion(id: "content", text: criterion(file))])
                    try session.save(frame, "\(language)-\(file).png")
                    let tabs = try session.contentTabs(workspaceName: workspace)
                    let path = fixture.external.appendingPathComponent(file).path
                    guard tabs.count == index + 1, tabs.contains(where: { $0.resource == path && $0.selected && $0.kind == "file" }) else {
                        throw E2EError.product("点击真实终端路径后未创建并选中准确文件标签：\(file)，实际 \(tabs)")
                    }
                    guard fixture.receiptRows().isEmpty else { throw E2EError.product("内置预览格式不应启动系统应用") }
                    return "路径点击 → 新文件标签 → 实际预览；SQLite 资源绝对路径和 selected 精确匹配"
                }
            }
            try await ContentExternalPlayback.verify(session, language: language)
            // 系统应用行为与语言无关；在第二种语言完成后验证，避免打开回执污染下一轮负断言。
            if language == "en" && !videoOnly {
                for file in ExternalFilesFixture.systemFiles {
                    try await session.step("点击外部 \(file) 路径交给真实系统应用") {
                        let before = fixture.receiptRows().count
                        let tabsBefore = try session.contentTabs(workspaceName: workspace)
                        try await session.visualClick("点击左侧真实终端输出中的 \(file) 路径，不点击其他内容")
                        let evidence = try await fixture.waitForSystemOpen(file, after: before)
                        guard try session.contentTabs(workspaceName: workspace) == tabsBefore else { throw E2EError.product("系统打开格式不得创建错误的预览标签") }
                        return evidence
                    }
                }
                if let output = session.output { try FileManager.default.copyItem(at: fixture.receipts, to: output.appendingPathComponent("system-open.jsonl")) }
            }
        }
    }

    private static func criterion(_ file: String) -> String {
        let common = "右侧内容抽屉选中文件 \(file)，没有项目外错误、持续加载或破图；"
        switch file {
        case "note.txt": return common + "正文清楚显示 EXTERNAL TEXT DOCUMENT 与项目外文档正文"
        case "note.md": return common + "EXTERNAL MARKDOWN 是渲染后的标题，没有原始 #，下面有 External document body."
        case "page.html": return common + "网页大标题 EXTERNAL HTML 可见，未显示 HTML 源码"
        case "PatternE2E.png": return common + "图片为蓝紫色底、左侧黄色方块，适应窗口显示"
        default: return common + "视频播放器及播放按钮可见，尚未播放，不是错误页"
        }
    }
}
