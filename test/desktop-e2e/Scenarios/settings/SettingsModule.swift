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
            SettingsGlobalAppearance.name,
            SettingsLiveConfiguration.name,
            SettingsLiveI18n.name,
            SettingsA1Configuration.name,
            SettingsA1RuntimeDownload.name,
            SettingsVoiceCommandsPrototype.name,
            SettingsVoiceCommandsPrototype.sectionsName,
            SettingsVoiceCapabilities.a1Name,
            SettingsA1InstalledActions.name,
            SettingsVoiceCapabilities.jevName,
            SettingsVoiceCapabilities.errorsName,
            SettingsShortcutCustomize.name,
            SettingsShortcutDefaults.name,
            SettingsOrchestrationPathConvention.name,
            SettingsGrokOrchestration.name,
            SettingsAgentsLibrary.name,
            SettingsDispatchEffects.name,
            SettingsRunEffects.name,
        ],
        visualScenarioNames: [SettingsA1RuntimeDownload.name, SettingsThemeConsistency.name, SettingsLiveConfiguration.name, SettingsLiveI18n.name, SettingsA1Configuration.name, SettingsVoiceCommandsPrototype.name, SettingsVoiceCommandsPrototype.sectionsName, SettingsVoiceCapabilities.a1Name, SettingsA1InstalledActions.name, SettingsVoiceCapabilities.jevName, SettingsVoiceCapabilities.errorsName],
        run: { scenario, session in
            switch scenario {
            case SettingsA1RuntimeDownload.name:
                try await SettingsA1RuntimeDownload.run(session)
            case SettingsA1InstalledActions.name:
                try await SettingsA1InstalledActions.run(session)
            case SettingsVoiceCapabilities.errorsName:
                try await SettingsVoiceCapabilities.runEnglishErrors(session)
            case SettingsVoiceCapabilities.a1Name:
                try await SettingsVoiceCapabilities.runA1(session)
            case SettingsVoiceCapabilities.jevName:
                try await SettingsVoiceCapabilities.runJev(session)
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
            case SettingsGlobalAppearance.name:
                try await SettingsGlobalAppearance.run(session)
            case SettingsA1Configuration.name:
                try await SettingsA1Configuration.run(session)
            case SettingsVoiceCommandsPrototype.sectionsName:
                try await SettingsVoiceCommandsPrototype.runSections(session)
            case SettingsVoiceCommandsPrototype.name:
                try await SettingsVoiceCommandsPrototype.run(session)
            case SettingsLiveConfiguration.name:
                try await SettingsLiveConfiguration.run(session)
            case SettingsLiveI18n.name:
                try await SettingsLiveI18n.run(session)
            case SettingsShortcutCustomize.name:
                try await SettingsShortcutCustomize.run(session)
            case SettingsShortcutDefaults.name:
                try await SettingsShortcutDefaults.run(session)
            case SettingsOrchestrationPathConvention.name:
                try await SettingsOrchestrationPathConvention.run(session)
            case SettingsGrokOrchestration.name:
                try await SettingsGrokOrchestration.run(session)
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
