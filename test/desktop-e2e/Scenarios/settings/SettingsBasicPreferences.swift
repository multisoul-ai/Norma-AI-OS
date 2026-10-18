import E2EKit

@MainActor
public enum SettingsBasicPreferences {
    public nonisolated static let name = "settings-basic-preferences"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("语言立即生效") {
            try await SettingsSupport.open(session)
            try await SettingsSupport.language(session, label: "English", saved: "en")
            let frame = try await session.wait(Expectation(allOf: ["Basic Settings", "Interface Language"]))
            try session.save(frame, "01-language-english.png")
            return "语言键已保存为 en，仍停留在基础设置"
        }
        try await session.step("开启节能") {
            let frame = try await SettingsSupport.energy(session, enabled: true)
            try await SettingsSupport.prove(session, key: SettingsSupport.languageKey, value: "en")
            try session.save(frame, "02-energy-enabled.png")
            return "节能键为 true，语言键仍为 en"
        }
        try await session.step("切换语言保留节能") {
            try await SettingsSupport.language(session, label: "简体中文", saved: "zh-CN")
            try await SettingsSupport.prove(session, key: SettingsSupport.energyKey, value: "true")
            let frame = try await session.wait(Expectation(allOf: ["基础设置", "界面语言"]))
            try session.save(frame, "03-language-chinese.png")
            return "中文立即生效，节能键仍为 true"
        }
        try await session.step("关闭重开保留当前页") {
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间"], absent: ["基础设置"]))
            try await session.shortcut(key: 43, modifiers: [0x37])
            let frame = try await session.wait(Expectation(allOf: ["基础设置", "界面语言"]))
            try session.save(frame, "04-reopened-basic.png")
            return "重开设置继续显示基础设置"
        }
        try await session.step("重启恢复语言与节能") {
            session.stop()
            try await SettingsSupport.open(session)
            _ = try await SettingsSupport.basic(session)
            try await SettingsSupport.prove(session, key: SettingsSupport.languageKey, value: "zh-CN")
            try await SettingsSupport.prove(session, key: SettingsSupport.energyKey, value: "true")
            let frame = try await session.wait(Expectation(allOf: ["基础设置", "界面语言"]))
            try session.save(frame, "05-restored-basic.png")
            return "同一隔离数据库恢复 zh-CN 与 true"
        }
        try await session.step("关闭节能独立保存") {
            let frame = try await SettingsSupport.energy(session, enabled: false)
            try await SettingsSupport.prove(session, key: SettingsSupport.languageKey, value: "zh-CN")
            try session.save(frame, "06-energy-disabled.png")
            return "节能键为 false，中文语言保持不变"
        }
    }
}
