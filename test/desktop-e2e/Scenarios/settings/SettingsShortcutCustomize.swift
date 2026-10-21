import CoreGraphics
import E2EKit
import Foundation

@MainActor
public enum SettingsShortcutCustomize {
    public nonisolated static let name = "settings-shortcut-customize"
    static let prefsKey = "keyboard.shortcuts.v1"
    static let rowTitle = ["Next Workspace", "下一个工作空间"]

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("点击快捷键进入录制态") {
            try await SettingsSupport.open(session)
            _ = try await SettingsSupport.select(
                session,
                labels: ["Keyboard Shortcuts", "键盘快捷键"],
                content: ["Next Workspace", "下一个工作空间", "Save File", "保存文件"])
            let frame = try await rowFrame(session)
            try session.save(frame, "01-shortcuts-page.png")
            try await clickKeycap(session, on: frame)
            // 键帽内的提示文字 OCR 不稳定：只留图供看图检查，硬判据是下一步的 SQLite 结果。
            try await Task.sleep(nanoseconds: 400_000_000)
            try session.save(try await session.capture(), "02-recording.png")
            return "已点击键帽，录制态截图留存"
        }
        try await session.step("按下新组合键后保存并显示已修改") {
            // key 17 = T，0x37 = Cmd，0x38 = Shift。
            try await session.shortcut(key: 17, modifiers: [0x37, 0x38])
            let deadline = Date().addingTimeInterval(5)
            var saved: String?
            while Date() < deadline {
                saved = try session.setting(prefsKey)
                if saved?.contains("cmd-shift-t") == true { break }
                try await Task.sleep(nanoseconds: 100_000_000)
            }
            guard saved?.contains("cmd-shift-t") == true else {
                throw E2EError.product("录制后 \(prefsKey) 应含 cmd-shift-t，实际 \(saved ?? "nil")")
            }
            // 已修改行才有恢复按钮；“Modified”徽标的 OCR 不稳定，改用按钮文字作判据。
            let frame = try await session.wait(Expectation(anyOf: ["Restore", "恢复"], timeout: 5))
            try session.save(frame, "03-saved.png")
            return "快捷键保存为 cmd-shift-t，行内出现恢复按钮"
        }
        try await session.step("冲突组合在该行内提示且不覆盖已有设置") {
            let frame = try await rowFrame(session)
            try await clickKeycap(session, on: frame)
            try await Task.sleep(nanoseconds: 400_000_000)
            // ⌘K 已被“聚焦指令输入”占用。
            try await session.shortcut(key: 40, modifiers: [0x37])
            try await Task.sleep(nanoseconds: 500_000_000)
            try session.save(try await session.capture(), "04-conflict-inline.png")
            let stored = try session.setting(prefsKey) ?? ""
            guard stored.contains("cmd-shift-t"), !stored.contains("cmd-k") else {
                throw E2EError.product("冲突录制不应改写已保存的快捷键，实际 \(stored)")
            }
            // Esc 取消录制，回到可恢复状态。
            try await session.shortcut(key: 53, modifiers: [])
            return "冲突后设置保持 cmd-shift-t，错误提示留图检查"
        }
        try await session.step("恢复默认") {
            try await session.click(matching: ["Restore", "恢复"], pick: .right)
            let restored = try await session.wait(Expectation(
                anyOf: ["Next Workspace", "下一个工作空间"], absent: ["Restore", "恢复"], timeout: 5))
            try session.save(restored, "05-restored.png")
            let stored = try session.setting(prefsKey) ?? ""
            guard !stored.contains("cmd-shift-t") else {
                throw E2EError.product("恢复后不应保留 cmd-shift-t，实际 \(stored)")
            }
            return "恢复后恢复按钮消失，覆盖项已清除"
        }
    }

    private static func rowFrame(_ session: Session) async throws -> Frame {
        for _ in 0..<8 {
            let frame = try await session.capture()
            if frame.pick(rowTitle, .right) != nil { return frame }
            try await session.scroll(xFraction: 0.7, yFraction: 0.65, deltaY: -160)
        }
        throw E2EError.product("快捷键页面找不到 \(rowTitle[0]) 行")
    }

    private static func clickKeycap(_ session: Session, on frame: Frame) async throws {
        guard let title = frame.pick(rowTitle, .right) else {
            throw E2EError.product("找不到 \(rowTitle[0]) 行")
        }
        let width = frame.windowFrame.width
        let panel = min(max(width * 0.7, 760), min(1280, width - 64))
        // 键帽右对齐：面板右缘 - 内容边距 20 - 行内边距 18 - 半个键帽 44。
        try await session.click(local: CGPoint(x: (width + panel) / 2 - 82, y: title.center.y), on: frame)
    }
}
