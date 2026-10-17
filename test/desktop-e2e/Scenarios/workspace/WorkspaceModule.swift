import E2EKit

@MainActor
public enum WorkspaceModule {
    public static let manifest = DesktopModule(
        name: "workspace",
        scenarioNames: [WorkspaceLifecycle.name],
        run: { scenario, session in
            switch scenario {
            case WorkspaceLifecycle.name:
                try await WorkspaceLifecycle.run(session)
                return true
            default:
                return false
            }
        }
    )
}
