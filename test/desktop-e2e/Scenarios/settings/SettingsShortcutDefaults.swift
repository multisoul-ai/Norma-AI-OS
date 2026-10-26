import E2EKit
import Foundation

/// 切换工作空间 / 全部工作空间键位互换后，Settings 的默认显示与既有自定义键位。
@MainActor
public enum SettingsShortcutDefaults {
    public nonisolated static let name = "settings-shortcut-defaults"
    static let prefsKey = "keyboard.shortcuts.v1"
    static let directoryMarker = "Continue working"
    static let switcherMarker = "A canvas for your projects"
    /// 旧版本用户的自定义：切换工作空间改为 ⌃⌥J（释放旧默认 ⇧⌘K），新建浏览器占用 ⇧⌘K。
    static let customPrefs = #"{"overrides":[{"command":"switch_workspace","keystroke":"ctrl-alt-j"},{"command":"new_browser","keystroke":"cmd-shift-k"}]}"#
    static let themes = [("paper-blue", "light"), ("sapphire-night", "dark")]

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
            return "已写入 Alpha、Beta，未写入任何快捷键覆盖"
        }
        for (theme, label) in themes {
            try await session.step("\(label)：默认设置页显示 ⇧⌘↑ / ⇧⌘K") {
                try Catalog.seedSetting(database: try session.databaseURL(), key: "appearance.theme.v1", value: theme)
                try await openShortcuts(session)
                let switchRow = try await rowFrame(session, title: "Switch Workspace")
                try session.save(switchRow, "\(label)-01-default-switch-workspace.png")
                let allRow = try await rowFrame(session, title: "All Workspaces")
                try session.save(allRow, "\(label)-02-default-all-workspaces.png")
                guard try session.setting(prefsKey) == nil else {
                    throw E2EError.product("默认路径不应写入 \(prefsKey)，实际 \(try session.setting(prefsKey) ?? "nil")")
                }
                // 未修改的默认行没有 Restore 按钮；键帽符号 OCR 不稳定，由看图清单判定。
                guard !rowHasRestore(switchRow, title: "Switch Workspace") else {
                    throw E2EError.product("默认状态下切换工作空间行不应显示 Restore。可见：\(switchRow.summary)")
                }
                session.stop()
                return "两行已截图，偏好未被写入"
            }
        }
        try Catalog.seedSetting(database: try session.databaseURL(), key: prefsKey, value: customPrefs)
        for (theme, label) in themes {
            try await session.step("\(label)：既有自定义键位保持并仍然生效") {
                try Catalog.seedSetting(database: try session.databaseURL(), key: "appearance.theme.v1", value: theme)
                try await openShortcuts(session)
                let row = try await rowFrame(session, title: "Switch Workspace")
                guard rowHasRestore(row, title: "Switch Workspace") else {
                    throw E2EError.product("自定义后切换工作空间行应显示 Restore。可见：\(row.summary)")
                }
                try session.save(row, "\(label)-03-custom-switch-workspace.png")
                try await session.shortcut(key: 43, modifiers: [0x37])
                _ = try await session.wait(Expectation(allOf: [directoryMarker], timeout: 8))

                try await session.shortcut(key: 126, modifiers: [0x37, 0x38])
                try await Task.sleep(nanoseconds: 800_000_000)
                let stay = try await session.capture()
                try expect(stay, present: [directoryMarker], absent: [switcherMarker], context: "⇧⌘↑ 已被自定义让出")

                try await session.shortcut(key: 38, modifiers: [0x3B, 0x3A])
                _ = try await session.wait(Expectation(allOf: [switcherMarker], timeout: 6))
                try await Task.sleep(nanoseconds: 700_000_000)
                let switcher = try await session.capture()
                try expect(switcher, present: [switcherMarker], absent: [directoryMarker], context: "⌃⌥J")
                try session.save(switcher, "\(label)-04-custom-switcher.png")

                try await session.shortcut(key: 40, modifiers: [0x37, 0x38])
                try await Task.sleep(nanoseconds: 800_000_000)
                let kept = try await session.capture()
                try expect(kept, present: [switcherMarker], absent: [directoryMarker], context: "⇧⌘K 属于自定义的新建浏览器")
                try session.save(kept, "\(label)-05-custom-k-not-directory.png")

                guard try session.setting(prefsKey) == customPrefs else {
                    throw E2EError.product("已有自定义被改写：\(try session.setting(prefsKey) ?? "nil")")
                }
                session.stop()
                return "⌃⌥J 打开切换器，⇧⌘↑ 与 ⇧⌘K 不触发默认行为，偏好原样保留"
            }
        }
    }

    private static func openShortcuts(_ session: Session) async throws {
        try session.launch(language: "en")
        _ = try await session.wait(Expectation(allOf: ["Beta", directoryMarker], timeout: 25))
        try await session.shortcut(key: 43, modifiers: [0x37])
        _ = try await session.wait(Expectation(anyOf: ["Communication Style"], timeout: 10))
        _ = try await SettingsSupport.select(
            session,
            labels: ["Keyboard Shortcuts"],
            content: ["Next Workspace", "Save File"])
    }

    private static func rowFrame(_ session: Session, title: String) async throws -> Frame {
        for step in 0..<14 {
            let frame = try await session.capture()
            if frame.pick([title], .right) != nil { return frame }
            try await session.scroll(xFraction: 0.7, yFraction: 0.65, deltaY: step < 7 ? -160 : 160)
        }
        throw E2EError.product("快捷键页面找不到 \(title) 行")
    }

    /// 只看与行标题同一水平带里的「Restore」，避开页头的「Restore Defaults」按钮。
    private static func rowHasRestore(_ frame: Frame, title: String) -> Bool {
        guard let label = frame.pick([title], .right) else { return false }
        return frame.texts.contains { text in
            text.value.localizedCaseInsensitiveContains("Restore")
                && !text.value.localizedCaseInsensitiveContains("Defaults")
                && abs(text.center.y - label.center.y) < 28
        }
    }

    private static func expect(_ frame: Frame, present: [String], absent: [String], context: String) throws {
        for word in present where !frame.contains(word) {
            throw E2EError.product("\(context)：应出现「\(word)」。可见：\(frame.summary)")
        }
        for word in absent where frame.contains(word) {
            throw E2EError.product("\(context)：不应出现「\(word)」。可见：\(frame.summary)")
        }
    }
}
