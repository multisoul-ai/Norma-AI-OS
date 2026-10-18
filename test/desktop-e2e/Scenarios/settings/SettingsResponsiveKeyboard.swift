import E2EKit

@MainActor
public enum SettingsResponsiveKeyboard {
    public nonisolated static let name = "settings-responsive-keyboard"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("键盘进入主题并返回基础设置") {
            try await SettingsSupport.open(session)
            try await SettingsSupport.language(session, label: "English", saved: "en")
            // 关闭重开将焦点恢复到面板；Tab 两次依次到基础设置、主题。
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await session.wait(Expectation(anyOf: ["Create Workspace"], absent: ["Basic Settings"]))
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await session.wait(Expectation(allOf: ["Basic Settings", "Interface Language"]))
            try await session.shortcut(key: 48, modifiers: [])
            try await session.shortcut(key: 48, modifiers: [])
            try session.save(try await session.capture(), "01-themes-keyboard-focus.png")
            try await session.shortcut(key: 36, modifiers: [])
            let themes = try await session.wait(Expectation(allOf: ["Paper Blue", "Light"], timeout: 5))
            try session.save(themes, "02-themes-keyboard-enter.png")
            try await session.shortcut(key: 48, modifiers: [0x38])
            try await session.shortcut(key: 49, modifiers: [])
            let basic = try await session.wait(Expectation(allOf: ["Basic Settings", "Interface Language"]))
            try session.save(basic, "03-basic-keyboard-space.png")
            try await SettingsSupport.prove(session, key: SettingsSupport.languageKey, value: "en")
            return "Tab、Shift-Tab、Return 和 Space 可导航，语言偏好保持 en"
        }
        try await session.step("紧凑侧栏下控件可达") {
            // 720px 窗口的面板宽度受可用空间约束为 656px，落入 CompactSidebar。
            try await session.resize(width: 720, height: 760)
            let top = try await session.wait(Expectation(allOf: ["Basic Settings", "Interface Language"]))
            try session.save(top, "04-basic-compact-dark.png")
            try session.save(try await energyStatus(session), "05-compact-energy-dark.png")
            return "紧凑侧栏保留通用分组，语言与节能均可滚动到达"
        }
        try await session.step("顶部两级导航下控件可达") {
            // 560px 窗口使用 12px 单侧边距，面板宽 536px，小于 TopBar 的 580px 阈值。
            try await session.resize(width: 560, height: 760)
            try await session.scroll(xFraction: 0.7, yFraction: 0.65, deltaY: 1200)
            let top = try await session.wait(Expectation(allOf: ["General", "Basic Settings", "Interface Language"]))
            try session.save(top, "06-basic-topbar-dark.png")
            try session.save(try await energyStatus(session), "07-topbar-energy-dark.png")
            return "顶部保留分组和页面两级，节能状态可见"
        }
        try await session.step("浅色顶部导航") {
            // 复用已验证的侧栏尺寸；系统缩放窗口时可能限制 800px 高度。
            try await session.resize(width: 720, height: 760)
            _ = try await SettingsSupport.select(session, labels: ["Themes"], content: ["Paper Blue"])
            try await session.click(matching: ["Paper Blue"], pick: .right)
            try await SettingsSupport.prove(session, key: "appearance.theme.v1", value: "paper-blue")
            _ = try await SettingsSupport.basic(session)
            try await session.resize(width: 560, height: 760)
            let frame = try await session.wait(Expectation(allOf: ["General", "Basic Settings", "Interface Language"]))
            try session.save(frame, "08-basic-topbar-light.png")
            return "浅色小窗口中的分组、页面与控件文字可读"
        }
    }

    private static func energyStatus(_ session: Session) async throws -> Frame {
        let expected = Expectation(anyOf: ["Smoothest Response", "Energy Saver"], timeout: 3)
        for _ in 0..<5 {
            let frame = try await session.capture()
            if frame.satisfies(expected) { return frame }
            try await session.scroll(xFraction: 0.7, yFraction: 0.65, deltaY: -180)
        }
        return try await session.wait(expected)
    }
}
