import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 旧覆盖迁移、两个真实窗口的即时同步、重启不重新导入旧值。
/// SQLite 只用于启动前构造与只读核对；显示结果必须由真实窗口截图确认。
@MainActor
public enum SettingsGlobalAppearance {
    public nonisolated static let name = "settings-global-appearance"

    public static func run(_ session: Session) async throws {
        var project: URL?
        var firstWindow: CGWindowID = 0
        defer {
            session.stop()
            if let project { try? FileManager.default.removeItem(at: project) }
        }
        try await session.step("安装旧空间与展开项目的外观覆盖") {
            try session.launch()
            try await session.resize(width: 1280, height: 800)
            _ = try await session.wait(Expectation(anyOf: ["Create Workspace", "创建工作空间"], timeout: 20))
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            project = try GlobalAppearanceFixture.install(dataRoot: dataRoot)
            try session.launch()
            try await session.resize(width: 1280, height: 800)
            _ = try await session.wait(Expectation(allOf: ["Legacy Appearance"], timeout: 20))
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await session.wait(Expectation(anyOf: ["Communication Style", "Basic Settings"]))
            try await SettingsSupport.prove(session, key: "appearance.global_migrated.v1", value: "1")
            try await SettingsSupport.prove(session, key: "wallpaper.value", value: "wallpapers/norma-violet-nebula.png")
            try await expectEffect(session, style: "fireworks", voice: "lightning")
            return "旧项目紫色星云与烟花/语音闪电迁移为全局偏好"
        }
        try await session.step("真实页面显示迁移值且无范围与跟随控件") {
            _ = try await SettingsSupport.select(session, labels: ["Wallpaper"], content: ["Current wallpaper"])
            let wallpaper = try await top(session, all: ["Violet Nebula", "Applies globally"])
            try session.save(wallpaper, "01-migrated-wallpaper.png")
            _ = try await SettingsSupport.select(session, labels: ["Effects"], content: ["Dispatch effects"])
            let effect = try await top(session, all: ["Fireworks", "Applies globally"])
            try session.save(effect, "02-migrated-effects.png")
            firstWindow = effect.windowID
            return "真实当前预览为紫色星云/烟花，范围与跟随开关消失"
        }
        try await session.step("保留原窗口，在第二窗口修改全局偏好") {
            try await session.shortcut(key: 53, modifiers: [])
            try await session.resize(width: 900, height: 760)
            try await session.shortcut(key: 45, modifiers: [0x37, 0x38])
            let deadline = Date().addingTimeInterval(10)
            var second = try await session.capture()
            while second.windowID == firstWindow, Date() < deadline {
                try await Task.sleep(nanoseconds: 150_000_000)
                second = try await session.capture()
            }
            guard second.windowID != firstWindow else { throw E2EError.product("新建窗口没有产生第二个真实窗口") }
            try session.save(second, "03-second-window.png")
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await session.wait(Expectation(anyOf: ["Communication Style", "Basic Settings"]))
            _ = try await SettingsSupport.select(session, labels: ["Wallpaper"], content: ["Current wallpaper"])
            try await pick(session, label: "Blue Horizon", effect: false)
            try await SettingsSupport.prove(session, key: "wallpaper.value", value: "wallpapers/norma-blue-horizon.png")
            _ = try await SettingsSupport.select(session, labels: ["Effects"], content: ["Dispatch effects"])
            try await pick(session, label: "Voice ribbon", effect: true)
            try await expectEffect(session, style: "ribbon", voice: "lightning")
            try session.save(try await top(session, all: ["Voice ribbon", "Applies globally"]), "04-second-window-saved.png")
            // 原生红色关闭按钮属于窗口框架，须经 AppKit 输入；只关闭当前第二窗口。
            let oldInput = ProcessInfo.processInfo.environment["NORMA_E2E_INPUT"]
            setenv("NORMA_E2E_INPUT", "ns", 1)
            defer { if let oldInput { setenv("NORMA_E2E_INPUT", oldInput, 1) } else { unsetenv("NORMA_E2E_INPUT") } }
            let closing = try await session.capture()
            guard closing.windowID == second.windowID else { throw E2EError.product("关闭前窗口身份发生变化") }
            try await session.click(local: CGPoint(x: 14, y: 14), on: closing)
            return "第二窗口已保存蓝色地平线与声纹，原窗口始终存活"
        }
        try await session.step("原窗口无需重启即可显示另一窗口的新值") {
            let original = try await session.capture()
            guard original.windowID == firstWindow else { throw E2EError.product("未返回原始窗口，不能证明同步") }
            try await session.resize(width: 1280, height: 800)
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await SettingsSupport.select(session, labels: ["Effects"], content: ["Dispatch effects"])
            try session.save(try await top(session, all: ["Voice ribbon", "Applies globally"]), "05-first-window-synced-effects.png")
            _ = try await SettingsSupport.select(session, labels: ["Wallpaper"], content: ["Current wallpaper"])
            try session.save(try await top(session, all: ["Blue Horizon", "Applies globally"]), "06-first-window-synced-wallpaper.png")
            return "同一 PID、原 windowID 的真实预览同步；没有重启或重建原窗口"
        }
        try await session.step("重启保留新全局值，不复活旧项目覆盖") {
            session.stop()
            try session.launch()
            try await session.resize(width: 1280, height: 800)
            _ = try await session.wait(Expectation(allOf: ["Legacy Appearance"], timeout: 20))
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await SettingsSupport.select(session, labels: ["Wallpaper"], content: ["Current wallpaper"])
            try session.save(try await top(session, all: ["Blue Horizon", "Applies globally"]), "07-restored-global-wallpaper.png")
            _ = try await SettingsSupport.select(session, labels: ["Effects"], content: ["Dispatch effects"])
            try session.save(try await top(session, all: ["Voice ribbon", "Applies globally"]), "08-restored-global-effects.png")
            try await expectEffect(session, style: "ribbon", voice: "lightning")
            return "重启仍为蓝色地平线与声纹，语音闪电覆盖保留"
        }
    }

    private static func top(_ session: Session, all: [String]) async throws -> Frame {
        try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 3000)
        return try await session.wait(Expectation(allOf: all, absent: ["Follow workspace", "Only this project", "Personalize"]))
    }

    private static func pick(_ session: Session, label: String, effect: Bool) async throws {
        try await session.scroll(xFraction: 0.7, yFraction: 0.5, deltaY: 3000)
        for _ in 0..<12 {
            let frame = try await session.capture()
            if let item = frame.pick([label], .bottom), item.center.x > frame.windowFrame.width * 0.4 {
                try await session.click(local: CGPoint(x: item.center.x, y: item.center.y - (effect ? 40 : 0)), on: frame)
                return
            }
            try await session.scroll(xFraction: 0.7, yFraction: 0.6, deltaY: -150)
        }
        throw E2EError.product("找不到素材 \(label)")
    }

    private static func expectEffect(_ session: Session, style: String, voice: String) async throws {
        let deadline = Date().addingTimeInterval(5)
        while Date() < deadline {
            if let raw = try session.setting("effects.dispatch.v1"),
               let value = try JSONSerialization.jsonObject(with: Data(raw.utf8)) as? [String: String],
               value["style"] == style, value["voice"] == voice { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw E2EError.product("全局特效未保存 style=\(style), voice=\(voice)")
    }
}
