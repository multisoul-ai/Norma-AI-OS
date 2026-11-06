import E2EKit

@MainActor
public enum IslandModule {
    public static let manifest = DesktopModule(
        name: "island",
        scenarioNames: [IslandWindow.name, IslandApprovals.name, IslandApprovals.rememberedName, IslandApprovals.fallbackName, IslandQuestions.name, IslandArtifacts.name, IslandNavigation.name, IslandMigration.name],
        visualScenarioNames: [IslandWindow.name, IslandApprovals.name, IslandApprovals.rememberedName, IslandApprovals.fallbackName, IslandQuestions.name, IslandArtifacts.name, IslandNavigation.name, IslandMigration.name],
        run: { scenario, session in
            switch scenario {
            case IslandMigration.name: try await IslandMigration.run(session); return true
            case IslandWindow.name: try await IslandWindow.run(session); return true
            case IslandApprovals.name: try await IslandApprovals.run(session, remembered: false); return true
            case IslandApprovals.rememberedName: try await IslandApprovals.run(session, remembered: true); return true
            case IslandApprovals.fallbackName: try await IslandApprovals.run(session, remembered: false, fallback: true); return true
            case IslandQuestions.name: try await IslandQuestions.run(session); return true
            case IslandArtifacts.name: try await IslandArtifacts.run(session); return true
            case IslandNavigation.name: try await IslandNavigation.run(session); return true
            default: return false
            }
        }
    )
}
