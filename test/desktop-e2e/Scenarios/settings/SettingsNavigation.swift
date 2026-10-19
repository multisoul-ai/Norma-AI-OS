import E2EKit

@MainActor
public enum SettingsNavigation {
    public nonisolated static let name = "settings-navigation"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("首次打开仍进入人格") {
            try await SettingsSupport.open(session)
            try session.save(try await session.capture(), "01-persona-default.png")
            return "首次打开显示人格内容"
        }
        try await session.step("基础设置与节能同页") {
            _ = try await SettingsSupport.basic(session)
            try await SettingsSupport.language(session, label: "English", saved: "en")
            let frame = try await session.wait(Expectation(
                allOf: ["General", "Basic Settings", "Interface Language"],
                absent: ["Appearance"]
            ))
            try session.save(frame, "02-basic-dark.png")
            return "General 分组包含基础设置；没有独立 Appearance 分组"
        }
        let pages: [(String, [String], [String])] = [
            ("themes", ["Themes"], ["Paper Blue"]),
            ("wallpaper", ["Wallpaper"], ["Wallpaper Library"]),
            ("persona", ["Persona"], ["Communication Style"]),
            ("memory", ["Memory"], ["Review and organize"]),
            ("voice", ["Voice"], ["Let Norma hear you"]),
            ("keyboard", ["Keyboard Shortcuts"], ["Customize app commands"]),
            ("agents", ["Agents"], ["Create, organize", "Create, Organize", "Create, organize and"]),
            ("orchestration", ["Orchestration"], ["Install Skill + CLI"]),
            ("mobile", ["Mobile Connection"], ["Connect your iPhone"]),
        ]
        for (index, page) in pages.enumerated() {
            try await session.step("进入 \(page.0)") {
                let frame = try await SettingsSupport.select(session, labels: page.1, content: page.2)
                try session.save(frame, "\(index + 3)-\(page.0).png")
                return "导航目的地显示对应页面内容"
            }
        }
        try await session.step("浅色主题下基础设置") {
            _ = try await SettingsSupport.select(session, labels: ["Themes"], content: ["Paper Blue"])
            try await session.click(matching: ["Paper Blue"], pick: .right)
            try await SettingsSupport.prove(session, key: "appearance.theme.v1", value: "paper-blue")
            let frame = try await SettingsSupport.basic(session)
            try session.save(frame, "12-basic-light.png")
            return "Paper Blue 已应用，基础设置布局可读"
        }
    }
}
