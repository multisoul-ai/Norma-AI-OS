import E2EKit

@MainActor
public enum SettingsModule {
    public static let manifest = DesktopModule(
        name: "settings",
        scenarioNames: [
            SettingsThemeConsistency.name,
            SettingsNavigation.name,
            SettingsBasicPreferences.name,
            SettingsResponsiveKeyboard.name,
            SettingsSpaceWallpapers.name,
            SettingsLiveConfiguration.name,
            SettingsA1Configuration.name,
            SettingsShortcutCustomize.name,
            SettingsShortcutDefaults.name,
            SettingsOrchestrationPathConvention.name,
            SettingsAgentsLibrary.name,
            SettingsDispatchEffects.name,
            SettingsRunEffects.name,
        ],
        visualScenarioNames: [SettingsThemeConsistency.name],
        run: { scenario, session in
            switch scenario {
            case SettingsThemeConsistency.name:
                try await SettingsThemeConsistency.run(session)
            case SettingsNavigation.name:
                try await SettingsNavigation.run(session)
            case SettingsBasicPreferences.name:
                try await SettingsBasicPreferences.run(session)
            case SettingsResponsiveKeyboard.name:
                try await SettingsResponsiveKeyboard.run(session)
            case SettingsSpaceWallpapers.name:
                try await SettingsSpaceWallpapers.run(session)
            case SettingsA1Configuration.name:
                try await SettingsA1Configuration.run(session)
            case SettingsLiveConfiguration.name:
                try await SettingsLiveConfiguration.run(session)
            case SettingsShortcutCustomize.name:
                try await SettingsShortcutCustomize.run(session)
            case SettingsShortcutDefaults.name:
                try await SettingsShortcutDefaults.run(session)
            case SettingsOrchestrationPathConvention.name:
                try await SettingsOrchestrationPathConvention.run(session)
            case SettingsAgentsLibrary.name:
                try await SettingsAgentsLibrary.run(session)
            case SettingsDispatchEffects.name:
                try await SettingsDispatchEffects.run(session)
            case SettingsRunEffects.name:
                try await SettingsRunEffects.run(session)
            default:
                return false
            }
            return true
        }
    )
}
