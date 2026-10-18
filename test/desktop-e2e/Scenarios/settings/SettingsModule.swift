import E2EKit

@MainActor
public enum SettingsModule {
    public static let manifest = DesktopModule(
        name: "settings",
        scenarioNames: [SettingsNavigation.name, SettingsBasicPreferences.name, SettingsResponsiveKeyboard.name],
        run: { scenario, session in
            switch scenario {
            case SettingsNavigation.name:
                try await SettingsNavigation.run(session)
            case SettingsBasicPreferences.name:
                try await SettingsBasicPreferences.run(session)
            case SettingsResponsiveKeyboard.name:
                try await SettingsResponsiveKeyboard.run(session)
            default:
                return false
            }
            return true
        }
    )
}
