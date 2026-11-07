import E2EKit
import Foundation

/// 真实系统热键：别的 App 在前台时，由 System Events 发出 ⌃⌥⌘Space（系统级输入，不是进程内注入），
/// 岛应展开，再按一次收起。结果以原生窗口高度判断，并用看图核对展开形态。
@MainActor
public enum IslandHotkey {
    public nonisolated static let name = "island-hotkey"

    private static func osascript(_ script: String) throws -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        process.arguments = ["-e", script]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        try process.run()
        process.waitUntilExit()
        let output = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        guard process.terminationStatus == 0 else { throw E2EError.env("osascript 失败（需要辅助功能授权）：\(output)") }
        return output.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func frontmost() throws -> String {
        try osascript(#"tell application "System Events" to get name of first process whose frontmost is true"#)
    }

    private static func pressHotkey() throws {
        _ = try osascript(#"tell application "System Events" to key code 49 using {control down, option down, command down}"#)
    }

    public static func run(_ session: Session) async throws {
        defer { session.stop(); unsetenv("NORMA_E2E_NO_ACTIVATE") }
        setenv("NORMA_E2E_NO_ACTIVATE", "1", 1)
        var island = 0
        try await session.step("启动真实 App，岛为收起胶囊") {
            try session.launch(language: "zh-Hans")
            _ = try await session.capturePixels()
            guard let window = try await session.nativeWindows().first(where: { $0.width >= 300 && $0.width < 700 && $0.height <= 60 }) else {
                throw E2EError.product("没有收起的岛窗口")
            }
            island = window.number
            return "岛窗口号 \(window.number)"
        }
        var before = ""
        try await session.step("确认前台是别的 App（非 Norma）") {
            // 用户正在使用的 App 保持前台即可；尝试 Finder 但不强求，只要不是 Norma。
            before = try frontmost()
            if before.lowercased().contains("norma") {
                _ = try osascript(#"tell application "Finder" to activate"#)
                try await Task.sleep(nanoseconds: 800_000_000)
                before = try frontmost()
            }
            guard !before.lowercased().contains("norma") else { throw E2EError.env("前台仍是 Norma，无法验证“别的 App 在前台”：\(before)") }
            return "前台 App：\(before)"
        }
        try await session.step("系统热键展开岛，前台 App 不变") {
            try pressHotkey()
            var expanded = false
            for _ in 0..<40 {
                if try await session.nativeWindows().contains(where: { $0.number == island && $0.height > 100 }) { expanded = true; break }
                try await Task.sleep(nanoseconds: 100_000_000)
            }
            guard expanded else {
                throw E2EError.product("系统热键后岛未展开（可能热键注册失败或被其他 App 占用）")
            }
            let after = try frontmost()
            try await session.selectNativeWindow(island)
            try session.save(try await session.visualCheck([
                VisualCriterion(id: "expanded", text: "岛已展开为运行概览：顶部标题与三个页签，面板为黑色圆角；不是收起胶囊")
            ]), "01-hotkey-expanded.png")
            guard after == before else { throw E2EError.product("热键展开岛抢走了前台：\(before) → \(after)") }
            return "岛展开，前台仍是 \(after)"
        }
        try await session.step("再次按下热键收起岛") {
            try pressHotkey()
            var collapsed = false
            for _ in 0..<40 {
                if try await session.nativeWindows().contains(where: { $0.number == island && $0.height <= 60 }) { collapsed = true; break }
                try await Task.sleep(nanoseconds: 100_000_000)
            }
            guard collapsed else { throw E2EError.product("再次按热键后岛未收起") }
            return "岛已收起"
        }
    }
}
