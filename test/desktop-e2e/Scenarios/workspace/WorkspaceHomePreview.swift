import E2EKit
import Fixtures
import Foundation

@MainActor
public enum WorkspaceHomePreview {
    public nonisolated static let name = "workspace-home-preview"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("初始化隔离数据库") {
            try session.launch(language: "zh-Hans")
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            session.stop()
            return "仅操作本场景数据"
        }
        for (theme, label, alternate) in [("sapphire-night", "dark", false), ("graphite", "graphite", true)] {
            try await session.step("\(label)：真实保存节点的示意预览") {
                try HomeDesignFixture.install(database: session.databaseURL(), count: 12, theme: theme, alternate: alternate)
                try HomeDesignFixture.seedPreviewNodes(database: session.databaseURL())
                try session.launch(language: "zh-Hans")
                try await session.resize(width: 1280, height: 800)
                let frame = try await session.wait(Expectation(allOf: ["最近访问", "Norma OS", "官网改版"], timeout: 25))
                try session.save(frame, "\(label)-01-previews.png")
                return "首卡3窗口左大右两小；二卡两个项目共5窗口；三卡三个项目共6窗口；无真实终端文字"
            }
            try await session.step("\(label)：目录四方向浏览与跨屏滚动") {
                try await session.shortcut(key: 126, modifiers: [0x37, 0x38])
                _ = try await session.wait(Expectation(allOf: ["Norma OS", "iOS 伴侣 App"], absent: ["最近访问"], timeout: 8))
                // 与工作空间快捷键场景一致：等相机转场结束后再发送下一组输入。
                try await Task.sleep(nanoseconds: 700_000_000)
                // ↓首项，→1，↓4，←3，↑0；接着三次↓落在第4行第1列索引9。
                for key: UInt16 in [125, 124, 125, 123, 126] {
                    try await session.shortcut(key: key, modifiers: [])
                }
                // 原始按键ACK先于WindowServer合成帧；截图须等末次↑的选中环上屏。
                try await Task.sleep(nanoseconds: 150_000_000)
                let selected = try await session.capture()
                try session.save(selected, "\(label)-02-selected-first.png")
                try await captureMaterialStates(session, frame: selected, title: "Norma OS", prefix: "\(label)-02-workspace")
                // 用实际进入的空间验证四方向终点，避免只因文字可见就把错误选中态判为通过。
                try await session.shortcut(key: 125, modifiers: [0x37])
                _ = try await session.wait(Expectation(allOf: ["Norma OS", "所有项目"], absent: ["推送中继", "性能调优"], timeout: 12))
                try await Task.sleep(nanoseconds: 700_000_000)
                try await session.shortcut(key: 125, modifiers: [])
                try await Task.sleep(nanoseconds: 150_000_000)
                let project = try await session.capture()
                try session.save(project, "\(label)-02-project-selected.png")
                try await captureMaterialStates(session, frame: project, title: "Project 1", prefix: "\(label)-02-project")
                try await session.shortcut(key: 14, modifiers: [0x37, 0x38])
                _ = try await session.wait(Expectation(allOf: ["运行中", "待开始"], timeout: 6))
                try await Task.sleep(nanoseconds: 700_000_000)
                try session.save(try await session.capture(), "\(label)-02-project-drawer.png")
                try await session.shortcut(key: 126, modifiers: [0x37, 0x38])
                _ = try await session.wait(Expectation(allOf: ["Norma OS", "iOS 伴侣 App"], absent: ["所有项目"], timeout: 8))
                try await Task.sleep(nanoseconds: 700_000_000)
                for _ in 0..<3 { try await session.shortcut(key: 125, modifiers: []) }
                let bottom = try await session.wait(Expectation(allOf: ["性能调优"], timeout: 5))
                try session.save(bottom, "\(label)-03-selected-bottom.png")
                return "看图验证首卡和性能调优的选中环，跨屏后目标完整可见"
            }
            try await session.step("\(label)：⌘↓打开高亮空间") {
                try await session.shortcut(key: 125, modifiers: [0x37])
                let opened = try await session.wait(Expectation(allOf: ["性能调优"], absent: ["最近访问", "插件市场", "发布流水线"], timeout: 12))
                try session.save(opened, "\(label)-04-opened.png")
                session.stop()
                return "进入性能调优而不是当前空间或相邻项目"
            }
        }
    }

    private static func captureMaterialStates(_ session: Session, frame: Frame, title: String, prefix: String) async throws {
        guard let label = frame.pick([title], .top) else {
            throw E2EError.product("材质验收未找到卡片标题 \(title)：\(frame.summary)")
        }
        let point = CGPoint(x: label.center.x, y: label.rect.maxY + 12)
        try await session.hover(local: point, on: frame)
        try session.save(try await session.capture(), "\(prefix)-hover.png")
        try session.save(try await session.capturePressed(local: point, on: frame), "\(prefix)-pressed.png")
    }
}
