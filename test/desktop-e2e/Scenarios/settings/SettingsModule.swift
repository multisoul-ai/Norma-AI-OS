import E2EKit

@MainActor
public enum SettingsModule {
    public static let manifest = DesktopModule(
        name: "settings",
        scenarioNames: [SettingsLiveConfiguration.name],
        run: { name, session in
            switch name {
            case SettingsLiveConfiguration.name:
                try await SettingsLiveConfiguration.run(session)
                return true
            default: return false
            }
        }
    )
}
