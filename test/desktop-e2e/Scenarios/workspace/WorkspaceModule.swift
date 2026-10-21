import E2EKit

@MainActor
public enum WorkspaceModule {
    public static let manifest = DesktopModule(
        name: "workspace",
        scenarioNames: [
            WorkspaceLifecycle.name,
            WorkspaceDrawerTabs.name,
            WorkspaceRunOverview.name,
            WorkspaceStatusCapsule.name,
            WorkspaceRunOverviewAgents.name,
            WorkspaceOverviewFloat.name,
            WorkspaceTitlebarBrand.name,
            WorkspaceIdeas.name,
        ],
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
            case WorkspaceRunOverviewAgents.name:
                try await WorkspaceRunOverviewAgents.run(session)
                return true
            case WorkspaceStatusCapsule.name:
                try await WorkspaceStatusCapsule.run(session)
                return true
            case WorkspaceOverviewFloat.name:
                try await WorkspaceOverviewFloat.run(session)
                return true
            case WorkspaceTitlebarBrand.name:
                try await WorkspaceTitlebarBrand.run(session)
                return true
            case WorkspaceIdeas.name:
                try await WorkspaceIdeas.run(session)
                return true
            default:
                return false
            }
        }
    )
}
