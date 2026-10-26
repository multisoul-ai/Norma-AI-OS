import E2EKit
import Foundation

/// 工作空间切换键位互换：⇧⌘↑ 打开切换器，⇧⌘K 回到全部工作空间；Dock 提示与速查表同步。
@MainActor
public enum WorkspaceSwitchShortcuts {
    public nonisolated static let name = "workspace-switch-shortcuts"

    /// 目录页独有「Continue working」；切换器独有每张空间卡里的「A canvas for your projects」。
    private static let directoryMarker = "Continue working"
    private static let switcherMarker = "A canvas for your projects"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("建立两个空工作空间并关闭功能引导") {
            try session.launch(language: "en")
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            session.stop()
            let database = try session.databaseURL()
            try Catalog.suppressFeatureGuide(database: database)
            try Catalog.seedWorkspace(database: database, name: "Alpha")
            try Catalog.seedWorkspace(database: database, name: "Beta")
            return "已写入 Alpha、Beta"
        }
        for (theme, label) in [("paper-blue", "light"), ("sapphire-night", "dark")] {
            try await session.step("\(label)：启动后停在全部工作空间目录") {
                try Catalog.seedSetting(database: try session.databaseURL(), key: "appearance.theme.v1", value: theme)
                try session.launch(language: "en")
                let frame = try await session.wait(Expectation(allOf: ["Beta", directoryMarker], timeout: 25))
                try expect(frame, present: [directoryMarker], absent: [switcherMarker], context: "启动目录")
                try session.save(frame, "\(label)-01-directory.png")
                return frame.summary
            }
            try await session.step("\(label)：⇧⌘↑ 打开切换器而不是目录") {
                try await session.shortcut(key: 126, modifiers: [0x37, 0x38])
                let frame = try await session.wait(Expectation(allOf: [switcherMarker], timeout: 6))
                try await Task.sleep(nanoseconds: 700_000_000)
                let settled = try await session.capture()
                try expect(settled, present: [switcherMarker], absent: [directoryMarker], context: "⇧⌘↑ 之后")
                try session.save(settled, "\(label)-02-switcher.png")
                return frame.summary
            }
            try await session.step("\(label)：⇧⌘K 回到全部工作空间目录") {
                try await session.shortcut(key: 40, modifiers: [0x37, 0x38])
                let frame = try await session.wait(Expectation(allOf: [directoryMarker], timeout: 6))
                try await Task.sleep(nanoseconds: 700_000_000)
                let settled = try await session.capture()
                try expect(settled, present: [directoryMarker], absent: [switcherMarker], context: "⇧⌘K 之后")
                try session.save(settled, "\(label)-03-all-workspaces.png")
                return frame.summary
            }
            try await session.step("\(label)：进入 Alpha 后 Dock 悬浮提示显示 ⌘⇧↑") {
                try await session.click(matching: ["打开", "Open"], pick: .bottom)
                _ = try await session.wait(Expectation(anyOf: ["All projects", "Add Project"], timeout: 12))
                try await Task.sleep(nanoseconds: 800_000_000)
                let frame = try await session.capture()
                // 切换工作空间图标是 Dock 右侧立方体，位于窗口宽 64.5%、高 94% 处。
                let point = CGPoint(
                    x: Double(frame.windowFrame.width) * 0.645,
                    y: Double(frame.windowFrame.height) * 0.94
                )
                try await session.hover(local: point, on: frame)
                let shown = try await session.capture()
                guard let tip = shown.texts.first(where: { $0.value.contains("Switch Workspace") }) else {
                    throw E2EError.product("悬浮 Dock 切换按钮后未出现「Switch Workspace」提示。可见：\(shown.summary)")
                }
                // 旧键位以字母 K 结尾；⌘⇧↑ 的符号 OCR 不稳定，因此只对旧值做负断言，符号由看图清单判定。
                let trimmed = tip.value.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !trimmed.hasSuffix("K"), !trimmed.hasSuffix("k") else {
                    throw E2EError.product("Dock 提示仍以 K 结尾，疑似旧键位 ⌘⇧K：\(trimmed)")
                }
                try session.save(shown, "\(label)-04-dock-tooltip.png")
                return trimmed
            }
            try await session.step("\(label)：/help 打开速查表") {
                let frame = try await session.capture()
                try await session.click(
                    local: CGPoint(
                        x: Double(frame.windowFrame.width) * 0.445,
                        y: Double(frame.windowFrame.height) * 0.94
                    ),
                    on: frame
                )
                try await session.type("/help")
                _ = try await session.wait(Expectation(anyOf: ["/help", "Help"], timeout: 6))
                try await Task.sleep(nanoseconds: 700_000_000)
                try await session.shortcut(key: 36, modifiers: [])
                _ = try await session.wait(Expectation(anyOf: ["Feature Guide", "Skip Guide", "Voice Group", "Agent Collaboration"], timeout: 8))
                // 引导从第 1 页开始，6 次下键到第 7 页（速查表）。
                for _ in 0..<6 {
                    try await session.shortcut(key: 125, modifiers: [])
                    try await Task.sleep(nanoseconds: 350_000_000)
                }
                _ = try await session.wait(Expectation(allOf: ["Get Started", "Cheat Sheet"], timeout: 6))
                try await Task.sleep(nanoseconds: 900_000_000)
                let sheet = try await session.capture()
                try expect(sheet, present: ["Cheat Sheet", "Workspaces"], absent: [], context: "速查表")
                try session.save(sheet, "\(label)-05-cheat-sheet.png")
                try await session.shortcut(key: 53, modifiers: [])
                return sheet.summary
            }
            session.stop()
        }
    }

    private static func expect(_ frame: Frame, present: [String], absent: [String], context: String) throws {
        for word in present where !frame.contains(word) {
            throw E2EError.product("\(context)：应出现「\(word)」。可见：\(frame.summary)")
        }
        for word in absent where frame.contains(word) {
            throw E2EError.product("\(context)：不应出现「\(word)」，说明打开了错误的视图。可见：\(frame.summary)")
        }
    }
}
