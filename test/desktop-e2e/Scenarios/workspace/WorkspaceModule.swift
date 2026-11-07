import E2EKit

@MainActor
public enum WorkspaceModule {
    public static let manifest = DesktopModule(
        name: "workspace",
        scenarioNames: [
            WorkspaceLifecycle.name,
            WorkspaceHomeDesign.name,
            WorkspaceHomePreview.name,
            WorkspaceTitlebarBrand.name,
            WorkspaceFeatureGuide.name,
            WorkspaceHandoffEffects.name,
            WorkspaceSwitchShortcuts.name,
        ],
        visualScenarioNames: [],
        run: { scenario, session in
            switch scenario {
            case WorkspaceHomePreview.name:
                try await WorkspaceHomePreview.run(session)
                return true
            case WorkspaceHomeDesign.name:
                try await WorkspaceHomeDesign.run(session)
                return true
            case WorkspaceLifecycle.name:
                try await WorkspaceLifecycle.run(session)
                return true
            case WorkspaceTitlebarBrand.name:
                try await WorkspaceTitlebarBrand.run(session)
                return true
            case WorkspaceHandoffEffects.name:
                try await WorkspaceHandoffEffects.run(session)
                return true
            case WorkspaceSwitchShortcuts.name:
                try await WorkspaceSwitchShortcuts.run(session)
                return true
            case WorkspaceFeatureGuide.name:
                try await WorkspaceFeatureGuide.run(session)
                return true
            default:
                return false
            }
        }
    )
}
