import E2EKit

@MainActor
public enum WorkspaceModule {
    public static let manifest = DesktopModule(
        name: "workspace",
        scenarioNames: [
            WorkspaceLifecycle.name,
            WorkspaceDrawerTabs.name,
            WorkspaceOverviewFloat.name,
        ],
        run: { scenario, session in
            switch scenario {
            case WorkspaceLifecycle.name:
                try await WorkspaceLifecycle.run(session)
                return true
            case WorkspaceDrawerTabs.name:
                try await WorkspaceDrawerTabs.run(session)
                return true
            case WorkspaceOverviewFloat.name:
                try await WorkspaceOverviewFloat.run(session)
                return true
            default:
                return false
            }
        }
    )
}
