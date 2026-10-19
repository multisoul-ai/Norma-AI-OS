import E2EKit

@MainActor
public enum WorkspaceModule {
    public static let manifest = DesktopModule(
        name: "workspace",
        scenarioNames: [WorkspaceLifecycle.name, WorkspaceDrawerTabs.name, WorkspaceRunOverview.name],
        run: { scenario, session in
            switch scenario {
            case WorkspaceLifecycle.name:
                try await WorkspaceLifecycle.run(session)
                return true
            case WorkspaceDrawerTabs.name:
                try await WorkspaceDrawerTabs.run(session)
                return true
            case WorkspaceRunOverview.name:
                try await WorkspaceRunOverview.run(session)
                return true
            default:
                return false
            }
        }
    )
}
