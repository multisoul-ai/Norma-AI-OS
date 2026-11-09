import E2EKit

@MainActor
public enum IslandModule {
    public static let manifest = DesktopModule(
        name: "island",
        scenarioNames: [IslandWindow.name, IslandApprovals.name, IslandApprovals.rememberedName, IslandApprovals.fallbackName, IslandQuestions.name, IslandHookQuestions.name, IslandHookProviders.name, IslandArtifacts.name, IslandNavigation.name, IslandMigration.name, IslandSettings.name, IslandStaff.name, IslandStaff.realName, IslandHotkey.name, IslandRedesign.name],
        visualScenarioNames: [IslandWindow.name, IslandApprovals.name, IslandApprovals.rememberedName, IslandApprovals.fallbackName, IslandQuestions.name, IslandHookQuestions.name, IslandHookProviders.name, IslandArtifacts.name, IslandNavigation.name, IslandMigration.name, IslandSettings.name, IslandStaff.name, IslandStaff.realName, IslandHotkey.name, IslandRedesign.name],
        run: { scenario, session in
            switch scenario {
            case IslandHotkey.name: try await IslandHotkey.run(session); return true
            case IslandStaff.name: try await IslandStaff.run(session, real: false); return true
            case IslandStaff.realName: try await IslandStaff.run(session, real: true); return true
            case IslandSettings.name: try await IslandSettings.run(session); return true
            case IslandMigration.name: try await IslandMigration.run(session); return true
            case IslandWindow.name: try await IslandWindow.run(session); return true
            case IslandApprovals.name: try await IslandApprovals.run(session, remembered: false); return true
            case IslandApprovals.rememberedName: try await IslandApprovals.run(session, remembered: true); return true
            case IslandApprovals.fallbackName: try await IslandApprovals.run(session, remembered: false, fallback: true); return true
            case IslandQuestions.name: try await IslandQuestions.run(session); return true
            case IslandHookQuestions.name: try await IslandHookQuestions.run(session); return true
            case IslandHookProviders.name: try await IslandHookProviders.run(session); return true
            case IslandArtifacts.name: try await IslandArtifacts.run(session); return true
            case IslandNavigation.name: try await IslandNavigation.run(session); return true
            case IslandRedesign.name: try await IslandRedesign.run(session); return true
            default: return false
            }
        }
    )
}
