import E2EKit

@MainActor
public enum SettingsModule {
    public static let manifest = DesktopModule(
        name: "settings",
        scenarioNames: [
            SettingsNavigation.name,
            SettingsBasicPreferences.name,
            SettingsResponsiveKeyboard.name,
            SettingsSpaceWallpapers.name,
            SettingsLiveConfiguration.name,
            SettingsShortcutCustomize.name,
            SettingsOrchestrationPathConvention.name,
            SettingsAgentsLibrary.name,
        ],
        run: { scenario, session in
            switch scenario {
            case SettingsNavigation.name:
                try await SettingsNavigation.run(session)
            case SettingsBasicPreferences.name:
                try await SettingsBasicPreferences.run(session)
            case SettingsResponsiveKeyboard.name:
                try await SettingsResponsiveKeyboard.run(session)
            case SettingsSpaceWallpapers.name:
                try await SettingsSpaceWallpapers.run(session)
            case SettingsLiveConfiguration.name:
                try await SettingsLiveConfiguration.run(session)
            case SettingsShortcutCustomize.name:
                try await SettingsShortcutCustomize.run(session)
            case SettingsOrchestrationPathConvention.name:
                try await SettingsOrchestrationPathConvention.run(session)
            case SettingsAgentsLibrary.name:
                try await SettingsAgentsLibrary.run(session)
            default:
                return false
            }
            return true
        }
    )
}
